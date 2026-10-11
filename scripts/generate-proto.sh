#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export GOBIN="$PWD/.tmp/proto-tools"
mkdir -p "$GOBIN"
export PATH="$GOBIN:$PWD/node_modules/.bin:$PATH"
go install github.com/bufbuild/buf/cmd/buf@v1.57.2
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.5.1
go install connectrpc.com/connect/cmd/protoc-gen-connect-go@v1.18.1
buf format -w
buf lint
buf generate
npx prettier --write apps/lgtm-ts-client/gen/lgtm/v1/review_pb.ts
