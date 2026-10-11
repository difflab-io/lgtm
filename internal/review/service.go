package review

import (
	"context"
	"fmt"
	"github.com/difflab-io/lgtm/internal/storage"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

type Service struct {
	mu              sync.Mutex
	root, workspace string
	store           storage.Store
	state           State
	github          GitHub
}

func New(root, database string, github GitHub) (*Service, error) {
	store, err := storage.Open(context.Background(), storage.Config{Driver: "sqlite", DSN: database})
	if err != nil {
		return nil, err
	}
	service, err := NewWithStore(root, "", store, github)
	if err != nil {
		store.Close()
	}
	return service, err
}
func NewWithStore(root, workspace string, store storage.Store, github GitHub) (*Service, error) {
	root, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	root, err = filepath.EvalSymlinks(root)
	if err != nil {
		return nil, err
	}
	if workspace == "" {
		workspace = root
	}
	return &Service{root: root, workspace: workspace, store: store, github: github, state: State{Reviews: []Review{}, Comments: []Comment{}}}, nil
}
func (s *Service) Close() error { return s.store.Close() }
func cloneState(state State) State {
	return State{Reviews: append([]Review{}, state.Reviews...), Comments: append([]Comment{}, state.Comments...)}
}
func (s *Service) find(id string) (*Review, error) {
	for i := range s.state.Reviews {
		if s.state.Reviews[i].ID == id {
			return &s.state.Reviews[i], nil
		}
	}
	return nil, missing("review not found")
}
func (s *Service) git(ctx context.Context, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", append([]string{"--literal-pathspecs", "-C", s.root}, args...)...)
	b, err := cmd.CombinedOutput()
	if err != nil {
		return "", upstream("git: %s: %w", b, err)
	}
	return string(b), nil
}
func (s *Service) filePath(path string) (string, error) {
	if path == "" || filepath.IsAbs(path) {
		return "", invalid("repository-relative path required")
	}
	full, err := filepath.EvalSymlinks(filepath.Join(s.root, path))
	if err != nil {
		return "", err
	}
	rel, err := filepath.Rel(s.root, full)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", invalid("path outside checkout")
	}
	return full, nil
}
func (s *Service) Execute(ctx context.Context, q Request) (Response, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	before := cloneState(s.state)
	readOnly := q.Op == "state" || q.Op == "diff" || q.Op == "file" || q.Op == "history" || q.Op == "files" || q.Op == "pulls"
	var response Response
	err := s.store.Transact(ctx, s.workspace, !readOnly, func(state *State) error {
		s.state = *state
		var err error
		response, err = s.execute(ctx, q)
		if err != nil {
			return err
		}
		*state = s.state
		response.State = cloneState(s.state)
		response.RepoPath = s.root
		return nil
	})
	if err != nil {
		s.state = before
		return Response{}, err
	}
	return response, nil
}
func (s *Service) execute(ctx context.Context, q Request) (Response, error) {
	out := Response{}
	switch q.Op {
	case "pulls":
		return s.openPulls(ctx)
	case "state":
	case "files":
		files, err := s.files(ctx, q)
		out.Files = files
		return out, err
	case "history":
		r, err := s.find(q.ID)
		if err != nil {
			return out, err
		}
		if r.Kind != "github" {
			return out, invalid("history requires GitHub review")
		}
		for _, endpoint := range []string{"/reviews", "/comments"} {
			type historyEntry struct {
				User struct {
					Login string `json:"login"`
				} `json:"user"`
				Body  string `json:"body"`
				Path  string `json:"path"`
				Line  int    `json:"line"`
				State string `json:"state"`
			}
			entries, err := githubList[historyEntry](ctx, s.github, prPath(*r)+endpoint)
			if err != nil {
				return out, err
			}
			for _, e := range entries {
				out.Text += fmt.Sprintf("%s · %s %s:%d\n%s\n\n", e.User.Login, e.State, e.Path, e.Line, e.Body)
			}
		}
	case "file":
		full, err := s.filePath(q.Path)
		if err != nil {
			return out, err
		}
		data, err := os.ReadFile(full)
		if err != nil {
			return out, err
		}
		out.Text = string(data)
	case "diff":
		if q.ID == "" {
			text, err := s.localDiff(ctx, q.Path)
			out.Text = text
			return out, err
		}
		r, err := s.find(q.ID)
		if err != nil {
			return out, err
		}
		if r.Kind == "local" {
			text, err := s.localDiff(ctx, q.Path)
			out.Text = text
			return out, err
		}
		files, err := githubList[githubFile](ctx, s.github, prPath(*r)+"/files")
		if err != nil {
			return out, err
		}
		for _, f := range files {
			if q.Path != "" && f.Filename != q.Path {
				continue
			}
			out.Text += "diff --git a/" + f.Filename + " b/" + f.Filename + "\n--- a/" + f.Filename + "\n+++ b/" + f.Filename + "\n" + f.Patch + "\n"
		}
	case "create":
		if err := validateActor(q.Actor); err != nil {
			return out, err
		}
		r := Review{ID: id(), Kind: q.Kind, Repo: q.Repo, PR: q.PR, Status: "draft", Body: q.Body, Actor: q.Actor}
		if r.Kind == "github" {
			if r.Repo == "" {
				var err error
				r.Repo, err = s.checkoutRepository(ctx)
				if err != nil {
					return out, err
				}
			}
			if !repoPattern.MatchString(r.Repo) || strings.Contains(r.Repo, "..") || strings.HasPrefix(r.Repo, "./") || strings.HasSuffix(r.Repo, "/.") || r.PR < 1 {
				return out, invalid("GitHub review requires owner/repo and positive PR")
			}
			var pr struct {
				Head struct {
					SHA string `json:"sha"`
				} `json:"head"`
			}
			if err := s.github.Call(ctx, "GET", prPath(r), nil, &pr); err != nil {
				return out, err
			}
			r.Head = pr.Head.SHA
		} else if r.Kind != "local" {
			return out, invalid("kind must be local or github")
		}
		s.state.Reviews = append(s.state.Reviews, r)
	case "comment":
		if err := validateActor(q.Actor); err != nil {
			return out, err
		}
		if strings.TrimSpace(q.Body) == "" || q.Line < 1 {
			return out, invalid("comment needs body and positive line")
		}
		q.Path = filepath.Clean(q.Path)
		if filepath.IsAbs(q.Path) || q.Path == ".." || strings.HasPrefix(q.Path, "../") {
			return out, invalid("repository-relative path required")
		}
		if _, err := s.filePath(q.Path); err != nil {
			remotePath := false
			if q.ReviewID != "" {
				r, findErr := s.find(q.ReviewID)
				if findErr != nil {
					return out, findErr
				}
				if r.Kind == "github" {
					files, fetchErr := githubList[githubFile](ctx, s.github, prPath(*r)+"/files")
					if fetchErr != nil {
						return out, fetchErr
					}
					for _, file := range files {
						if file.Filename == q.Path || (q.Side == "LEFT" && file.PreviousFilename == q.Path) {
							remotePath = true
							break
						}
					}
				}
			}
			if !remotePath {
				if q.Side != "LEFT" {
					return out, err
				}
				if _, oldErr := s.git(ctx, "show", "HEAD:"+q.Path); oldErr != nil {
					return out, err
				}
			}
		}
		if q.Side == "" {
			q.Side = "RIGHT"
		}
		if q.Side != "LEFT" && q.Side != "RIGHT" {
			return out, invalid("side must be LEFT or RIGHT")
		}
		if q.ReviewID != "" {
			r, err := s.find(q.ReviewID)
			if err != nil {
				return out, err
			}
			if r.Status != "draft" {
				return out, precondition("review already submitted")
			}
		}
		s.state.Comments = append(s.state.Comments, Comment{ID: id(), Path: q.Path, Line: q.Line, Side: q.Side, Body: q.Body, Actor: q.Actor, ReviewID: q.ReviewID})
	case "reply", "resolve":
		var root *Comment
		for i := range s.state.Comments {
			c := &s.state.Comments[i]
			if c.ID == q.ID && c.ThreadID == "" {
				root = c
				break
			}
		}
		if root == nil {
			return out, missing("thread not found")
		}
		if q.Op == "resolve" {
			root.Resolved = q.Event != "REOPEN"
			break
		}
		if err := validateActor(q.Actor); err != nil {
			return out, err
		}
		if strings.TrimSpace(q.Body) == "" {
			return out, invalid("reply body required")
		}
		if root.ReviewID != "" {
			r, err := s.find(root.ReviewID)
			if err != nil {
				return out, err
			}
			if r.Status != "draft" {
				return out, precondition("review already submitted")
			}
		}
		s.state.Comments = append(s.state.Comments, Comment{ID: id(), ThreadID: root.ID, Path: root.Path, Line: root.Line, Side: root.Side, Body: q.Body, Actor: q.Actor, ReviewID: root.ReviewID})
	case "promote":
		r, err := s.find(q.ReviewID)
		if err != nil {
			return out, err
		}
		if r.Status != "draft" {
			return out, precondition("review already submitted")
		}
		found := false
		for i := range s.state.Comments {
			c := &s.state.Comments[i]
			if c.ID == q.ID {
				if c.ReviewID != "" {
					return out, invalid("only standalone comments can be promoted")
				}
				if c.ThreadID != "" {
					return out, invalid("promote the root thread")
				}
				c.ReviewID = r.ID
				for j := range s.state.Comments {
					if s.state.Comments[j].ThreadID == c.ID {
						s.state.Comments[j].ReviewID = r.ID
					}
				}
				found = true
				break
			}
		}
		if !found {
			return out, missing("comment not found")
		}
	case "edit":
		r, err := s.find(q.ID)
		if err != nil {
			return out, err
		}
		if r.Status != "draft" {
			return out, precondition("review already submitted")
		}
		r.Body = q.Body
	case "submit":
		r, err := s.find(q.ID)
		if err != nil {
			return out, err
		}
		if r.Status != "draft" {
			return out, precondition("review already submitted")
		}
		if q.Event != "COMMENT" && q.Event != "APPROVE" && q.Event != "REQUEST_CHANGES" {
			return out, invalid("invalid review event")
		}
		if q.Event == "REQUEST_CHANGES" && strings.TrimSpace(r.Body) == "" {
			return out, precondition("request changes requires a summary")
		}
		if r.Kind == "github" {
			if err = s.remoteSubmit(ctx, r, q.Event); err != nil {
				return out, err
			}
		}
		r.Status = q.Event
	default:
		return out, invalid("unknown operation %q", q.Op)
	}
	return out, nil
}
