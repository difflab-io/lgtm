package review

import (
	"context"
	"fmt"
	"github.com/sourcegraph/go-diff/diff"
	"strings"
)

type githubFile struct {
	Filename         string `json:"filename"`
	PreviousFilename string `json:"previous_filename"`
	Patch            string `json:"patch"`
}

func githubList[T any](ctx context.Context, g GitHub, path string) ([]T, error) {
	result := []T{}
	separator := "?"
	if strings.Contains(path, "?") {
		separator = "&"
	}
	for page := 1; ; page++ {
		var batch []T
		if err := g.Call(ctx, "GET", fmt.Sprintf("%s%sper_page=100&page=%d", path, separator, page), nil, &batch); err != nil {
			return nil, err
		}
		result = append(result, batch...)
		if len(batch) < 100 {
			return result, nil
		}
	}
}

// GitHub accepts anchors in a diff hunk, including its context lines. Notes
// outside these ranges become summary notes instead of invalid inline comments.
func inlinePath(c Comment, files []githubFile) string {
	for _, file := range files {
		if c.Path != file.Filename && !(c.Side == "LEFT" && c.Path == file.PreviousFilename) {
			continue
		}
		hunks, err := diff.ParseHunks([]byte(file.Patch))
		if err != nil {
			continue
		}
		for _, h := range hunks {
			start, count := int(h.NewStartLine), int(h.NewLines)
			if c.Side == "LEFT" {
				start, count = int(h.OrigStartLine), int(h.OrigLines)
			}
			if c.Line >= start && c.Line < start+count {
				return file.Filename
			}
		}
	}
	return ""
}
func submissionPayload(r Review, comments []Comment, files []githubFile, event string) map[string]any {
	body := attributed(r.Body, r.Actor)
	inline := []map[string]any{}
	for _, c := range comments {
		if c.ReviewID != r.ID || c.ThreadID != "" {
			continue
		}
		text := attributed(c.Body, c.Actor)
		for _, reply := range comments {
			if reply.ThreadID == c.ID {
				text += fmt.Sprintf("\n\n**%s [%s]**\n\n%s", reply.Actor.Name, reply.Actor.Kind, attributed(reply.Body, reply.Actor))
			}
		}
		if path := inlinePath(c, files); path != "" {
			inline = append(inline, map[string]any{"path": path, "line": c.Line, "side": c.Side, "body": text})
		} else {
			body += fmt.Sprintf("\n\n### Checkout note: %s:%d (%s)\n%s [%s]\n\n%s", c.Path, c.Line, c.Side, c.Actor.Name, c.Actor.Kind, text)
		}
	}
	return map[string]any{"body": body, "event": event, "commit_id": r.Head, "comments": inline}
}
