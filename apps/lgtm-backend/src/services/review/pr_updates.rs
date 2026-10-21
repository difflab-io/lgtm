//! PR identity survives pushes. Only active, unfrozen comparisons follow provider updates.
use super::ReviewService;
use crate::{
    error::{Error, Result, invalid},
    models::Context,
};

impl ReviewService {
    pub(crate) async fn get_posting_anchor(
        &self,
        session: &crate::models::Session,
        thread: &crate::models::Thread,
    ) -> Result<crate::models::Anchor> {
        let context = self.get_context_by_id(&session.context_id).await?;
        let crate::models::ContextKey::ForgeBacked {
            repository_url,
            number,
        } = &context.key
        else {
            return Err(invalid("Posting requires a PR context"));
        };
        self.sources
            .get_posting_anchor(
                &context.paths,
                crate::models::ReviewTarget::PullRequest {
                    repository_url: repository_url.as_str().into(),
                    number: *number,
                },
                crate::models::ReviewComparison::PullRequest {
                    base_commit: session
                        .commits
                        .base
                        .clone()
                        .ok_or_else(|| invalid("PR base unavailable"))?,
                    head_commit: session
                        .commits
                        .head
                        .clone()
                        .ok_or_else(|| invalid("PR head unavailable"))?,
                },
                &thread.anchor,
                thread.evidence.as_ref(),
                thread.current_path.as_deref(),
            )
            .await
    }

    pub(crate) async fn synchronize_pr_comparison(&self, observed: &Context) -> Result<Context> {
        if !observed.key.is_forge_backed()
            || observed.forge_state != Some(crate::database::types::ForgeState::Open)
        {
            return Ok(observed.clone());
        }
        let (base, head) = observed
            .refreshed_base
            .as_deref()
            .zip(observed.refreshed_head.as_deref())
            .ok_or_else(|| invalid("PR observation has no complete comparison"))?;
        let tx = self.database.begin().await?;
        let context = self.load_context(&tx, &observed.id).await?;
        if context.revision != observed.revision {
            return Err(Error::Conflict(
                "PR changed during synchronization; refresh again".into(),
            ));
        }
        if let Some(active) = tx.sessions.get_active(&context.id).await?
            && ["unsubmitted", "draft_failed"].contains(&active.submission_status.as_str())
            && (active.base_commit.as_deref() != Some(base)
                || active.head_commit.as_deref() != Some(head))
            && (!tx
                .review_contexts
                .advance_revision(&context.id, Some(context.revision))
                .await?
                || !tx
                    .sessions
                    .update_pr_comparison(&active, base, head)
                    .await?)
        {
            return Err(Error::Conflict(
                "PR review changed during synchronization".into(),
            ));
        }
        tx.commit().await?;
        self.get_context_by_id(&observed.id).await
    }
}
