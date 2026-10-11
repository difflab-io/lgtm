package review

import (
	"bytes"
	"context"
	"encoding/json"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

type GitHub interface {
	Call(context.Context, string, string, any, any) error
}
type GH struct{}

func (GH) Call(ctx context.Context, method, path string, input, output any) error {
	args := []string{"api", "--method", method, path}
	cmd := exec.CommandContext(ctx, "gh", args...)
	if input != nil {
		data, err := json.Marshal(input)
		if err != nil {
			return err
		}
		cmd.Args = append(cmd.Args, "--input", "-")
		cmd.Stdin = bytes.NewReader(data)
	}
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	data, err := cmd.Output()
	if err != nil {
		var detail struct {
			Message string            `json:"message"`
			Errors  []json.RawMessage `json:"errors"`
		}
		if json.Unmarshal(data, &detail) == nil && detail.Message != "" {
			messages := []string{detail.Message}
			for _, raw := range detail.Errors {
				var message string
				if json.Unmarshal(raw, &message) == nil {
					messages = append(messages, message)
					continue
				}
				var item struct {
					Message string `json:"message"`
					Field   string `json:"field"`
					Code    string `json:"code"`
				}
				if json.Unmarshal(raw, &item) == nil {
					if item.Message != "" {
						messages = append(messages, item.Message)
					} else {
						messages = append(messages, item.Field+": "+item.Code)
					}
				}
			}
			return upstream("GitHub: %s", strings.Join(messages, "; "))
		}
		return upstream("GitHub: %s: %w", strings.TrimSpace(stderr.String()), err)
	}
	if output != nil {
		return json.Unmarshal(data, output)
	}
	return nil
}

var repoPattern = regexp.MustCompile(`^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$`)

func prPath(r Review) string { return "repos/" + r.Repo + "/pulls/" + strconv.Itoa(r.PR) }
func (s *Service) remoteSubmit(ctx context.Context, r *Review, event string) error {
	var current struct {
		State string `json:"state"`
		User  struct {
			Login string `json:"login"`
		} `json:"user"`
		Head struct {
			SHA string `json:"sha"`
		} `json:"head"`
	}
	if err := s.github.Call(ctx, "GET", prPath(*r), nil, &current); err != nil {
		return err
	}
	if current.Head.SHA != r.Head {
		return precondition("PR head changed; create a new review against the current head")
	}
	if current.State != "" && current.State != "open" {
		return precondition("this pull request is closed; your draft remains saved locally")
	}
	if event == "APPROVE" || event == "REQUEST_CHANGES" {
		var user struct {
			Login string `json:"login"`
		}
		if err := s.github.Call(ctx, "GET", "user", nil, &user); err != nil {
			return err
		}
		if user.Login != "" && user.Login == current.User.Login {
			return precondition("GitHub does not allow approving or requesting changes on your own PR; choose Submit review comments instead")
		}
	}
	files, err := githubList[githubFile](ctx, s.github, prPath(*r)+"/files")
	if err != nil {
		return err
	}
	payload := submissionPayload(*r, s.state.Comments, files, event)
	if event == "COMMENT" && strings.TrimSpace(payload["body"].(string)) == "" {
		if len(payload["comments"].([]map[string]any)) == 0 {
			return precondition("add a summary or comment before submitting a GitHub comment review")
		}
		payload["body"] = "Review comments."
	}
	var result struct {
		ID int64 `json:"id"`
	}
	if err := s.github.Call(ctx, "POST", prPath(*r)+"/reviews", payload, &result); err != nil {
		return err
	}
	r.RemoteID = result.ID
	return nil
}
