//! Local reviewed-file marks; PR viewer state is owned by the forge.
use super::*;

impl ReviewService {
    pub async fn get_file_reviews(
        &self,
        review_id: &uuid::Uuid,
        actor: &Actor,
    ) -> Result<Vec<(String, String)>> {
        self.auth.validate_requester(actor).await?;
        let session = self.get_session(review_id).await?;
        let context = self.get_context_by_id(&session.context_id).await?;
        if context.key.is_forge_backed() {
            return Err(invalid("PR file marks belong to the forge"));
        }
        Ok(self
            .database
            .review_contexts
            .get_file_reviews(&context.id, &actor.id)
            .await?)
    }

    pub async fn set_file_reviewed(
        &self,
        review_id: &uuid::Uuid,
        actor: &Actor,
        path: &str,
        content_identity: Option<&str>,
    ) -> Result<()> {
        self.auth.validate_requester(actor).await?;
        Anchor::File { path: path.into() }.validate()?;
        if content_identity.is_some_and(|value| value.is_empty() || value.len() > 4096) {
            return Err(invalid(
                "Reviewed content requires a bounded nonempty identity",
            ));
        }
        let session = self.get_session(review_id).await?;
        let context = self.get_context_by_id(&session.context_id).await?;
        if context.key.is_forge_backed() || session.comparison_kind == ComparisonKind::Notes {
            return Err(invalid("Local reviewed-file marks require a local review"));
        }
        self.database
            .review_contexts
            .set_file_reviewed(&context.id, &actor.id, path, content_identity)
            .await?;
        Ok(())
    }
}
