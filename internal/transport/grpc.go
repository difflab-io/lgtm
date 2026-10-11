// Package transport implements the generated lgtm.v1 gRPC service.
package transport

import (
	"connectrpc.com/connect"
	"context"
	"errors"
	"fmt"
	pb "github.com/difflab-io/lgtm/gen/go/lgtm/v1"
	"github.com/difflab-io/lgtm/internal/review"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"os"
	"strings"
)

type executor interface {
	Execute(context.Context, review.Request) (review.Response, error)
}
type Server struct {
	pb.UnimplementedReviewServiceServer
	service executor
}

func Register(server *grpc.Server, service executor) {
	pb.RegisterReviewServiceServer(server, &Server{service: service})
}
func rpcError(err error) error {
	if err == nil {
		return nil
	}
	if errors.Is(err, context.Canceled) {
		return status.Error(codes.Canceled, "request canceled")
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return status.Error(codes.DeadlineExceeded, "request deadline exceeded")
	}
	if errors.Is(err, os.ErrNotExist) {
		return status.Error(codes.NotFound, err.Error())
	}
	code := codes.Internal
	var domain *review.DomainError
	if errors.As(err, &domain) {
		switch domain.Kind {
		case review.InvalidArgument:
			code = codes.InvalidArgument
		case review.NotFound:
			code = codes.NotFound
		case review.FailedPrecondition:
			code = codes.FailedPrecondition
		case review.UpstreamFailure:
			code = codes.Unavailable
		}
	}
	return status.Error(code, err.Error())
}
func workspaceToProto(out review.Response) *pb.Workspace {
	return &pb.Workspace{RepoPath: out.RepoPath, State: stateToProto(out.State)}
}
func fileKind(all bool) string {
	if all {
		return "all"
	}
	return ""
}
func resolutionEvent(resolved bool) string {
	if resolved {
		return "RESOLVE"
	}
	return "REOPEN"
}
func actorToDomain(a *pb.Actor) review.Actor {
	if a == nil {
		return review.Actor{}
	}
	kind := ""
	switch a.Kind {
	case pb.ActorKind_ACTOR_KIND_HUMAN:
		kind = "human"
	case pb.ActorKind_ACTOR_KIND_AGENT:
		kind = "agent"
	}
	return review.Actor{Name: a.Name, Kind: kind}
}
func actorToProto(a review.Actor) *pb.Actor {
	kind := pb.ActorKind_ACTOR_KIND_UNSPECIFIED
	if a.Kind == "human" {
		kind = pb.ActorKind_ACTOR_KIND_HUMAN
	}
	if a.Kind == "agent" {
		kind = pb.ActorKind_ACTOR_KIND_AGENT
	}
	return &pb.Actor{Name: a.Name, Kind: kind}
}
func kindToDomain(k pb.ReviewKind) string {
	switch k {
	case pb.ReviewKind_REVIEW_KIND_LOCAL:
		return "local"
	case pb.ReviewKind_REVIEW_KIND_GITHUB:
		return "github"
	}
	return ""
}
func sideToDomain(side pb.DiffSide) string {
	switch side {
	case pb.DiffSide_DIFF_SIDE_UNSPECIFIED, pb.DiffSide_DIFF_SIDE_RIGHT:
		return "RIGHT"
	case pb.DiffSide_DIFF_SIDE_LEFT:
		return "LEFT"
	}
	return "invalid"
}
func eventToDomain(event pb.ReviewEvent) string {
	switch event {
	case pb.ReviewEvent_REVIEW_EVENT_COMMENT:
		return "COMMENT"
	case pb.ReviewEvent_REVIEW_EVENT_APPROVE:
		return "APPROVE"
	case pb.ReviewEvent_REVIEW_EVENT_REQUEST_CHANGES:
		return "REQUEST_CHANGES"
	}
	return ""
}
func stateToProto(state review.State) *pb.State {
	result := &pb.State{Reviews: []*pb.Review{}, Comments: []*pb.Comment{}}
	for _, r := range state.Reviews {
		kind := pb.ReviewKind_REVIEW_KIND_LOCAL
		if r.Kind == "github" {
			kind = pb.ReviewKind_REVIEW_KIND_GITHUB
		}
		value := r.Status
		if value == "draft" {
			value = "DRAFT"
		}
		result.Reviews = append(result.Reviews, &pb.Review{Id: r.ID, Kind: kind, Repo: r.Repo, Pr: int32(r.PR), Head: r.Head, Status: pb.ReviewStatus(pb.ReviewStatus_value["REVIEW_STATUS_"+value]), Body: r.Body, Actor: actorToProto(r.Actor), RemoteId: r.RemoteID})
	}
	for _, c := range state.Comments {
		side := pb.DiffSide_DIFF_SIDE_RIGHT
		if c.Side == "LEFT" {
			side = pb.DiffSide_DIFF_SIDE_LEFT
		}
		result.Comments = append(result.Comments, &pb.Comment{Id: c.ID, ThreadId: c.ThreadID, Resolved: c.Resolved, Path: c.Path, Line: int32(c.Line), Side: side, Body: c.Body, Actor: actorToProto(c.Actor), ReviewId: c.ReviewID})
	}
	return result
}
func filesToProto(files []review.ChangedFile) []*pb.ChangedFile {
	out := []*pb.ChangedFile{}
	for _, f := range files {
		out = append(out, &pb.ChangedFile{Path: f.Path, Status: f.Status})
	}
	return out
}
func pullsToProto(pulls []review.PullRequest) []*pb.PullRequest {
	out := []*pb.PullRequest{}
	for _, p := range pulls {
		out = append(out, &pb.PullRequest{Number: int32(p.Number), Title: p.Title, Draft: p.Draft})
	}
	return out
}

