//! Typed Connect/gRPC boundary over the existing application services.
use crate::{
    database::Database,
    models as m,
    proto::lgtm::v1::*,
    services::{AuthService, ReviewService as Reviews},
};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};
use uuid::Uuid;

pub struct Api {
    pub reviews: Reviews,
}
impl Api {
    pub fn new(database: Database) -> Self {
        Self {
            reviews: Reviews::new(database.clone(), AuthService::new(database)),
        }
    }
}
fn error(error: crate::error::Error) -> ConnectError {
    match error {
        crate::error::Error::Invalid(message) => ConnectError::invalid_argument(message),
        crate::error::Error::NotFound(message) => ConnectError::not_found(message),
        crate::error::Error::Conflict(message) => ConnectError::failed_precondition(message),
        other => {
            tracing::error!(error = %other, "RPC failed");
            ConnectError::internal("Backend operation failed")
        }
    }
}
fn id(value: &str) -> Result<Uuid, ConnectError> {
    Uuid::parse_str(value).map_err(|_| ConnectError::invalid_argument("Expected UUID"))
}
fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}
fn actor(v: m::Actor) -> Actor {
    let mut out = Actor {
        id: v.id.to_string(),
        name: v.name,
        ..Default::default()
    };
    match v.kind {
        m::ActorKind::Human { email } => {
            out.kind = "human".into();
            out.email = email;
        }
        m::ActorKind::Agent { model, provider } => {
            out.kind = "agent".into();
            out.model = model;
            out.provider = provider;
        }
    }
    out
}
fn key(v: m::ContextKey) -> ContextKey {
    let mut out = ContextKey::default();
    match v {
        m::ContextKey::Local {
            location: m::LocalLocation::Notes { path },
        } => {
            out.kind = "notes".into();
            out.path = path;
        }
        m::ContextKey::Local {
            location: m::LocalLocation::Branch { git_dir, branch },
        } => {
            out.kind = "branch".into();
            out.git_dir = git_dir;
            out.branch = branch;
        }
        m::ContextKey::ForgeBacked {
            repository_url,
            number,
        } => {
            out.kind = "forge_backed".into();
            out.repository_url = repository_url.as_str().into();
            out.number = number;
        }
    }
    out
}
fn context(v: m::Context) -> Context {
    Context {
        id: v.id.to_string(),
        key: Some(key(v.key)).into(),
        paths: v.paths,
        refreshed_head: v.refreshed_head.unwrap_or_default(),
        refreshed_base: v.refreshed_base.unwrap_or_default(),
        refresh_status: v.refresh_status,
        refresh_error: v.refresh_error.unwrap_or_default(),
        ..Default::default()
    }
}
fn session(v: m::Session) -> Session {
    let completion = v.completion().cloned();
    Session {
        id: v.id.to_string(),
        context_id: v.context_id.to_string(),
        state: v.state().into(),
        creator: Some(actor(v.creator)).into(),
        base: v.commits.base.unwrap_or_default(),
        head: v.commits.head.unwrap_or_default(),
        created_at: v.created_at,
        outcome: completion
            .as_ref()
            .map(|c| c.outcome.clone())
            .unwrap_or_default(),
        body: completion.and_then(|c| c.body).unwrap_or_default(),
        submission_status: v.submission_status,
        submission_error: v.submission_error.unwrap_or_default(),
        forge_review_id: v.forge_review_id.unwrap_or_default(),
        ..Default::default()
    }
}
fn anchor(v: m::Anchor) -> Anchor {
    let mut out = Anchor::default();
    match v {
        m::Anchor::General => out.kind = "general".into(),
        m::Anchor::File { path } => {
            out.kind = "file".into();
            out.path = path;
        }
        m::Anchor::Line {
            path,
            start,
            end,
            side,
            commit,
        } => {
            out.kind = "line".into();
            out.path = path;
            out.start = start;
            out.end = end;
            out.side = side;
            if commit.starts_with("sha256:") {
                out.content_hash = commit;
            } else {
                out.commit = commit;
            }
        }
    }
    out
}
fn input_anchor(v: &Anchor) -> Result<m::Anchor, ConnectError> {
    if (!v.commit.is_empty() && !v.content_hash.is_empty())
        || (v.kind != "line"
            && (v.start != 0
                || v.end != 0
                || !v.side.is_empty()
                || !v.commit.is_empty()
                || !v.content_hash.is_empty()))
        || ((v.kind.is_empty() || v.kind == "general") && !v.path.is_empty())
    {
        return Err(ConnectError::invalid_argument(
            "Anchor fields do not match its kind",
        ));
    }
    let out = match v.kind.as_str() {
        "" | "general" => m::Anchor::General,
        "file" => m::Anchor::File {
            path: v.path.clone(),
        },
        "line" => m::Anchor::Line {
            path: v.path.clone(),
            start: v.start,
            end: if v.end == 0 { v.start } else { v.end },
            side: if v.side.is_empty() {
                "new".into()
            } else {
                v.side.clone()
            },
            commit: if v.content_hash.is_empty() {
                v.commit.clone()
            } else {
                v.content_hash.clone()
            },
        },
        _ => return Err(ConnectError::invalid_argument("Unknown anchor kind")),
    };
    out.validate().map_err(error)?;
    Ok(out)
}
fn comment(v: m::Comment) -> Comment {
    Comment {
        id: v.id.to_string(),
        context_id: v.context_id.to_string(),
        thread_id: v.thread_id.to_string(),
        origin_session_id: v
            .origin_session_id
            .map(|x| x.to_string())
            .unwrap_or_default(),
        posting_status: v.posting.state().as_str().into(),
        author: Some(actor(v.author)).into(),
        body: v.body,
        position: v.position,
        visibility: v.visibility,
        created_at: v.created_at,
        updated_at: v.updated_at,
        forge_comment_id: v.forge_comment_id.unwrap_or_default(),
        remote_presence: v.remote_presence,
        ..Default::default()
    }
}
fn thread(v: m::Thread) -> Thread {
    Thread {
        id: v.id.to_string(),
        anchor: Some(anchor(v.anchor)).into(),
        state: v.state,
        outdated: v.outdated,
        author: Some(actor(v.author)).into(),
        forge_thread_id: v.forge_thread_id.unwrap_or_default(),
        ..Default::default()
    }
}
impl IdentityService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/ensure_identity"))]
    async fn ensure_identity<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, EnsureIdentityRequest>,
    ) -> ServiceResult<impl Encodable<EnsureIdentityResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        if !request.email.is_empty() && (!request.model.is_empty() || !request.provider.is_empty())
        {
            return Err(ConnectError::invalid_argument(
                "Human email and agent model/provider are mutually exclusive",
            ));
        }
        let v = if request.model.is_empty() && request.provider.is_empty() {
            self.reviews
                .auth()
                .ensure_human(&request.name, &request.email)
                .await
        } else {
            self.reviews
                .auth()
                .ensure_agent(&request.name, &request.model, &request.provider)
                .await
        }
        .map_err(error)?;
        Response::ok(EnsureIdentityResponse {
            actor: Some(actor(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_capabilities"))]
    async fn get_capabilities<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetCapabilitiesRequest>,
    ) -> ServiceResult<impl Encodable<GetCapabilitiesResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let _ = request;
        Response::ok(GetCapabilitiesResponse {
            api_version: "1".into(),
            capabilities: vec!["notes".into(), "local_review".into(), "export".into()],
            ..Default::default()
        })
    }
}
impl ReviewService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/ensure_context"))]
    async fn ensure_context<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, EnsureContextRequest>,
    ) -> ServiceResult<impl Encodable<EnsureContextResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let v = &request.key;
        let valid_shape = match v.kind.as_str() {
            "notes" => {
                v.git_dir.is_empty()
                    && v.branch.is_empty()
                    && v.repository_url.is_empty()
                    && v.number == 0
            }
            "branch" => v.path.is_empty() && v.repository_url.is_empty() && v.number == 0,
            "forge_backed" => v.path.is_empty() && v.git_dir.is_empty() && v.branch.is_empty(),
            _ => false,
        };
        if !valid_shape {
            return Err(ConnectError::invalid_argument(
                "Context fields do not match its kind",
            ));
        }
        let k = match v.kind.as_str() {
            "notes" => {
                let path = std::fs::canonicalize(&v.path).map_err(|e| error(e.into()))?;
                m::ContextKey::Local {
                    location: m::LocalLocation::Notes {
                        path: path.to_string_lossy().into_owned(),
                    },
                }
            }
            "branch" => m::ContextKey::Local {
                location: m::LocalLocation::Branch {
                    git_dir: v.git_dir.clone(),
                    branch: v.branch.clone(),
                },
            },
            "forge_backed" => m::ContextKey::ForgeBacked {
                repository_url: m::ForgeRepositoryUrl::parse(&v.repository_url).map_err(error)?,
                number: v.number,
            },
            _ => {
                return Err(ConnectError::invalid_argument(
                    "Expected notes, branch or forge_backed context kind",
                ));
            }
        };
        let v = self
            .reviews
            .ensure_context(k, optional(&request.workspace_path).as_deref())
            .await
            .map_err(error)?;
        Response::ok(EnsureContextResponse {
            context: Some(context(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/start_session"))]
    async fn start_session<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, StartSessionRequest>,
    ) -> ServiceResult<impl Encodable<StartSessionResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let c = self
            .reviews
            .get_context_by_id(&id(&request.context_id)?)
            .await
            .map_err(error)?;
        let v = self
            .reviews
            .start_session(
                &c,
                &who,
                m::CommitContext {
                    base: optional(&request.base),
                    head: optional(&request.head),
                },
            )
            .await
            .map_err(error)?;
        Response::ok(StartSessionResponse {
            session: Some(session(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_session"))]
    async fn get_session<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetSessionRequest>,
    ) -> ServiceResult<impl Encodable<GetSessionResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let v = self
            .reviews
            .get_session(&id(&request.session_id)?)
            .await
            .map_err(error)?;
        Response::ok(GetSessionResponse {
            session: Some(session(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/list_sessions"))]
    async fn list_sessions<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ListSessionsRequest>,
    ) -> ServiceResult<impl Encodable<ListSessionsResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let rows = self
            .reviews
            .get_sessions(&m::SessionFilter {
                path: optional(&request.path),
                git_dir: optional(&request.git_dir),
                completed: request.completed,
            })
            .await
            .map_err(error)?;
        Response::ok(ListSessionsResponse {
            entries: rows
                .into_iter()
                .map(|(c, s)| SessionEntry {
                    context: Some(context(c)).into(),
                    session: Some(session(s)).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_conversation"))]
    async fn get_conversation<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetConversationRequest>,
    ) -> ServiceResult<impl Encodable<GetConversationResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let v = self
            .reviews
            .get_conversation(&id(&request.context_id)?, request.include_drafts)
            .await
            .map_err(error)?;
        Response::ok(GetConversationResponse {
            context: Some(context(v.context)).into(),
            sessions: v.sessions.into_iter().map(session).collect(),
            threads: v.threads.into_iter().map(thread).collect(),
            comments: v.comments.into_iter().map(comment).collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/add_comment"))]
    async fn add_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, AddCommentRequest>,
    ) -> ServiceResult<impl Encodable<AddCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let v = self
            .reviews
            .add_comment(
                &id(&request.session_id)?,
                &who,
                &request.body,
                input_anchor(&request.anchor)?,
            )
            .await
            .map_err(error)?;
        Response::ok(AddCommentResponse {
            comment: Some(comment(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/reply"))]
    async fn reply<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ReplyRequest>,
    ) -> ServiceResult<impl Encodable<ReplyResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let v = self
            .reviews
            .reply(
                &id(&request.session_id)?,
                &id(&request.thread_id)?,
                &who,
                &request.body,
            )
            .await
            .map_err(error)?;
        Response::ok(ReplyResponse {
            comment: Some(comment(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/edit_comment"))]
    async fn edit_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, EditCommentRequest>,
    ) -> ServiceResult<impl Encodable<EditCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let v = forge
            .edit_comment(
                &id(&request.session_id)?,
                &id(&request.comment_id)?,
                &who,
                &request.body,
            )
            .await
            .map_err(error)?;
        Response::ok(EditCommentResponse {
            comment: Some(comment(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/delete_comment"))]
    async fn delete_comment<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, DeleteCommentRequest>,
    ) -> ServiceResult<impl Encodable<DeleteCommentResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        forge
            .delete_comment(&id(&request.session_id)?, &id(&request.comment_id)?, &who)
            .await
            .map_err(error)?;
        Response::ok(DeleteCommentResponse::default())
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/resolve_thread"))]
    async fn resolve_thread<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ResolveThreadRequest>,
    ) -> ServiceResult<impl Encodable<ResolveThreadResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let v = forge
            .resolve(
                &id(&request.session_id)?,
                &id(&request.thread_id)?,
                &who,
                request.resolved,
            )
            .await
            .map_err(error)?;
        Response::ok(ResolveThreadResponse {
            thread: Some(thread(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/complete_session"))]
    async fn complete_session<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, CompleteSessionRequest>,
    ) -> ServiceResult<impl Encodable<CompleteSessionResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let v = self
            .reviews
            .complete(
                &id(&request.context_id)?,
                &who,
                m::Completion {
                    outcome: optional(&request.outcome),
                    body: optional(&request.body),
                    submit: false,
                },
                &request.request_key,
            )
            .await
            .map_err(error)?;
        Response::ok(CompleteSessionResponse {
            session: Some(session(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/export"))]
    async fn export<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ExportRequest>,
    ) -> ServiceResult<impl Encodable<ExportResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let s = self
            .reviews
            .get_session(&id(&request.session_id)?)
            .await
            .map_err(error)?;
        let mut c = self
            .reviews
            .get_conversation(&s.context_id, true)
            .await
            .map_err(error)?;
        if !request.comment_ids.is_empty() {
            let ids: Vec<_> = request
                .comment_ids
                .iter()
                .map(|x| id(x))
                .collect::<Result<_, _>>()?;
            let threads: Vec<_> = c
                .comments
                .iter()
                .filter(|v| ids.contains(&v.id))
                .map(|v| v.thread_id)
                .collect();
            c.comments.retain(|v| threads.contains(&v.thread_id));
            c.threads.retain(|v| threads.contains(&v.id));
        }
        Response::ok(ExportResponse {
            markdown: {
                let mut markdown = Reviews::export(&c, &s);
                for receipt in self
                    .reviews
                    .database()
                    .promotions
                    .get_for_target(&s.id)
                    .await
                    .map_err(|e| error(e.into()))?
                {
                    markdown.push_str(&format!(
                        "\nPromoted note: {} → {:?} (source retained)\n",
                        receipt.source_comment_id, receipt.target_comment_id
                    ));
                }
                markdown
            },
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_workspace"))]
    async fn get_workspace<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GetWorkspaceRequest>,
    ) -> ServiceResult<impl Encodable<GetWorkspaceResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let c = self
            .reviews
            .get_context_by_id(&id(&request.context_id)?)
            .await
            .map_err(error)?;
        let selected = request.path.clone();
        let (root, files) = if c.key.is_forge_backed() {
            crate::services::forge::ForgeService::new(self.reviews.clone())
                .get_workspace(&c.id)
                .await
                .map_err(error)?
        } else {
            tokio::task::spawn_blocking(move || {
                crate::services::workspace::get_review_workspace(
                    &c,
                    &request.path,
                    &request.mode,
                    &request.base,
                    &request.head,
                )
            })
            .await
            .map_err(|_| ConnectError::internal("Workspace task failed"))?
            .map_err(error)?
        };
        let files: Vec<_> = files
            .into_iter()
            .filter(|(file, _)| {
                selected.is_empty() || std::path::Path::new(&file.path).starts_with(&selected)
            })
            .collect();
        Response::ok(GetWorkspaceResponse {
            root,
            files: files
                .into_iter()
                .map(|(v, base_content)| WorkspaceFile {
                    base_content,
                    path: v.path,
                    content: v.content,
                    content_hash: v.content_hash,
                    status: v.status,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/promote_notes"))]
    async fn promote_notes<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, PromoteNotesRequest>,
    ) -> ServiceResult<impl Encodable<PromoteNotesResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let ids = request
            .comment_ids
            .iter()
            .map(|v| id(v))
            .collect::<Result<Vec<_>, _>>()?;
        let anchors = request
            .anchors
            .iter()
            .map(input_anchor)
            .collect::<Result<Vec<_>, _>>()?;
        let rows = self
            .reviews
            .promote_notes(
                &id(&request.source_context_id)?,
                &id(&request.target_session_id)?,
                &who,
                &ids,
                &anchors,
                &request.request_key,
            )
            .await
            .map_err(error)?;
        Response::ok(PromoteNotesResponse {
            comments: rows.into_iter().map(comment).collect(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/list_pull_requests"))]
    async fn list_pull_requests<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ListPullRequestsRequest>,
    ) -> ServiceResult<impl Encodable<ListPullRequestsResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let (rows, next_page) = forge
            .get_pull_requests(&request.repository_url, &request.query, request.page)
            .await
            .map_err(error)?;
        Response::ok(ListPullRequestsResponse {
            pull_requests: rows
                .into_iter()
                .map(|v| PullRequest {
                    number: v.number,
                    title: v.title,
                    head: v.head,
                    base: v.base,
                    repository_url: v.repository_url,
                    ..Default::default()
                })
                .collect(),
            next_page,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/refresh_context"))]
    async fn refresh_context<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, RefreshContextRequest>,
    ) -> ServiceResult<impl Encodable<RefreshContextResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let v = forge
            .refresh(&id(&request.context_id)?)
            .await
            .map_err(error)?;
        Response::ok(RefreshContextResponse {
            context: Some(context(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/submit_review"))]
    async fn submit_review<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SubmitReviewRequest>,
    ) -> ServiceResult<impl Encodable<SubmitReviewResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let v = forge
            .submit(
                &id(&request.session_id)?,
                &who,
                &request.outcome,
                &request.body,
                &request.request_key,
                &request.expected_head,
            )
            .await
            .map_err(error)?;
        Response::ok(SubmitReviewResponse {
            session: Some(session(v)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/reconcile_submission"))]
    async fn reconcile_submission<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ReconcileSubmissionRequest>,
    ) -> ServiceResult<impl Encodable<ReconcileSubmissionResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let _ = &ctx;
        let who = self.reviews.auth().requester(&ctx).await.map_err(error)?;
        let forge = crate::services::forge::ForgeService::new(self.reviews.clone());
        let v = forge
            .reconcile(&id(&request.session_id)?, &who)
            .await
            .map_err(error)?;
        Response::ok(ReconcileSubmissionResponse {
            session: Some(session(v)).into(),
            ..Default::default()
        })
    }
}
