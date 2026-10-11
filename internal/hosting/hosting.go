package hosting

import (
	"context"
	"encoding/json"
	"fmt"
	"github.com/difflab-io/lgtm/gen/go/lgtm/v1/lgtmv1connect"
	"github.com/difflab-io/lgtm/internal/review"
	"github.com/difflab-io/lgtm/internal/storage"
	"github.com/difflab-io/lgtm/internal/transport"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/health"
	healthpb "google.golang.org/grpc/health/grpc_health_v1"
	"google.golang.org/grpc/reflection"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sync"
	"time"
)

type Running struct {
	Close    func()
	GRPCAddr string
	HTTPAddr string
}

type Config struct {
	Root, Workspace, UI, GRPCAddr, HTTPAddr string
	Database                                storage.Config
	LegacyState                             string
}

func Start(root, database, ui, grpcAddr, httpAddr string) (*Running, error) {
	return StartConfigured(Config{Root: root, UI: ui, GRPCAddr: grpcAddr, HTTPAddr: httpAddr, Database: storage.Config{Driver: "sqlite", DSN: database}})
}
func StartConfigured(config Config) (*Running, error) {
	root, ui, grpcAddr, httpAddr := config.Root, config.UI, config.GRPCAddr, config.HTTPAddr
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	store, err := storage.Open(ctx, config.Database)
	if err != nil {
		return nil, err
	}
	service, err := review.NewWithStore(root, config.Workspace, store, review.GH{})
	if err != nil {
		store.Close()
		return nil, err
	}
	workspace := config.Workspace
	if workspace == "" {
		workspace, _ = filepath.Abs(root)
		workspace, _ = filepath.EvalSymlinks(workspace)
	}
	legacy := config.LegacyState
	if legacy == "" && config.Database.Driver != "postgres" {
		target, _ := filepath.Abs(config.Database.DSN)
		defaultPath, _ := filepath.Abs(filepath.Join(root, ".lgtm/lgtm.db"))
		if target == defaultPath {
			legacy = filepath.Join(root, ".lgtm/state.json")
		}
	}
	if legacy != "" {
		if _, statErr := os.Stat(legacy); statErr == nil {
			if err = store.ImportJSON(ctx, workspace, legacy); err != nil {
				service.Close()
				return nil, err
			}
		} else if config.LegacyState != "" || !os.IsNotExist(statErr) {
			service.Close()
			return nil, fmt.Errorf("legacy import source: %w", statErr)
		}
	}
	succeeded := false
	defer func() {
		if !succeeded {
			service.Close()
		}
	}()

	listener, err := net.Listen("tcp", grpcAddr)
	if err != nil {
		return nil, err
	}
	server := grpc.NewServer(grpc.MaxRecvMsgSize(32<<20), grpc.MaxSendMsgSize(32<<20), grpc.UnaryInterceptor(transport.Interceptor))
	transport.Register(server, service)
	healthServer := health.NewServer()
	healthpb.RegisterHealthServer(server, healthServer)
	reflection.Register(server)
	healthServer.SetServingStatus("", healthpb.HealthCheckResponse_SERVING)
	healthServer.SetServingStatus(lgtmv1connect.ReviewServiceName, healthpb.HealthCheckResponse_SERVING)
	go server.Serve(listener)
	internalAddress := listener.Addr().String()
	host, port, _ := net.SplitHostPort(internalAddress)
	if host == "0.0.0.0" || host == "::" {
		internalAddress = net.JoinHostPort("localhost", port)
	}
	conn, err := grpc.NewClient(internalAddress, grpc.WithTransportCredentials(insecure.NewCredentials()), grpc.WithDefaultCallOptions(grpc.MaxCallRecvMsgSize(32<<20), grpc.MaxCallSendMsgSize(32<<20)))
	if err != nil {
		server.Stop()
		return nil, err
	}
	mux := http.NewServeMux()
	rpcPath, rpcHandler := lgtmv1connect.NewReviewServiceHandler(transport.NewBrowserProxy(conn))
	mux.Handle(rpcPath, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if origin := r.Header.Get("Origin"); origin != "" {
			u, err := url.Parse(origin)
			if err != nil || u.Host != r.Host {
				http.Error(w, "same origin required", http.StatusForbidden)
				return
			}
		}
		rpcHandler.ServeHTTP(w, r)
	}))
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		ctx, cancel := context.WithTimeout(r.Context(), 2*time.Second)
		defer cancel()
		if err := store.Ping(ctx); err != nil {
			http.Error(w, "database unavailable", http.StatusServiceUnavailable)
			return
		}
		w.WriteHeader(http.StatusOK)
	})
	mux.HandleFunc("/api", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fail := func(code int, err error) {
			w.WriteHeader(code)
			_ = json.NewEncoder(w).Encode(review.Response{Error: err.Error()})
		}
		if r.Method != "POST" {
			fail(405, fmt.Errorf("POST required"))
			return
		}
		if origin := r.Header.Get("Origin"); origin != "" {
			u, err := url.Parse(origin)
			if err != nil || u.Host != r.Host {
				fail(403, fmt.Errorf("same origin required"))
				return
			}
		}
		if r.Header.Get("Content-Type") != "application/json" {
			fail(415, fmt.Errorf("application/json required"))
			return
		}
		var q review.Request
		dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1<<20))
		dec.DisallowUnknownFields()
		if err := dec.Decode(&q); err != nil {
			fail(400, err)
			return
		}
		ctx, cancel := context.WithTimeout(r.Context(), 30*time.Second)
		defer cancel()
		out, err := transport.Call(ctx, conn, q)
		if err != nil {
			fail(400, err)
			return
		}
		_ = json.NewEncoder(w).Encode(out)
	})
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		path := filepath.Join(ui, filepath.Clean("/"+r.URL.Path))
		if info, err := os.Stat(path); err == nil && !info.IsDir() {
			http.ServeFile(w, r, path)
			return
		}
		http.ServeFile(w, r, filepath.Join(ui, "index.html"))
	})
	httpListener, err := net.Listen("tcp", httpAddr)
	if err != nil {
		conn.Close()
		server.Stop()
		return nil, err
	}
	web := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	go web.Serve(httpListener)
	fmt.Printf("gRPC %s · web http://%s · checkout %s\n", listener.Addr(), httpListener.Addr(), root)
	succeeded = true
	var once sync.Once
	return &Running{GRPCAddr: listener.Addr().String(), HTTPAddr: httpListener.Addr().String(), Close: func() {
		once.Do(func() {
			healthServer.Shutdown()
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			_ = web.Shutdown(ctx)
			done := make(chan struct{})
			go func() { server.GracefulStop(); close(done) }()
			select {
			case <-done:
			case <-ctx.Done():
				server.Stop()
			}
			_ = conn.Close()
			_ = service.Close()
		})
	}}, nil
}
