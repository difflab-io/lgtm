package review

import (
	"context"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

func (s *Service) files(ctx context.Context, q Request) ([]ChangedFile, error) {
	if q.ID != "" {
		r, err := s.find(q.ID)
		if err != nil {
			return nil, err
		}
		if r.Kind == "github" {
			files, err := githubList[struct {
				Filename string `json:"filename"`
				Status   string `json:"status"`
			}](ctx, s.github, prPath(*r)+"/files")
			if err != nil {
				return nil, err
			}
			out := []ChangedFile{}
			for _, f := range files {
				out = append(out, ChangedFile{f.Filename, f.Status})
			}
			return out, nil
		}
	}
	changed, err := s.git(ctx, "diff", "--no-renames", "--name-status", "-z", "HEAD", "--")
	if err != nil {
		return nil, err
	}
	byPath := map[string]string{}
	parts := strings.Split(strings.TrimSuffix(changed, "\x00"), "\x00")
	for i := 0; i+1 < len(parts); i += 2 {
		status := map[string]string{"M": "modified", "A": "added", "D": "deleted", "T": "modified"}[parts[i]]
		if status == "" {
			status = "modified"
		}
		byPath[parts[i+1]] = status
	}
	untracked, err := s.git(ctx, "ls-files", "--others", "--exclude-standard", "-z")
	if err != nil {
		return nil, err
	}
	for _, p := range strings.Split(untracked, "\x00") {
		if p != "" {
			byPath[p] = "untracked"
		}
	}
	if q.Kind == "all" {
		tracked, err := s.git(ctx, "ls-files", "--cached", "-z")
		if err != nil {
			return nil, err
		}
		for _, p := range strings.Split(tracked, "\x00") {
			if p != "" && byPath[p] == "" {
				byPath[p] = "unchanged"
			}
		}
	}
	out := []ChangedFile{}
	for path, status := range byPath {
		out = append(out, ChangedFile{path, status})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Path < out[j].Path })
	return out, nil
}
func (s *Service) localDiff(ctx context.Context, path string) (string, error) {
	args := []string{"diff", "HEAD", "--"}
	if path != "" {
		args = append(args, path)
	}
	text, err := s.git(ctx, args...)
	if err != nil || text != "" || path == "" {
		return text, err
	}
	tracked, err := s.git(ctx, "ls-files", "--error-unmatch", "--", path)
	if err == nil && tracked != "" {
		return "", nil
	}
	full, err := s.filePath(path)
	if err != nil {
		return "", err
	}
	// Let Git produce a real patch for untracked files, including escaping paths.
	cmd := exec.CommandContext(ctx, "git", "diff", "--no-index", "--", "/dev/null", full)
	data, err := cmd.CombinedOutput()
	if exit, ok := err.(*exec.ExitError); ok && exit.ExitCode() == 1 {
		err = nil
	}
	if err != nil {
		return "", invalid("git diff: %s: %w", data, err)
	}
	return strings.ReplaceAll(string(data), filepath.ToSlash(full), "/"+filepath.ToSlash(path)), nil
}