func (s *Server) GetWorkspace(ctx context.Context, q *pb.GetWorkspaceRequest) (*pb.GetWorkspaceResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "state"})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.GetWorkspaceResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) ListFiles(ctx context.Context, q *pb.ListFilesRequest) (*pb.ListFilesResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "files", ID: q.ReviewId, Kind: fileKind(q.IncludeUnchanged)})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.ListFilesResponse{Workspace: workspaceToProto(out), Files: filesToProto(out.Files)}, nil
}

func (s *Server) GetDiff(ctx context.Context, q *pb.GetDiffRequest) (*pb.GetDiffResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "diff", ID: q.ReviewId, Path: q.Path})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.GetDiffResponse{Workspace: workspaceToProto(out), Text: out.Text}, nil
}

func (s *Server) GetFile(ctx context.Context, q *pb.GetFileRequest) (*pb.GetFileResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "file", Path: q.Path})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.GetFileResponse{Workspace: workspaceToProto(out), Text: out.Text}, nil
}

func (s *Server) GetHistory(ctx context.Context, q *pb.GetHistoryRequest) (*pb.GetHistoryResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "history", ID: q.ReviewId})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.GetHistoryResponse{Workspace: workspaceToProto(out), Text: out.Text}, nil
}

func (s *Server) ListPullRequests(ctx context.Context, q *pb.ListPullRequestsRequest) (*pb.ListPullRequestsResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "pulls"})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.ListPullRequestsResponse{Workspace: workspaceToProto(out), Repository: out.Repository, Pulls: pullsToProto(out.Pulls)}, nil
}

func (s *Server) CreateReview(ctx context.Context, q *pb.CreateReviewRequest) (*pb.CreateReviewResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "create", Kind: kindToDomain(q.Kind), Actor: actorToDomain(q.Actor), Body: q.Body, PR: int(q.Pr), Repo: q.Repository})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.CreateReviewResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) AddComment(ctx context.Context, q *pb.AddCommentRequest) (*pb.AddCommentResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "comment", Path: q.Path, Line: int(q.Line), Side: sideToDomain(q.Side), Body: q.Body, Actor: actorToDomain(q.Actor), ReviewID: q.ReviewId})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.AddCommentResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) AddReply(ctx context.Context, q *pb.AddReplyRequest) (*pb.AddReplyResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "reply", ID: q.ThreadId, Body: q.Body, Actor: actorToDomain(q.Actor)})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.AddReplyResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) SetThreadResolved(ctx context.Context, q *pb.SetThreadResolvedRequest) (*pb.SetThreadResolvedResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "resolve", ID: q.ThreadId, Event: resolutionEvent(q.Resolved)})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.SetThreadResolvedResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) PromoteThread(ctx context.Context, q *pb.PromoteThreadRequest) (*pb.PromoteThreadResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "promote", ID: q.ThreadId, ReviewID: q.ReviewId})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.PromoteThreadResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) UpdateReview(ctx context.Context, q *pb.UpdateReviewRequest) (*pb.UpdateReviewResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "edit", ID: q.ReviewId, Body: q.Body})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.UpdateReviewResponse{Workspace: workspaceToProto(out)}, nil
}

