package review

import (
	"context"
	"net/url"
	"os/exec"
	"strings"
)

func (s *Service) checkoutRepository(ctx context.Context) (string, error) {
	cmd := exec.CommandContext(ctx, "git", "-C", s.root, "remote", "get-url", "origin")
	data, err := cmd.Output()
	if err != nil {
		return "", invalid("this checkout has no origin remote")
	}
	remote := strings.TrimSpace(string(data))
	var path string
	if strings.HasPrefix(remote, "git@github.com:") {
		path = strings.TrimPrefix(remote, "git@github.com:")
	} else {
		parsed, err := url.Parse(remote)
		if err != nil || parsed.Hostname() != "github.com" {
			return "", invalid("origin is not a GitHub repository")
		}
		path = strings.TrimPrefix(parsed.Path, "/")
	}
	path = strings.TrimSuffix(path, ".git")
	if !repoPattern.MatchString(path) || strings.Contains(path, "..") {
		return "", invalid("invalid GitHub origin")
	}
	return path, nil
}

func (s *Service) openPulls(ctx context.Context) (Response, error) {
	repo, err := s.checkoutRepository(ctx)
	if err != nil {
		return Response{}, err
	}
	pulls, err := githubList[PullRequest](ctx, s.github, "repos/"+repo+"/pulls?state=open")
	return Response{Repository: repo, Pulls: pulls}, err
}
