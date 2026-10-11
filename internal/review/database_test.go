package review

import (
	"context"
	"github.com/difflab-io/lgtm/internal/storage"
	"os"
	"path/filepath"
	"testing"
)

func TestPostgresReviewWorkflowAcrossServiceInstances(t *testing.T) {
	dsn := os.Getenv("LGTM_TEST_POSTGRES_URL")
	if dsn == "" {
		t.Skip("run mise run test:postgres")
	}
	root := t.TempDir()
	os.WriteFile(filepath.Join(root, "hello.go"), []byte("package hello\n"), 0600)
	workspace := "review-workflow-" + id()
	newService := func() *Service {
		store, err := storage.Open(context.Background(), storage.Config{Driver: "postgres", DSN: dsn})
		if err != nil {
			t.Fatal(err)
		}
		s, err := NewWithStore(root, workspace, store, &fakeGitHub{})
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { s.Close() })
		return s
	}
	a, b := newService(), newService()
	r := call(t, a, Request{Op: "create", Kind: "local", Actor: human}).State.Reviews[0]
	note := call(t, b, Request{Op: "comment", Path: "hello.go", Line: 1, Body: "Agent note", Actor: agent}).State.Comments[0]
	call(t, a, Request{Op: "reply", ID: note.ID, Body: "Human reply", Actor: human})
	call(t, b, Request{Op: "promote", ID: note.ID, ReviewID: r.ID})
	call(t, a, Request{Op: "resolve", ID: note.ID})
	call(t, b, Request{Op: "edit", ID: r.ID, Body: "Please fix the issue"})
	call(t, a, Request{Op: "submit", ID: r.ID, Event: "REQUEST_CHANGES"})
	state := call(t, b, Request{Op: "state"}).State
	if len(state.Comments) != 2 || state.Comments[1].ReviewID != r.ID || state.Comments[0].Actor != agent || !state.Comments[0].Resolved || state.Reviews[0].Status != "REQUEST_CHANGES" {
		t.Fatal(state)
	}
}
