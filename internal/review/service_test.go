package review

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type fakeGitHub struct {
	head    string
	payload map[string]any
	fail    bool
	posts   int
}

func (g *fakeGitHub) Call(_ context.Context, method, path string, input, output any) error {
	if method == "POST" {
		g.posts++
		if g.fail {
			return errors.New("remote failure")
		}
		g.payload = input.(map[string]any)
		data := []byte(`{"id":42}`)
		return json.Unmarshal(data, output)
	}
	if strings.Contains(path, "/files?") {
		return json.Unmarshal([]byte(`[{"filename":"hello.go","patch":"@@ -1 +1 @@\n-package old\n+package hello\n"}]`), output)
	}
	return json.Unmarshal([]byte(`{"head":{"sha":"`+g.head+`"}}`), output)
}
func fixture(t *testing.T, g GitHub) (*Service, string) {
	t.Helper()
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "hello.go"), []byte("package hello\n"), 0600); err != nil {
		t.Fatal(err)
	}
	file := filepath.Join(root, ".lgtm/lgtm.db")
	s, err := New(root, file, g)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	return s, file
}
func call(t *testing.T, s *Service, q Request) Response {
	t.Helper()
	out, err := s.Execute(context.Background(), q)
	if err != nil {
		t.Fatal(err)
	}
	return out
}

var human = Actor{Name: "sam", Kind: "human"}
var agent = Actor{Name: "Codex", Kind: "agent"}

func TestLocalNotesPromotionPersistence(t *testing.T) {
	s, file := fixture(t, &fakeGitHub{})
	out := call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "Check naming", Actor: agent})
	c := out.State.Comments[0]
	out = call(t, s, Request{Op: "create", Kind: "local", Actor: human})
	r := out.State.Reviews[0]
	call(t, s, Request{Op: "promote", ID: c.ID, ReviewID: r.ID})
	call(t, s, Request{Op: "submit", ID: r.ID, Event: "APPROVE"})
	if _, err := s.Execute(context.Background(), Request{Op: "comment", Path: "hello.go", Line: 1, Body: "late", Actor: human, ReviewID: r.ID}); err == nil {
		t.Fatal("submitted review accepted edit")
	}
	loaded, err := New(s.root, file, &fakeGitHub{})
	if err != nil {
		t.Fatal(err)
	}
	defer loaded.Close()
	out = call(t, loaded, Request{Op: "state"})
	if out.State.Reviews[0].Status != "APPROVE" || out.State.Comments[0].ReviewID != r.ID || out.State.Comments[0].Actor != agent {
		t.Fatalf("bad persisted state: %+v", out)
	}
}
func TestGitHubSubmissionAttributionAndDuplicateGuard(t *testing.T) {
	g := &fakeGitHub{head: "abc"}
	s, _ := fixture(t, g)
	out := call(t, s, Request{Op: "create", Kind: "github", Repo: "owner/repo", PR: 7, Actor: agent, Body: "Looks good"})
	r := out.State.Reviews[0]
	call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "Human note", Actor: human, ReviewID: r.ID})
	call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "Agent note", Actor: agent, ReviewID: r.ID})
	out = call(t, s, Request{Op: "submit", ID: r.ID, Event: "APPROVE"})
	if out.State.Reviews[0].RemoteID != 42 {
		t.Fatal("remote id lost")
	}
	if g.payload["body"] != "Looks good\n\n> Generated with Codex." {
		t.Fatal(g.payload)
	}
	comments := g.payload["comments"].([]map[string]any)
	if comments[0]["body"] != "Human note" || comments[1]["body"] != "Agent note\n\n> Generated with Codex." {
		t.Fatal(comments)
	}
	if _, err := s.Execute(context.Background(), Request{Op: "submit", ID: r.ID, Event: "APPROVE"}); err == nil {
		t.Fatal("duplicate submission accepted")
	}
	if g.posts != 1 {
		t.Fatal("duplicate POST")
	}
}
func TestStaleHeadAndRemoteFailureKeepDraft(t *testing.T) {
	g := &fakeGitHub{head: "abc"}
	s, _ := fixture(t, g)
	out := call(t, s, Request{Op: "create", Kind: "github", Repo: "o/r", PR: 1, Actor: human})
	id := out.State.Reviews[0].ID
	g.head = "changed"
	if _, err := s.Execute(context.Background(), Request{Op: "submit", ID: id, Event: "COMMENT"}); err == nil || !strings.Contains(err.Error(), "head changed") {
		t.Fatal(err)
	}
	if g.posts != 0 {
		t.Fatal("submitted stale head")
	}
	g.head = "abc"
	g.fail = true
	if _, err := s.Execute(context.Background(), Request{Op: "submit", ID: id, Event: "COMMENT"}); err == nil {
		t.Fatal("failure ignored")
	}
	if call(t, s, Request{Op: "state"}).State.Reviews[0].Status != "draft" {
		t.Fatal("failure changed draft")
	}
}
func TestValidationAndPathEscape(t *testing.T) {
	s, _ := fixture(t, &fakeGitHub{})
	for _, q := range []Request{{Op: "create", Kind: "github", Repo: "../bad", PR: 1, Actor: human}, {Op: "create", Kind: "local", Actor: Actor{Name: "bad", Kind: "unknown"}}, {Op: "comment", Path: "hello.go", Line: 0, Body: "x", Actor: human}, {Op: "file", Path: "../outside"}} {
		if _, err := s.Execute(context.Background(), q); err == nil {
			t.Fatalf("accepted invalid request %+v", q)
		}
	}
	external := filepath.Join(t.TempDir(), "secret")
	_ = os.WriteFile(external, []byte("secret"), 0600)
	if err := os.Symlink(external, filepath.Join(s.root, "escape")); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Execute(context.Background(), Request{Op: "file", Path: "escape"}); err == nil {
		t.Fatal("symlink escape accepted")
	}
}

