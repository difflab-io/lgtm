//! Typed Connect/gRPC boundary over the existing application services.
mod diff;
mod review;
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
    forge: crate::services::forge::ForgeService,
    diffs: crate::services::diff::DiffService,
}
impl Api {
    pub fn new(database: Database) -> Self {
        let diffs = crate::services::diff::DiffService::new();
        let reviews =
            Reviews::new(database.clone(), AuthService::new(database)).with_sources(diffs.clone());
        Self {
            forge: crate::services::forge::ForgeService::new(reviews.clone()),
            diffs,
            reviews,
        }
    }
}

pub(crate) fn id(value: &str) -> Result<Uuid, ConnectError> {
    Uuid::parse_str(value).map_err(|_| ConnectError::invalid_argument("Expected UUID"))
}
pub(crate) fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
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
        .map_err(ConnectError::from)?;
        Response::ok(EnsureIdentityResponse {
            actor: Some(m::Actor::into_proto(v)).into(),
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
            capabilities: vec![
                "notes".into(),
                "local_review".into(),
                "export".into(),
                "github_pending_drafts".into(),
                "review_targets".into(),
                "feedback_scopes".into(),
                "anchor_evidence".into(),
                "diff_service".into(),
                "file_reviews".into(),
                "rejected_submission_recovery".into(),
            ],
            ..Default::default()
        })
    }
}
