//! Transactions for saved remote intents. Provider calls run outside these transactions.
use super::ReviewService;
use crate::{
    database::models::{comments, mutation_receipts, threads},
    error::{Error, Result, invalid},
    models::{Actor, ForgeChange, ForgeChangeInput, ForgeChangeObject, MutationGuard},
};
use uuid::Uuid;

impl ReviewService {
    pub(crate) async fn get_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
    ) -> Result<Option<ForgeChange>> {
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let Some(receipt) = self
            .database
            .mutations
            .get_result(&session.context_id, &actor.id, &guard.request_key)
            .await?
        else {
            return Ok(None);
        };
        if receipt.method != input.method() || receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict(
                "Retry key belongs to a different remote change".into(),
            ));
        }
        Ok(Some(serde_json::from_str(&receipt.result_json)?))
    }

    pub(crate) async fn prepare_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
    ) -> Result<(ForgeChange, bool)> {
        let actor = self.auth.validate_requester(actor).await?;
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let tx = self.database.begin().await?;
        if let Some(saved) = self
            .begin_mutation::<ForgeChange>(
                &tx,
                &session.context_id,
                &actor.id,
                input.method(),
                Some(&guard),
                true,
            )
            .await?
        {
            tx.commit().await?;
            return Ok((saved, false));
        }
        let (_, object_id) = input.object();
        if tx
            .mutations
            .get_pending(&session.context_id, Some(&object_id))
            .await?
        {
            return Err(Error::Conflict(
                "This object has an unresolved remote change; retry its saved intent first".into(),
            ));
        }
        let context = self.load_context(&tx, &session.context_id).await?;
        if !context.key.is_forge_backed() {
            return Err(invalid("Remote changes require a PR context"));
        }
        let original = match input {
            ForgeChangeInput::Edit { comment_id, .. } | ForgeChangeInput::Delete { comment_id } => {
                if let ForgeChangeInput::Edit { body, .. } = input {
                    crate::models::Comment::validate_body(body)?;
                }
                let row = tx
                    .comments
                    .get_by_id(&session.context_id, comment_id)
                    .await?
                    .ok_or_else(|| Error::NotFound("Comment".into()))?;
                let comment = Self::comment_from_db_model(row, &tx).await?;
                guard.validate_object(crate::models::ObjectKind::Comment, comment.id, &comment)?;
                if comment.origin_session_id.is_some() && comment.owner_id != Some(actor.id) {
                    return Err(invalid("Only the draft owner may change this feedback"));
                }
                if comment.visibility == "draft" {
                    self.require_active_session(&tx, session_id).await?;
                }
                ForgeChangeObject::Comment(Box::new(comment))
            }
            ForgeChangeInput::Resolve { thread_id, .. } => {
                let thread = self
                    .load_thread(&tx, thread_id, &session.context_id)
                    .await?;
                guard.validate_object(crate::models::ObjectKind::Thread, thread.id, &thread)?;
                ForgeChangeObject::Thread(Box::new(thread))
            }
        };
        let saved = ForgeChange {
            input: input.clone(),
            original,
            result: None,
            rejection: None,
        };
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        tx.mutations
            .insert_result(&mutation_receipts::Model {
                context_id: session.context_id,
                actor_id: actor.id,
                request_key: guard.request_key,
                method: input.method().into(),
                request_hash: guard.payload_hash,
                result_json: serde_json::to_string(&saved)?,
                version,
                object_id: Some(object_id),
                pending: true,
            })
            .await?;
        tx.commit().await?;
        Ok((saved, true))
    }

    pub(crate) async fn finish_forge_change(
        &self,
        session_id: &Uuid,
        actor: &Actor,
        input: &ForgeChangeInput,
        guard: &MutationGuard,
        result: Option<ForgeChangeObject>,
        rejection: Option<String>,
    ) -> Result<ForgeChange> {
        if result.is_none() && rejection.is_none() {
            return Err(invalid(
                "A remote change requires confirmation or definite rejection",
            ));
        }
        let session = self.get_session(session_id).await?;
        let guard = guard.bind_attempt(session_id, input)?;
        let tx = self.database.begin().await?;
        tx.review_contexts
            .lock_for_write(&session.context_id)
            .await?;
        let receipt = tx
            .mutations
            .get_result(&session.context_id, &actor.id, &guard.request_key)
            .await?
            .ok_or_else(|| Error::NotFound("Saved remote intent".into()))?;
        if receipt.method != input.method() || receipt.request_hash != guard.payload_hash {
            return Err(Error::Conflict("Remote intent changed".into()));
        }
        let mut saved: ForgeChange = serde_json::from_str(&receipt.result_json)?;
        if saved.result.is_some() || saved.rejection.is_some() {
            tx.commit().await?;
            return Ok(saved);
        }
        match &result {
            Some(ForgeChangeObject::Comment(comment)) => {
                tx.comments
                    .update(&comments::Model::from(comment.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Thread(thread)) => {
                tx.comments
                    .update_thread(&threads::Model::from(thread.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Deleted { .. }) => {
                return Err(invalid("A remote receipt requires a saved object"));
            }
            None => {}
        }
        saved.result = result;
        saved.rejection = rejection;
        tx.review_contexts
            .advance_revision(&session.context_id, None)
            .await?;
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        if !tx
            .mutations
            .update_result(&receipt, &serde_json::to_string(&saved)?, version, false)
            .await?
        {
            return Err(Error::Conflict("Remote receipt changed".into()));
        }
        tx.commit().await?;
        Ok(saved)
    }
}