func TestPromotedNoteOutsideDiffBecomesAttributedSummary(t *testing.T) {
	g := &fakeGitHub{head: "abc"}
	s, _ := fixture(t, g)
	out := call(t, s, Request{Op: "comment", Path: "hello.go", Line: 20, Body: "Outside the diff", Actor: agent})
	c := out.State.Comments[0]
	out = call(t, s, Request{Op: "create", Kind: "github", Repo: "o/r", PR: 1, Actor: human, Body: "Summary"})
	r := out.State.Reviews[0]
	call(t, s, Request{Op: "promote", ID: c.ID, ReviewID: r.ID})
	call(t, s, Request{Op: "submit", ID: r.ID, Event: "COMMENT"})
	if len(g.payload["comments"].([]map[string]any)) != 0 {
		t.Fatal("unanchored note sent inline")
	}
	body := g.payload["body"].(string)
	if !strings.Contains(body, "hello.go:20 (RIGHT)") || !strings.Contains(body, "Codex [agent]") || !strings.Contains(body, "> Generated with Codex.") {
		t.Fatal(body)
	}
}

func TestGitHubReviewActionsAndRequiredSummary(t *testing.T) {
	for _, event := range []string{"COMMENT", "APPROVE", "REQUEST_CHANGES"} {
		t.Run(event, func(t *testing.T) {
			g := &fakeGitHub{head: "abc"}
			s, _ := fixture(t, g)
			out := call(t, s, Request{Op: "create", Kind: "github", Repo: "o/r", PR: 1, Actor: human})
			r := out.State.Reviews[0]
			if event == "REQUEST_CHANGES" {
				if _, err := s.Execute(context.Background(), Request{Op: "submit", ID: r.ID, Event: event}); err == nil {
					t.Fatal("request changes accepted without summary")
				}
				if g.posts != 0 {
					t.Fatal("invalid review reached GitHub")
				}
			}
			call(t, s, Request{Op: "edit", ID: r.ID, Body: "Review summary"})
			call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Side: "LEFT", Body: "Old-side comment", Actor: human, ReviewID: r.ID})
			out = call(t, s, Request{Op: "submit", ID: r.ID, Event: event})
			if out.State.Reviews[0].Status != event || g.payload["event"] != event || g.payload["commit_id"] != "abc" {
				t.Fatal(out, g.payload)
			}
			comments := g.payload["comments"].([]map[string]any)
			if len(comments) != 1 || comments[0]["side"] != "LEFT" {
				t.Fatal(comments)
			}
		})
	}
}

func TestPersistenceFailureRollsBackMutation(t *testing.T) {
	s, _ := fixture(t, &fakeGitHub{})
	s.store.Close()
	if _, err := s.Execute(context.Background(), Request{Op: "create", Kind: "local", Actor: human}); err == nil {
		t.Fatal("persistence failure ignored")
	}
	if len(s.state.Reviews) != 0 {
		t.Fatal("failed mutation remained in memory")
	}
}

func TestThreadRepliesPromoteTogetherAndSyncAsOneThread(t *testing.T) {
	g := &fakeGitHub{head: "abc"}
	s, _ := fixture(t, g)
	out := call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "Human thread", Actor: human})
	root := out.State.Comments[0]
	call(t, s, Request{Op: "reply", ID: root.ID, Body: "Agent reply", Actor: agent})
	out = call(t, s, Request{Op: "create", Kind: "github", Repo: "o/r", PR: 1, Actor: human})
	r := out.State.Reviews[0]
	out = call(t, s, Request{Op: "promote", ID: root.ID, ReviewID: r.ID})
	if out.State.Comments[1].ReviewID != r.ID {
		t.Fatal("reply was not promoted with root")
	}
	call(t, s, Request{Op: "resolve", ID: root.ID})
	call(t, s, Request{Op: "resolve", ID: root.ID, Event: "REOPEN"})
	call(t, s, Request{Op: "submit", ID: r.ID, Event: "COMMENT"})
	comments := g.payload["comments"].([]map[string]any)
	if len(comments) != 1 {
		t.Fatal("reply sent as separate thread", comments)
	}
	body := comments[0]["body"].(string)
	if !strings.Contains(body, "Human thread") || !strings.Contains(body, "Codex [agent]") || !strings.Contains(body, "> Generated with Codex.") {
		t.Fatal(body)
	}
	if _, err := s.Execute(context.Background(), Request{Op: "reply", ID: root.ID, Body: "late", Actor: human}); err == nil {
		t.Fatal("submitted review accepted reply")
	}
}
func TestFailedResolutionRestoresOptionalFields(t *testing.T) {
	s, _ := fixture(t, &fakeGitHub{})
	out := call(t, s, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "note", Actor: human})
	id := out.State.Comments[0].ID
	s.store.Close()
	if _, err := s.Execute(context.Background(), Request{Op: "resolve", ID: id}); err == nil {
		t.Fatal("persistence failure ignored")
	}
	if s.state.Comments[0].Resolved {
		t.Fatal("failed resolution remained in memory")
	}
}
