//! Generated Rust RPC clients shared by the human and machine CLI commands.
use crate::{backend::proto::lgtm::v1::*, store::Store};
use anyhow::{Context, Result, bail};
use connectrpc::{
    Protocol,
    client::{CallOptions, ClientConfig, HttpClient},
};
use serde_json::Value;
use std::time::Duration;

pub struct Client {
    endpoint_url: String,
    identity: Option<String>,
}

impl Client {
    pub async fn call(&self, method: &str, input: Value) -> Result<Value> {
        let config = ClientConfig::new(self.endpoint_url.parse()?).with_protocol(Protocol::Grpc);
        let options = match &self.identity {
            Some(id) => CallOptions::default().with_header("x-lgtm-identity", id.as_str()),
            None => CallOptions::default(),
        }
        .with_timeout(Duration::from_secs(30));
        let transport = HttpClient::plaintext_http2_only();
        macro_rules! invoke {
            ($client:ident, $request:ident, $method:ident) => {{
                let request: $request = serde_json::from_value(input)?;
                let response = $client::new(transport, config)
                    .$method(request, options)
                    .await?
                    .into_owned();
                Ok(serde_json::to_value(response)?)
            }};
        }
        match method {
            "EnsureIdentity" => invoke!(
                IdentityServiceClient,
                EnsureIdentityRequest,
                ensure_identity_with_options
            ),
            "GetCapabilities" => invoke!(
                IdentityServiceClient,
                GetCapabilitiesRequest,
                get_capabilities_with_options
            ),
            "EnsureContext" => invoke!(
                ReviewServiceClient,
                EnsureContextRequest,
                ensure_context_with_options
            ),
            "StartSession" => invoke!(
                ReviewServiceClient,
                StartSessionRequest,
                start_session_with_options
            ),
            "GetSession" => invoke!(
                ReviewServiceClient,
                GetSessionRequest,
                get_session_with_options
            ),
            "ListSessions" => invoke!(
                ReviewServiceClient,
                ListSessionsRequest,
                list_sessions_with_options
            ),
            "GetConversation" => invoke!(
                ReviewServiceClient,
                GetConversationRequest,
                get_conversation_with_options
            ),
            "AddComment" => invoke!(
                ReviewServiceClient,
                AddCommentRequest,
                add_comment_with_options
            ),
            "Reply" => invoke!(ReviewServiceClient, ReplyRequest, reply_with_options),
            "EditComment" => invoke!(
                ReviewServiceClient,
                EditCommentRequest,
                edit_comment_with_options
            ),
            "DeleteComment" => invoke!(
                ReviewServiceClient,
                DeleteCommentRequest,
                delete_comment_with_options
            ),
            "ResolveThread" => invoke!(
                ReviewServiceClient,
                ResolveThreadRequest,
                resolve_thread_with_options
            ),
            "CompleteSession" => invoke!(
                ReviewServiceClient,
                CompleteSessionRequest,
                complete_session_with_options
            ),
            "Export" => invoke!(ReviewServiceClient, ExportRequest, export_with_options),
            "GetWorkspace" => invoke!(
                ReviewServiceClient,
                GetWorkspaceRequest,
                get_workspace_with_options
            ),
            "PromoteNotes" => invoke!(
                ReviewServiceClient,
                PromoteNotesRequest,
                promote_notes_with_options
            ),
            "ListPullRequests" => invoke!(
                ReviewServiceClient,
                ListPullRequestsRequest,
                list_pull_requests_with_options
            ),
            "RefreshContext" => invoke!(
                ReviewServiceClient,
                RefreshContextRequest,
                refresh_context_with_options
            ),
            "SubmitReview" => invoke!(
                ReviewServiceClient,
                SubmitReviewRequest,
                submit_review_with_options
            ),
            "ReconcileSubmission" => invoke!(
                ReviewServiceClient,
                ReconcileSubmissionRequest,
                reconcile_submission_with_options
            ),
            _ => bail!("Unsupported RPC method: {method}"),
        }
    }

    pub fn with_identity(&self, identity: String) -> Self {
        Self {
            endpoint_url: self.endpoint_url.clone(),
            identity: Some(identity),
        }
    }

    pub fn new(store: &Store) -> Result<Self> {
        let endpoint_url = std::env::var("LGTM_RPC_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                store
                    .instance()
                    .map(|instance| format!("{}/rpc", instance.url()))
            })
            .context("LGTM backend is unavailable; run lgtm up --no-open or set LGTM_RPC_URL")?;
        Ok(Self {
            endpoint_url,
            identity: None,
        })
    }
}
