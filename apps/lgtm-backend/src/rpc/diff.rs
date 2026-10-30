//! Diff transport mapping. The content service has no review persistence dependency.
use super::Api;
use crate::{models as m, proto::lgtm::v1::*};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};

impl DiffService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_diff"))]
    async fn get_diff<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetDiffRequest>,
    ) -> ServiceResult<impl Encodable<GetDiffResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let selected = if request.review_id.is_empty() {
            self.diffs
                .get_diff(
                    m::ReviewTarget::from_proto(&request.target)?,
                    m::ReviewComparison::from_proto(&request.comparison)?,
                )
                .await
        } else {
            let session = super::id(&request.review_id)?;
            let revision = super::optional(&request.revision_id)
                .map(|id| super::id(&id))
                .transpose()?;
            let snapshot = self
                .reviews
                .get_snapshot(&session, revision.as_ref())
                .await
                .map_err(ConnectError::from)?;
            self.diffs
                .get_diff(
                    m::ReviewTarget::LocalGit {
                        path: snapshot.root,
                        branch: None,
                    },
                    Some(m::ReviewComparison::Branch {
                        base_commit: snapshot.base_commit,
                        head_commit: snapshot.head_commit,
                    }),
                )
                .await
        }
        .map_err(ConnectError::from)?;
        Response::ok(GetDiffResponse {
            diff: Some(m::DiffComparison::into_proto(selected)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_files"))]
    async fn get_files<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetFilesRequest>,
    ) -> ServiceResult<impl Encodable<GetFilesResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let selected = m::DiffComparison::from_proto(&request.diff)?;
        let files = self
            .diffs
            .get_files(&selected)
            .await
            .map_err(ConnectError::from)?;
        let prefix = format!("{}:", selected.observation_token);
        let offset = if request.page_token.is_empty() {
            0
        } else {
            request
                .page_token
                .strip_prefix(&prefix)
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|offset| *offset <= files.len())
                .ok_or_else(|| {
                    ConnectError::failed_precondition("File page belongs to another observation")
                })?
        };
        let size = if request.page_size == 0 {
            100
        } else {
            request.page_size.min(500)
        } as usize;
        let next = if offset + size < files.len() {
            format!("{prefix}{}", offset + size)
        } else {
            String::new()
        };
        Response::ok(GetFilesResponse {
            files: files
                .into_iter()
                .skip(offset)
                .take(size)
                .map(m::DiffFile::into_proto)
                .collect(),
            next_page_token: next,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_file"))]
    async fn get_file<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetFileRequest>,
    ) -> ServiceResult<impl Encodable<GetFileResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let selected = m::DiffComparison::from_proto(&request.diff)?;
        let content = self
            .diffs
            .get_file(&selected, &request.path)
            .await
            .map_err(ConnectError::from)?;
        Response::ok(GetFileResponse {
            file: Some(m::DiffFile::into_proto(content.file)).into(),
            old_side: content.old.map(m::ContentSide::into_proto).into(),
            new_side: content.new.map(m::ContentSide::into_proto).into(),
            ..Default::default()
        })
    }
}
