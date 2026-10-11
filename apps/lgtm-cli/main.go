package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"github.com/difflab-io/lgtm/internal/hosting"
	"github.com/difflab-io/lgtm/internal/review"
	"github.com/difflab-io/lgtm/internal/storage"
	"github.com/difflab-io/lgtm/internal/transport"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"log"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"
)

func main() {
	if len(os.Args) < 2 {
		log.Fatal("usage: lgtm serve | rpc < request.json | migrate --from state.json | smoke")
	}
	switch os.Args[1] {
	case "serve":
		flags := flag.NewFlagSet("serve", flag.ExitOnError)
		options := hosting.BindFlags(flags)
		_ = flags.Parse(os.Args[2:])
		stop, err := hosting.StartConfigured(options.Config())
		if err != nil {
			log.Fatal(err)
		}
		defer stop.Close()
		signals := make(chan os.Signal, 1)
		signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
		<-signals
	case "migrate":
		flags := flag.NewFlagSet("migrate", flag.ExitOnError)
		options := hosting.BindFlags(flags)
		from := flags.String("from", "", "legacy JSON source")
		_ = flags.Parse(os.Args[2:])
		if *from == "" {
			log.Fatal("--from requires the legacy JSON state file")
		}
		config := options.Config()
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		store, err := storage.Open(ctx, config.Database)
		if err != nil {
			log.Fatal(err)
		}
		defer store.Close()
		root, err := filepath.Abs(config.Root)
		if err != nil {
			log.Fatal(err)
		}
		root, err = filepath.EvalSymlinks(root)
		if err != nil {
			log.Fatal(err)
		}
		workspace := config.Workspace
		if workspace == "" {
			workspace = root
		}
		source, err := filepath.Abs(*from)
		if err != nil {
			log.Fatal(err)
		}
		if err = store.ImportJSON(ctx, workspace, source); err != nil {
			log.Fatal(err)
		}
		fmt.Println("Legacy state imported; source JSON preserved")
	case "rpc":
		conn, err := grpc.NewClient("127.0.0.1:9911", grpc.WithTransportCredentials(insecure.NewCredentials()), grpc.WithDefaultCallOptions(grpc.MaxCallRecvMsgSize(32<<20), grpc.MaxCallSendMsgSize(32<<20)))
		if err != nil {
			log.Fatal(err)
		}
		defer conn.Close()
		var q review.Request
		if err = json.NewDecoder(os.Stdin).Decode(&q); err != nil {
			log.Fatal(err)
		}
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		out, err := transport.Call(ctx, conn, q)
		if err != nil {
			log.Fatal(err)
		}
		_ = json.NewEncoder(os.Stdout).Encode(out)
	case "smoke":
		dir, err := os.MkdirTemp("", "lgtm-smoke-")
		if err != nil {
			log.Fatal(err)
		}
		defer os.RemoveAll(dir)
		stop, err := hosting.Start(".", filepath.Join(dir, "lgtm.db"), "apps/lgtm-web-app/build", "127.0.0.1:0", "127.0.0.1:0")
		if err != nil {
			log.Fatal(err)
		}
		conn, err := grpc.NewClient(stop.GRPCAddr, grpc.WithTransportCredentials(insecure.NewCredentials()), grpc.WithDefaultCallOptions(grpc.MaxCallRecvMsgSize(32<<20), grpc.MaxCallSendMsgSize(32<<20)))
		if err != nil {
			log.Fatal(err)
		}
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		out, err := transport.Call(ctx, conn, review.Request{Op: "create", Kind: "local", Actor: review.Actor{Name: "smoke", Kind: "human"}})
		if err != nil || len(out.State.Reviews) != 1 {
			log.Fatalf("roundtrip failed: %v", err)
		}
		cancel()
		conn.Close()
		stop.Close()
		fmt.Println("service, gateway, and gRPC review creation passed")
	default:
		log.Fatal("unknown command")
	}
}
