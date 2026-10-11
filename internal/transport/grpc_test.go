package transport

import (
	"context"
	"github.com/difflab-io/lgtm/internal/review"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/test/bufconn"
	"net"
	"path/filepath"
	"testing"
)

func TestRoundTrip(t *testing.T) {
	root := t.TempDir()
	s, err := review.New(root, filepath.Join(root, "lgtm.db"), review.GH{})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	listener := bufconn.Listen(1 << 20)
	server := grpc.NewServer()
	Register(server, s)
	go server.Serve(listener)
	defer server.Stop()
	conn, err := grpc.NewClient("passthrough:///test", grpc.WithContextDialer(func(context.Context, string) (net.Conn, error) { return listener.Dial() }), grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	out, err := Call(context.Background(), conn, review.Request{Op: "create", Kind: "local", Actor: review.Actor{Name: "Claude", Kind: "agent"}})
	if err != nil {
		t.Fatal(err)
	}
	if len(out.State.Reviews) != 1 || out.State.Reviews[0].Actor.Name != "Claude" {
		t.Fatal(out)
	}
	if _, err = Call(context.Background(), conn, review.Request{Op: "invalid"}); err == nil {
		t.Fatal("RPC lost service error")
	}
}
