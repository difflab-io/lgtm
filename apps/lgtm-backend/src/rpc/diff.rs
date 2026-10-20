//! Diff transport mapping. The content service has no review persistence dependency.
use super::{
    Api, anchor, error, input_anchor, input_comparison, input_evidence, input_target,
    output_comparison, output_target, source,
};
use crate::{models as m, proto::lgtm::v1::*};
use connectrpc::{
    ConnectError, Encodable, RequestContext, Response, ServiceRequest, ServiceResult,
};

fn descriptor(value: m::DiffComparison) -> DiffDescriptor {
    DiffDescriptor {
        target: Some(output_target(value.target)).into(),
        comparison: Some(output_comparison(value.comparison)).into(),
        root: value.root.unwrap_or_default(),
        observation_token: value.observation_token,
        ..Default::default()
    }
}

fn input_descriptor(value: &DiffDescriptor) -> Result<m::DiffComparison, ConnectError> {
    Ok(m::DiffComparison {
        target: input_target(&value.target)?,
        comparison: input_comparison(&value.comparison)?.ok_or_else(|| {
            ConnectError::invalid_argument("Comparison descriptor is missing its pinned selection")
        })?,
        root: (!value.root.is_empty()).then(|| value.root.clone()),
        observation_token: value.observation_token.clone(),
    })
}

fn file(value: m::DiffFile) -> DiffFile {
    DiffFile {
        path: value.path,
        previous_path: value.previous_path,
        status: value.status,
        identity: value.identity,
        ..Default::default()
    }
}

fn side(value: m::ContentSide) -> ContentSide {
    ContentSide {
        path: value.path,
        source: Some(source(value.source)).into(),
        content: value.content,
        ..Default::default()
    }
}

fn placement(index: usize, value: m::AnchorPlacement) -> AnchorPlacement {
    use anchor_placement::Kind;
    let kind = match value {
        m::AnchorPlacement::Exact {
            location,
            displayed_source,
        } => Kind::Exact(Box::new(ConfirmedPlacement {
            location: Some(anchor(location)).into(),
            displayed_source: Some(source(displayed_source)).into(),
            ..Default::default()
        })),
        m::AnchorPlacement::Relocated {
            location,
            displayed_source,
        } => Kind::Relocated(Box::new(ConfirmedPlacement {
            location: Some(anchor(location)).into(),
            displayed_source: Some(source(displayed_source)).into(),
            ..Default::default()
        })),
        m::AnchorPlacement::Outdated { reason } => Kind::Outdated(Box::new(UnplacedAnchor {
            reason,
            ..Default::default()
        })),
        m::AnchorPlacement::Orphaned { reason } => Kind::Orphaned(Box::new(UnplacedAnchor {
            reason,
            ..Default::default()
        })),
        m::AnchorPlacement::Unverified { reason } => Kind::Unverified(Box::new(UnplacedAnchor {
            reason,
            ..Default::default()
        })),
    };
    AnchorPlacement {
        index: index as u32,
        kind: Some(kind),
        ..Default::default()
    }
}

impl DiffService for Api {
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
                input_target(&request.target)?,
                input_comparison(&request.comparison)?,
            )
            .await
            .map_err(error)?;
        Response::ok(GetDiffResponse {
            diff: Some(descriptor(selected)).into(),
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
        let selected = input_descriptor(&request.diff)?;
        let files = self.diffs.get_files(&selected).await.map_err(error)?;
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
                .map(file)
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
        let selected = input_descriptor(&request.diff)?;
        let content = self
            .diffs
            .get_file(&selected, &request.path)
            .await
            .map_err(error)?;
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
            let original = input_anchor(&candidate.original)?;
            let evidence = input_evidence(&candidate.original)?;
            if !(candidate.current_path.is_empty()
                || directory_root.is_some()
                    && std::path::Path::new(&candidate.current_path).is_absolute())
            {
                m::Anchor::File {
                    path: candidate.current_path.clone(),
                }
                .validate()
                .map_err(error)?;
            }
            let original_path = match &original {
                m::Anchor::File { path } | m::Anchor::Line { path, .. } => path.as_str(),
                m::Anchor::General => "",
            };
            let displayed = match &original {
                m::Anchor::Line { side, .. } if side == "old" => content.old.as_ref(),
                m::Anchor::File { .. } => content.new.as_ref().or(content.old.as_ref()),
                _ => content.new.as_ref(),
            };
            let path = displayed
                .map(|s| s.path.as_str())
                .unwrap_or(content.file.path.as_str());
            let renamed = !content.file.previous_path.is_empty()
                && (content.file.status == "renamed" || content.file.status.starts_with('R'));
            let current_matches = if let Some(root) = directory_root
                .as_ref()
                .filter(|_| std::path::Path::new(&candidate.current_path).is_absolute())
            {
                if std::path::Path::new(&candidate.current_path)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err(ConnectError::invalid_argument(
                        "Current source location cannot contain ..",
                    ));
                }
                std::path::Path::new(&candidate.current_path) == root.join(&content.file.path)
            } else {
                candidate.current_path == content.file.path
                    || renamed && candidate.current_path == content.file.previous_path
            };
            // A current association wins over a reused old filename. An exact historical commit remains independently verifiable.
            let historical = evidence.as_ref().is_some_and(|evidence| {
                matches!((&evidence.source, displayed.map(|side| &side.source)),
                    (m::AnchorSource::Commit { sha: origin, .. }, Some(m::AnchorSource::Commit { sha: shown, .. })) if origin == shown && original_path == path)
            });
            let confirmed = if candidate.current_path.is_empty() {
                original_path == content.file.path
                    || renamed && original_path == content.file.previous_path
            } else {
                current_matches || historical
            };
            let result = original.get_placement(
                evidence.as_ref(),
                path,
                displayed.map(|s| &s.source),
                displayed.map(|s| s.content.as_str()),
                confirmed,
            );
            placements.push(placement(index, result));
        }
        Response::ok(GetFileResponse {
            file: Some(file(content.file)).into(),
            old_side: content.old.map(side).into(),
            new_side: content.new.map(side).into(),
            placements,
            ..Default::default()
        })
    }
}
