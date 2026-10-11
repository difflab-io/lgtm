package review

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
	"testing"
)

type pullGitHub struct{ calls []string }

func (g *pullGitHub) Call(_ context.Context, method, path string, input, output any) error {
	g.calls = append(g.calls, path)
	if strings.Contains(path, "/pulls?") {
		if !strings.Contains(path, "?state=open&per_page=100&page=") {
			return fmt.Errorf("invalid query: %s", path)
		}
		pulls := []PullRequest{}
		if strings.HasSuffix(path, "page=1") {
			for i := 1; i <= 100; i++ {
				pulls = append(pulls, PullRequest{Number: i, Title: "Review changes"})
			}
		}
		data, _ := json.Marshal(pulls)
		return json.Unmarshal(data, output)
	}
	return json.Unmarshal([]byte(`{"head":{"sha":"head"}}`), output)
}
func TestCheckoutOpenPullsAndInferredReviewRepository(t *testing.T) {
	g := &pullGitHub{}
	s, _ := fixture(t, g)
	for _, remote := range []string{"https://github.com/example/project.git", "git@github.com:example/project.git", "ssh://git@github.com/example/project.git"} {
		cmd := exec.Command("git", "-C", s.root, "init", "-q")
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
		_ = exec.Command("git", "-C", s.root, "remote", "remove", "origin").Run()
		if err := exec.Command("git", "-C", s.root, "remote", "add", "origin", remote).Run(); err != nil {
			t.Fatal(err)
		}
		out := call(t, s, Request{Op: "pulls"})
		if out.Repository != "example/project" || len(out.Pulls) != 100 {
			t.Fatal(out)
		}
	}
	out := call(t, s, Request{Op: "create", Kind: "github", PR: 1, Actor: Actor{Name: "me", Kind: "human"}})
	if out.State.Reviews[0].Repo != "example/project" {
		t.Fatal(out.State)
	}
	_ = exec.Command("git", "-C", s.root, "remote", "set-url", "origin", "https://gitlab.com/example/project.git").Run()
	if _, err := s.Execute(context.Background(), Request{Op: "pulls"}); err == nil {
		t.Fatal("non-GitHub origin accepted")
	}
}