func (s *Server) SubmitReview(ctx context.Context, q *pb.SubmitReviewRequest) (*pb.SubmitReviewResponse, error) {
	if q == nil {
		return nil, status.Error(codes.InvalidArgument, "request required")
	}
	out, err := s.service.Execute(ctx, review.Request{Op: "submit", ID: q.ReviewId, Event: eventToDomain(q.Event)})
	if err != nil {
		return nil, rpcError(err)
	}
	return &pb.SubmitReviewResponse{Workspace: workspaceToProto(out)}, nil
}

func responseFromProto(workspace *pb.Workspace) review.Response {
	out := review.Response{State: review.State{Reviews: []review.Review{}, Comments: []review.Comment{}}}
	if workspace == nil {
		return out
	}
	out.RepoPath = workspace.RepoPath
	if workspace.State == nil {
		return out
	}
	for _, r := range workspace.State.Reviews {
		value := strings.TrimPrefix(r.Status.String(), "REVIEW_STATUS_")
		if value == "DRAFT" {
			value = "draft"
		}
		out.State.Reviews = append(out.State.Reviews, review.Review{ID: r.Id, Kind: kindToDomain(r.Kind), Repo: r.Repo, PR: int(r.Pr), Head: r.Head, Status: value, Body: r.Body, Actor: actorToDomain(r.Actor), RemoteID: r.RemoteId})
	}
	for _, c := range workspace.State.Comments {
		out.State.Comments = append(out.State.Comments, review.Comment{ID: c.Id, ThreadID: c.ThreadId, Resolved: c.Resolved, Path: c.Path, Line: int(c.Line), Side: sideToDomain(c.Side), Body: c.Body, Actor: actorToDomain(c.Actor), ReviewID: c.ReviewId})
	}
	return out
}

// Call adapts the CLI/Lua operation vocabulary to generated protobuf RPCs.
// The gRPC server exposes only named, typed methods; there is no Execute RPC.
func Call(ctx context.Context, conn grpc.ClientConnInterface, q review.Request) (review.Response, error) {
	client := pb.NewReviewServiceClient(conn)
	var workspace *pb.Workspace
	var out review.Response
	var err error
	actor := actorToProto(q.Actor)
	kind := pb.ReviewKind(pb.ReviewKind_value["REVIEW_KIND_"+strings.ToUpper(q.Kind)])
	side := pb.DiffSide(pb.DiffSide_value["DIFF_SIDE_"+q.Side])
	if q.Side != "" && q.Side != "LEFT" && q.Side != "RIGHT" {
		side = pb.DiffSide(-1)
	}
	event := pb.ReviewEvent(pb.ReviewEvent_value["REVIEW_EVENT_"+q.Event])
	switch q.Op {
	case "state":
		result, e := client.GetWorkspace(ctx, &pb.GetWorkspaceRequest{})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "files":
		result, e := client.ListFiles(ctx, &pb.ListFilesRequest{ReviewId: q.ID, IncludeUnchanged: q.Kind == "all"})
		err = e
		if e == nil {
			workspace = result.Workspace
			for _, f := range result.Files {
				out.Files = append(out.Files, review.ChangedFile{Path: f.Path, Status: f.Status})
			}
		}
	case "diff":
		result, e := client.GetDiff(ctx, &pb.GetDiffRequest{ReviewId: q.ID, Path: q.Path})
		err = e
		if e == nil {
			workspace = result.Workspace
			out.Text = result.Text
		}
	case "file":
		result, e := client.GetFile(ctx, &pb.GetFileRequest{Path: q.Path})
		err = e
		if e == nil {
			workspace = result.Workspace
			out.Text = result.Text
		}
	case "history":
		result, e := client.GetHistory(ctx, &pb.GetHistoryRequest{ReviewId: q.ID})
		err = e
		if e == nil {
			workspace = result.Workspace
			out.Text = result.Text
		}
	case "pulls":
		result, e := client.ListPullRequests(ctx, &pb.ListPullRequestsRequest{})
		err = e
		if e == nil {
			workspace = result.Workspace
			out.Repository = result.Repository
			out.Pulls = []review.PullRequest{}
			for _, p := range result.Pulls {
				out.Pulls = append(out.Pulls, review.PullRequest{Number: int(p.Number), Title: p.Title, Draft: p.Draft})
			}
		}
	case "create":
		result, e := client.CreateReview(ctx, &pb.CreateReviewRequest{Kind: kind, Actor: actor, Body: q.Body, Pr: int32(q.PR), Repository: q.Repo})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "comment":
		result, e := client.AddComment(ctx, &pb.AddCommentRequest{Path: q.Path, Line: int32(q.Line), Side: side, Body: q.Body, Actor: actor, ReviewId: q.ReviewID})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "reply":
		result, e := client.AddReply(ctx, &pb.AddReplyRequest{ThreadId: q.ID, Body: q.Body, Actor: actor})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "resolve":
		result, e := client.SetThreadResolved(ctx, &pb.SetThreadResolvedRequest{ThreadId: q.ID, Resolved: q.Event != "REOPEN"})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "promote":
		result, e := client.PromoteThread(ctx, &pb.PromoteThreadRequest{ThreadId: q.ID, ReviewId: q.ReviewID})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "edit":
		result, e := client.UpdateReview(ctx, &pb.UpdateReviewRequest{ReviewId: q.ID, Body: q.Body})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	case "submit":
		result, e := client.SubmitReview(ctx, &pb.SubmitReviewRequest{ReviewId: q.ID, Event: event})
		err = e
		if e == nil {
			workspace = result.Workspace
		}
	default:
		return out, status.Error(codes.InvalidArgument, fmt.Sprintf("unknown operation %q", q.Op))
	}
	if err != nil {
		return review.Response{}, err
	}
	snapshot := responseFromProto(workspace)
	out.State = snapshot.State
	out.RepoPath = snapshot.RepoPath
	return out, nil
}

