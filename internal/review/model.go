package review

import (
	"crypto/rand"
	"encoding/hex"
	"github.com/difflab-io/lgtm/internal/domain"
)

type Actor = domain.Actor
type Comment = domain.Comment
type Review = domain.Review
type State = domain.State
type Request = domain.Request
type Response = domain.Response
type ChangedFile = domain.ChangedFile
type PullRequest = domain.PullRequest

func id() string {
	var b [12]byte
	if _, err := rand.Read(b[:]); err != nil {
		panic(err)
	}
	return hex.EncodeToString(b[:])
}
func attributed(body string, actor Actor) string {
	if actor.Kind == "agent" {
		return body + "\n\n> Generated with " + actor.Name + "."
	}
	return body
}
func validateActor(a Actor) error {
	if a.Name == "" || (a.Kind != "human" && a.Kind != "agent") {
		return invalid("actor requires name and kind human or agent")
	}
	return nil
}
