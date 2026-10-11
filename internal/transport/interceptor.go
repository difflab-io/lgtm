package transport

import (
	"context"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"log/slog"
	"time"
)

// Interceptor bounds request work and turns unexpected panics into server errors.
func Interceptor(ctx context.Context, req any, info *grpc.UnaryServerInfo, next grpc.UnaryHandler) (out any, err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("RPC panic", "method", info.FullMethod, "panic", recovered)
			err = status.Error(codes.Internal, "internal server error")
		}
	}()
	ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	return next(ctx, req)
}
