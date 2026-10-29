//! Diff transport mapping. The content service has no review persistence dependency.
use super::Api;
use crate::{models as m, proto::lgtm::v1::*};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};

impl DiffService for Api {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_source"))]
    async fn get_source<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetSourceRequest>,
    ) -> ServiceResult<impl Encodable<GetSourceResponse> + Send + use<'a>> {
        use crate::database::types::SourceKind;
        let request = request.to_owned_message();
        let kind = match request.kind.to_i32() {
            1 => SourceKind::Commit,
            2 => SourceKind::IndexFile,
            3 => SourceKind::WorkingFile,
            4 => SourceKind::LocalFile,
            _ => return Err(ConnectError::invalid_argument("Select a saved source kind")),
        };
        let selected = self
            .diffs
            .get_source(
                &request.root,
                &request.path,
                kind,
                super::optional(&request.commit).as_deref(),
            )
            .await
            .map_err(ConnectError::from)?;
        Response::ok(GetSourceResponse {
            selected: selected.map(m::ContentSide::into_proto).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/capture_anchor"))]
    async fn capture_anchor<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, CaptureAnchorRequest>,
    ) -> ServiceResult<impl Encodable<CaptureAnchorResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let anchor = m::Anchor::from_proto(&request.location)?;
        let selected = match request.selection.as_ref() {
            Some(capture_anchor_request::Selection::Saved(source)) => {
                use crate::database::types::SourceKind;
                let kind = match source.kind.to_i32() {
                    1 => SourceKind::Commit,
                    2 => SourceKind::IndexFile,
                    3 => SourceKind::WorkingFile,
                    4 => SourceKind::LocalFile,
                    _ => return Err(ConnectError::invalid_argument("Select a saved source kind")),
                };
                self.diffs
                    .get_source(
                        &source.root,
                        &source.path,
                        kind,
                        super::optional(&source.commit).as_deref(),
                    )
                    .await
            }
            Some(capture_anchor_request::Selection::File(file)) => {
                let diff = m::DiffComparison::from_proto(&file.diff)?;
                let content = self
                    .diffs
                    .get_file(&diff, &file.path)
                    .await
                    .map_err(ConnectError::from)?;
                match request.side.as_str() {
                    "old" => Ok(content.old),
                    "new" => Ok(content.new),
                    _ => {
                        return Err(ConnectError::invalid_argument(
                            "Select the old or new source side",
                        ));
                    }
                }
            }
            None => {
                return Err(ConnectError::invalid_argument(
                    "Select a backend-readable source",
                ));
            }
        }
        .map_err(ConnectError::from)?
        .ok_or_else(|| ConnectError::failed_precondition("Selected source no longer exists"))?;
        let evidence = selected.capture(&anchor).map_err(ConnectError::from)?;
        Response::ok(CaptureAnchorResponse {
            anchor: Some(evidence.into_proto_anchor(anchor)).into(),
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_anchor_placements"))]
    async fn get_anchor_placements<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetAnchorPlacementsRequest>,
    ) -> ServiceResult<impl Encodable<GetAnchorPlacementsResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        if request.anchors.len() > 500 || request.views.len() > 2 {
            return Err(ConnectError::invalid_argument(
                "Placement accepts at most 500 anchors and two source sides",
            ));
        }
        let anchors = request
            .anchors
            .iter()
            .map(m::SuppliedAnchor::from_proto)
            .collect::<Result<Vec<_>, _>>()?;
        let views = request
            .views
            .iter()
            .map(m::AnchorView::from_proto)
            .collect::<Result<Vec<_>, _>>()?;
        let has_new = views.iter().any(|v| v.side == "new" && v.source.is_some());
        let mut placements = Vec::new();
        for view in &views {
            for (index, anchor) in anchors.iter().enumerate() {
                if let Some(placement) = view.get_placement(anchor, request.notes, has_new) {
                    let mut placement = placement.into_proto(index);
                    placement.side = view.side.clone();
                    placements.push(placement);
                }
            }
        }
        Response::ok(GetAnchorPlacementsResponse {
            placements,
            ..Default::default()
        })
    }

    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1/get_diff"))]
    async fn get_diff<'a>(
        &'a self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, GetDiffRequest>,
    ) -> ServiceResult<impl Encodable<GetDiffResponse> + Send + use<'a>> {
        let request = request.to_owned_message();
        let selected = self
            .diffs
            .get_diff(
                m::ReviewTarget::from_proto(&request.target)?,
                m::ReviewComparison::from_proto(&request.comparison)?,
            )
            .await
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
        if request.anchors.len() > 500 {
            return Err(ConnectError::invalid_argument(
                "Selected file exceeds 500 supplied anchors",
            ));
        }
        let selected = m::DiffComparison::from_proto(&request.diff)?;
        let content = self
            .diffs
            .get_file(&selected, &request.path)
            .await
            .map_err(ConnectError::from)?;
        let mut placements = Vec::with_capacity(request.anchors.len());
        let directory_root = match &selected.target {
            m::ReviewTarget::Directory { path } => {
                Some(std::fs::canonicalize(path).map_err(|_| {
                    ConnectError::failed_precondition("Selected directory is unavailable")
                })?)
            }
            _ => None,
        };
        for (index, candidate) in request.anchors.iter().enumerate() {
            let placement = m::SuppliedAnchor::from_proto(candidate)?
                .get_placement(&content, directory_root.as_deref())?;
            placements.push(placement.into_proto(index));
        }
        Response::ok(GetFileResponse {
            file: Some(m::DiffFile::into_proto(content.file)).into(),
            old_side: content.old.map(m::ContentSide::into_proto).into(),
            new_side: content.new.map(m::ContentSide::into_proto).into(),
            placements,
            ..Default::default()
        })
    }
}
