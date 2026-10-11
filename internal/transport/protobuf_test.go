package transport

import (
	"context"
	pb "github.com/difflab-io/lgtm/gen/go/lgtm/v1"
	"github.com/difflab-io/lgtm/internal/review"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
	"google.golang.org/grpc/test/bufconn"
	"google.golang.org/protobuf/proto"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestGeneratedProtobufClientCompleteLocalWorkflow(t *testing.T) {
	root := t.TempDir()
	os.WriteFile(filepath.Join(root, "hello.go"), []byte("package hello\n"), 0600)
	service, err := review.New(root, filepath.Join(root, "lgtm.db"), review.GH{})
	if err != nil {
		t.Fatal(err)
	}
	defer service.Close()
	listener := bufconn.Listen(1 << 20)
	server := grpc.NewServer(grpc.UnaryInterceptor(Interceptor))
	Register(server, service)
	go server.Serve(listener)
	defer server.Stop()
	conn, err := grpc.NewClient("passthrough:///protobuf", grpc.WithContextDialer(func(context.Context, string) (net.Conn, error) { return listener.Dial() }), grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	client := pb.NewReviewServiceClient(conn)
	ctx := context.Background()
	actor := &pb.Actor{Name: "Codex", Kind: pb.ActorKind_ACTOR_KIND_AGENT}
	created, err := client.CreateReview(ctx, &pb.CreateReviewRequest{Kind: pb.ReviewKind_REVIEW_KIND_LOCAL, Actor: actor})
	if err != nil {
		t.Fatal(err)
	}
	r := created.Workspace.State.Reviews[0]
	added, err := client.AddComment(ctx, &pb.AddCommentRequest{Path: "hello.go", Line: 1, Side: pb.DiffSide_DIFF_SIDE_LEFT, Body: "Review this", Actor: actor})
	if err != nil {
		t.Fatal(err)
	}
	c := added.Workspace.State.Comments[0]
	reply, err := client.AddReply(ctx, &pb.AddReplyRequest{ThreadId: c.Id, Body: "Human reply", Actor: &pb.Actor{Name: "sam", Kind: pb.ActorKind_ACTOR_KIND_HUMAN}})
	if err != nil || reply.Workspace.State.Comments[1].ThreadId != c.Id {
		t.Fatal(reply, err)
	}
	if _, err = client.PromoteThread(ctx, &pb.PromoteThreadRequest{ThreadId: c.Id, ReviewId: r.Id}); err != nil {
		t.Fatal(err)
	}
	if _, err = client.SetThreadResolved(ctx, &pb.SetThreadResolvedRequest{ThreadId: c.Id, Resolved: true}); err != nil {
		t.Fatal(err)
	}
	if _, err = client.UpdateReview(ctx, &pb.UpdateReviewRequest{ReviewId: r.Id, Body: "Please fix it"}); err != nil {
		t.Fatal(err)
	}
	if _, err = client.SubmitReview(ctx, &pb.SubmitReviewRequest{ReviewId: r.Id, Event: pb.ReviewEvent_REVIEW_EVENT_REQUEST_CHANGES}); err != nil {
		t.Fatal(err)
	}
	snapshot, err := client.GetWorkspace(ctx, &pb.GetWorkspaceRequest{})
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Workspace.State.Reviews[0].Status != pb.ReviewStatus_REVIEW_STATUS_REQUEST_CHANGES || !snapshot.Workspace.State.Comments[0].Resolved || snapshot.Workspace.State.Comments[1].ReviewId != r.Id {
		t.Fatal(snapshot)
	}
	file, err := client.GetFile(ctx, &pb.GetFileRequest{Path: "hello.go"})
	if err != nil || file.Text != "package hello\n" {
		t.Fatal(file, err)
	}
	if _, err = client.SubmitReview(ctx, &pb.SubmitReviewRequest{ReviewId: r.Id, Event: pb.ReviewEvent_REVIEW_EVENT_APPROVE}); status.Code(err) != codes.FailedPrecondition {
		t.Fatal(err)
	}
	if _, err = client.UpdateReview(ctx, &pb.UpdateReviewRequest{ReviewId: "missing"}); status.Code(err) != codes.NotFound {
		t.Fatal(err)
	}
	if _, err = client.AddComment(ctx, &pb.AddCommentRequest{Path: "hello.go", Line: 1, Side: pb.DiffSide(99), Body: "Invalid side", Actor: actor}); status.Code(err) != codes.InvalidArgument {
		t.Fatal(err)
	}
	canceled, cancel := context.WithCancel(ctx)
	cancel()
	if _, err = client.GetWorkspace(canceled, &pb.GetWorkspaceRequest{}); status.Code(err) != codes.Canceled {
		t.Fatal(err)
	}
	bytes, err := proto.Marshal(snapshot)
	if err != nil {
		t.Fatal(err)
	}
	decoded := &pb.GetWorkspaceResponse{}
	if err = proto.Unmarshal(bytes, decoded); err != nil || !proto.Equal(snapshot, decoded) {
		t.Fatal("protobuf roundtrip", err)
	}
	for name, service := range server.GetServiceInfo() {
		if name != "lgtm.v1.ReviewService" {
			t.Fatal(name)
		}
		if len(service.Methods) != 13 {
			t.Fatal(service.Methods)
		}
		for _, method := range service.Methods {
			if strings.EqualFold(method.Name, "Execute") {
				t.Fatal("legacy Execute RPC remains")
			}
		}
	}
}
