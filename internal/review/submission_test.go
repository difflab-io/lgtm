package review

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type submissionGitHub struct {
	fakeGitHub
	author, login, state string
}

func (g *submissionGitHub) Call(ctx context.Context, method, path string, input, output any) error {
	if method == "GET" && path == "user" {
		data, _ := json.Marshal(map[string]string{"login": g.login})
		return json.Unmarshal(data, output)
	}
	if method == "GET" && !strings.Contains(path, "?") {
		data, _ := json.Marshal(map[string]any{"head": map[string]string{"sha": g.head}, "user": map[string]string{"login": g.author}, "state": g.state})
		return json.Unmarshal(data, output)
	}
	return g.fakeGitHub.Call(ctx, method, path, input, output)
}
func TestGitHubSubmissionPreconditionsRetainDraft(t *testing.T) {
	g := &submissionGitHub{fakeGitHub: fakeGitHub{head: "head"}, author: "me", login: "me", state: "open"}
	s, _ := fixture(t, g)
	r := call(t, s, Request{Op: "create", Kind: "github", Repo: "owner/repo", PR: 1, Actor: Actor{Name: "me", Kind: "human"}}).State.Reviews[0]
	for _, event := range []string{"APPROVE", "REQUEST_CHANGES", "COMMENT"} {
		if event == "REQUEST_CHANGES" {
			call(t, s, Request{Op: "edit", ID: r.ID, Body: "Please fix this"})
		}
		if event == "COMMENT" {
			call(t, s, Request{Op: "edit", ID: r.ID, Body: ""})
		}
		_, err := s.Execute(context.Background(), Request{Op: "submit", ID: r.ID, Event: event})
		if err == nil || g.posts != 0 || s.state.Reviews[0].Status != "draft" {
			t.Fatalf("%s: %v %+v", event, err, s.state)
		}
	}
	call(t, s, Request{Op: "comment", ReviewID: r.ID, Path: "hello.go", Line: 1, Body: "Check this", Actor: Actor{Name: "me", Kind: "human"}})
	call(t, s, Request{Op: "submit", ID: r.ID, Event: "COMMENT"})
	if g.payload["body"] != "Review comments." || g.posts != 1 {
		t.Fatal(g.payload)
	}
}
func TestGHReportsStructuredValidationErrors(t *testing.T) {
	dir := t.TempDir()
	executable := filepath.Join(dir, "gh")
	script := "#!/bin/sh\nprintf '%s' '{\"message\":\"Validation Failed\",\"errors\":[{\"message\":\"Line is outside the diff\"},\"Review already pending\"]}'\nprintf '%s' 'gh: Validation Failed (HTTP 422)' >&2\nexit 1\n"
	if err := os.WriteFile(executable, []byte(script), 0700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	err := (GH{}).Call(context.Background(), "POST", "repos/owner/repo/pulls/1/reviews", nil, nil)
	if err == nil || !strings.Contains(err.Error(), "Line is outside the diff") || !strings.Contains(err.Error(), "Review already pending") {
		t.Fatal(err)
	}
}