// BrowserProxy serves generated Connect/gRPC-Web endpoints through a real gRPC
// client, so all clients share server validation, interceptors and persistence.
type BrowserProxy struct{ client pb.ReviewServiceClient }

func NewBrowserProxy(conn grpc.ClientConnInterface) *BrowserProxy {
	return &BrowserProxy{client: pb.NewReviewServiceClient(conn)}
}
func connectError(err error) error {
	if err == nil {
		return nil
	}
	st := status.Convert(err)
	return connect.NewError(connect.Code(st.Code()), errors.New(st.Message()))
}
func (p *BrowserProxy) GetWorkspace(ctx context.Context, q *connect.Request[pb.GetWorkspaceRequest]) (*connect.Response[pb.GetWorkspaceResponse], error) {
	out, err := p.client.GetWorkspace(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) ListFiles(ctx context.Context, q *connect.Request[pb.ListFilesRequest]) (*connect.Response[pb.ListFilesResponse], error) {
	out, err := p.client.ListFiles(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) GetDiff(ctx context.Context, q *connect.Request[pb.GetDiffRequest]) (*connect.Response[pb.GetDiffResponse], error) {
	out, err := p.client.GetDiff(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) GetFile(ctx context.Context, q *connect.Request[pb.GetFileRequest]) (*connect.Response[pb.GetFileResponse], error) {
	out, err := p.client.GetFile(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) GetHistory(ctx context.Context, q *connect.Request[pb.GetHistoryRequest]) (*connect.Response[pb.GetHistoryResponse], error) {
	out, err := p.client.GetHistory(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) ListPullRequests(ctx context.Context, q *connect.Request[pb.ListPullRequestsRequest]) (*connect.Response[pb.ListPullRequestsResponse], error) {
	out, err := p.client.ListPullRequests(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) CreateReview(ctx context.Context, q *connect.Request[pb.CreateReviewRequest]) (*connect.Response[pb.CreateReviewResponse], error) {
	out, err := p.client.CreateReview(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) AddComment(ctx context.Context, q *connect.Request[pb.AddCommentRequest]) (*connect.Response[pb.AddCommentResponse], error) {
	out, err := p.client.AddComment(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) AddReply(ctx context.Context, q *connect.Request[pb.AddReplyRequest]) (*connect.Response[pb.AddReplyResponse], error) {
	out, err := p.client.AddReply(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) SetThreadResolved(ctx context.Context, q *connect.Request[pb.SetThreadResolvedRequest]) (*connect.Response[pb.SetThreadResolvedResponse], error) {
	out, err := p.client.SetThreadResolved(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) PromoteThread(ctx context.Context, q *connect.Request[pb.PromoteThreadRequest]) (*connect.Response[pb.PromoteThreadResponse], error) {
	out, err := p.client.PromoteThread(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) UpdateReview(ctx context.Context, q *connect.Request[pb.UpdateReviewRequest]) (*connect.Response[pb.UpdateReviewResponse], error) {
	out, err := p.client.UpdateReview(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
func (p *BrowserProxy) SubmitReview(ctx context.Context, q *connect.Request[pb.SubmitReviewRequest]) (*connect.Response[pb.SubmitReviewResponse], error) {
	out, err := p.client.SubmitReview(ctx, q.Msg)
	if err != nil {
		return nil, connectError(err)
	}
	return connect.NewResponse(out), nil
}
