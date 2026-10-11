package hosting

import (
	"bytes"
	"connectrpc.com/connect"
	"context"
	"encoding/json"
	pb "github.com/difflab-io/lgtm/gen/go/lgtm/v1"
	"github.com/difflab-io/lgtm/gen/go/lgtm/v1/lgtmv1connect"
	"github.com/difflab-io/lgtm/internal/review"
	"github.com/difflab-io/lgtm/internal/storage"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	healthpb "google.golang.org/grpc/health/grpc_health_v1"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"testing"
)

func TestHTTPGatewayAndStaticUI(t *testing.T) {
	root := t.TempDir()
	_ = os.WriteFile(filepath.Join(root, "index.html"), []byte("<title>LGTM</title>"), 0600)
	running, err := Start(root, filepath.Join(root, "lgtm.db"), root, "127.0.0.1:0", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer running.Close()
	endpoint := "http://" + running.HTTPAddr
	request := func(origin string, q review.Request) (int, review.Response) {
		data, _ := json.Marshal(q)
		req, _ := http.NewRequest("POST", endpoint+"/api", bytes.NewReader(data))
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Origin", origin)
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer res.Body.Close()
		var out review.Response
		if err = json.NewDecoder(res.Body).Decode(&out); err != nil {
			t.Fatal(err)
		}
		return res.StatusCode, out
	}
	code, out := request(endpoint, review.Request{Op: "create", Kind: "local", Actor: review.Actor{Name: "me", Kind: "human"}})
	if code != 200 || len(out.State.Reviews) != 1 {
		t.Fatalf("%d %+v", code, out)
	}
	code, _ = request("https://outside.example", review.Request{Op: "state"})
	if code != 403 {
		t.Fatal("cross-origin request accepted")
	}
	res, err := http.Get(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	data, _ := io.ReadAll(res.Body)
	if !bytes.Contains(data, []byte("LGTM")) {
		t.Fatal("static UI not served")
	}
}

func TestPostgresHostedGeneratedClientAndHealth(t *testing.T) {
	dsn := os.Getenv("LGTM_TEST_POSTGRES_URL")
	if dsn == "" {
		t.Skip("run mise run test:postgres")
	}
	root := t.TempDir()
	running, err := StartConfigured(Config{Root: root, Workspace: "host-" + filepath.Base(root), UI: root, GRPCAddr: "127.0.0.1:0", HTTPAddr: "127.0.0.1:0", Database: storage.Config{Driver: "postgres", DSN: dsn}})
	if err != nil {
		t.Fatal(err)
	}
	defer running.Close()
	conn, err := grpc.NewClient(running.GRPCAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	health, err := healthpb.NewHealthClient(conn).Check(context.Background(), &healthpb.HealthCheckRequest{Service: "lgtm.v1.ReviewService"})
	if err != nil || health.Status != healthpb.HealthCheckResponse_SERVING {
		t.Fatal(health, err)
	}
	client := pb.NewReviewServiceClient(conn)
	created, err := client.CreateReview(context.Background(), &pb.CreateReviewRequest{Kind: pb.ReviewKind_REVIEW_KIND_LOCAL, Actor: &pb.Actor{Name: "host", Kind: pb.ActorKind_ACTOR_KIND_HUMAN}})
	if err != nil || len(created.Workspace.State.Reviews) != 1 {
		t.Fatal(created, err)
	}
	res, err := http.Get("http://" + running.HTTPAddr + "/healthz")
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatal(res.Status)
	}
	browser := lgtmv1connect.NewReviewServiceClient(http.DefaultClient, "http://"+running.HTTPAddr)
	workspace, err := browser.GetWorkspace(context.Background(), connect.NewRequest(&pb.GetWorkspaceRequest{}))
	if err != nil || len(workspace.Msg.Workspace.State.Reviews) != 1 {
		t.Fatal(workspace, err)
	}
}
