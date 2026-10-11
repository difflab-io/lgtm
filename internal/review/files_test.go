package review

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestChangedFilesPerFileDiffAndDeletedLineComment(t *testing.T) {
	s, _ := fixture(t, &fakeGitHub{})
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", s.root}, args...)...)
		if data, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %s %v", args, data, err)
		}
	}
	git("init", "-q")
	_ = os.WriteFile(filepath.Join(s.root, ".gitignore"), []byte(".lgtm/\n"), 0600)
	_ = os.WriteFile(filepath.Join(s.root, "old.go"), []byte("package old\n"), 0600)
	git("add", ".")
	git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "Fixture baseline")
	_ = os.WriteFile(filepath.Join(s.root, "hello.go"), []byte("package changed\n"), 0600)
	git("add", "hello.go")
	_ = os.WriteFile(filepath.Join(s.root, "note.go"), []byte("package note\n"), 0600)
	_ = os.Remove(filepath.Join(s.root, "old.go"))
	out := call(t, s, Request{Op: "files"})
	if len(out.Files) != 3 {
		t.Fatal(out.Files)
	}
	out = call(t, s, Request{Op: "diff", Path: "hello.go"})
	if !strings.Contains(out.Text, "+package changed") || strings.Contains(out.Text, "old.go") {
		t.Fatal(out.Text)
	}
	out = call(t, s, Request{Op: "diff", Path: "note.go"})
	if !strings.Contains(out.Text, "+package note") {
		t.Fatal(out.Text)
	}
	call(t, s, Request{Op: "comment", Path: "old.go", Line: 1, Side: "LEFT", Body: "Deleted file", Actor: human})
	out = call(t, s, Request{Op: "files", Kind: "all"})
	if len(out.Files) != 4 {
		t.Fatal(out.Files)
	}
	if _, err := s.Execute(context.Background(), Request{Op: "diff", Path: "../outside"}); err == nil {
		t.Fatal("outside path accepted")
	}
}
