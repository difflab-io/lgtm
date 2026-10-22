//! Transactions for saved remote intents. Provider calls run outside these transactions.
use super::ReviewService;
use crate::{
    database::models::{comments, mutation_receipts, threads},
    error::{Error, Result, invalid},
    models::{Actor, ForgeChange, ForgeChangeInput, ForgeChangeObject, MutationGuard},
};
use uuid::Uuid;

/// Remote acknowledgments contain no copy of the comment, thread, or request body.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct ChangeReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) rejection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) deleted_remote: Option<(String, String)>,
}

impl ReviewService {
    /// Reopen only after the forge confirms the final event was rejected and all drafts are accounted for.
    pub(crate) async fn resume_rejected_submission(
        &self,
        session_id: &Uuid,
        actor: &Actor,
    ) -> Result<crate::models::Session> {
        let actor = self.auth.validate_requester(actor).await?;
        let seed = self.get_session(session_id).await?;
        let tx = self.database.begin().await?;
        self.lock_context(&tx, &seed.context_id).await?;
        let row = tx
            .sessions
            .get_by_id(session_id)
            .await?
            .ok_or_else(|| Error::NotFound("Review".into()))?;
        let mut session = Self::session_from_db_model(row.clone(), &tx).await?;
        if session.creator.id != actor.id
            || !self
                .load_context(&tx, &session.context_id)
                .await?
                .key
                .is_forge_backed()
        {
            return Err(invalid(
                "Only the PR review owner can recover its rejected submission",
            ));
        }
        if session.state() == "active" && session.submission_status == "unsubmitted" {
            tx.commit().await?;
            return self.get_session(session_id).await;
        }
        let completion = session
            .completion()
            .ok_or_else(|| invalid("No frozen submission to recover"))?
            .clone();
        if tx.sessions.get_active(&session.context_id).await?.is_some() {
            return Err(Error::Conflict(
                "Another review is active; the rejected review was preserved".into(),
            ));
        }
        let comments = Self::comments_from_db_models(
            tx.comments.get_comments(&session.context_id).await?,
            &tx,
        )
        .await?;
        for comment in comments
            .iter()
            .filter(|c| c.origin_session_id == Some(session.id))
        {
            if comment.forge_comment_id.is_none()
                || ["sending", "uncertain"].contains(&comment.posting.state().as_str())
            {
                return Err(Error::Conflict(
                    "Recover unconfirmed comment writes before reopening the rejected review"
                        .into(),
                ));
            }
        }
        session.resume_rejected_submission()?;
        tx.review_contexts
            .advance_revision(&session.context_id, None)
            .await?;
        let version = tx
            .review_contexts
            .get_by_id(&session.context_id)
            .await?
            .ok_or_else(|| Error::NotFound("Context".into()))?
            .revision;
        // A delayed retry of the rejected key must never publish a second outcome.
        tx.mutations
            .insert_result(&mutation_receipts::Model {
                context_id: session.context_id,
                actor_id: actor.id,
                request_key: completion.request_key,
                method: "rejected_submission".into(),
                request_hash: completion.request_hash,
                result_json: "{}".into(),
                version,
                object_id: Some(session.id),
                pending: false,
            })
            .await?;
        if !tx.sessions.reopen_rejected_submission(&row).await? {
            return Err(Error::Conflict(
                "Submission state changed during recovery".into(),
            ));
        }
        for mut comment in comments {
            if comment.origin_session_id == Some(session.id)
                && comment.forge_object_kind.as_deref() == Some("review_comment")
            {
                comment.visibility = "draft".into();
                tx.comments
                    .update_comment(&comments::Model::from(&comment))
                    .await?;
            }
        }
        tx.commit().await?;
        self.get_session(session_id).await
    }

    async fn get_change_from_receipt<D: sea_orm::ConnectionTrait + sea_orm::TransactionTrait>(
        &self,
        database: &crate::database::Database<D>,
        receipt: &mutation_receipts::Model,
        input: &ForgeChangeInput,
    ) -> Result<ForgeChange> {
        let saved: ChangeReceipt = serde_json::from_str(&receipt.result_json)?;
        let (_, object_id) = input.object();
        let original = if saved.rejection.is_some()
            || (!receipt.pending && matches!(input, ForgeChangeInput::Delete { .. }))
        {
            ForgeChangeObject::Deleted {
                comment_id: object_id,
            }
        } else {
            match input {
                ForgeChangeInput::Edit { .. } | ForgeChangeInput::Delete { .. } => {
                    let row = database
                        .comments
                        .get_comment_by_id(&receipt.context_id, &object_id)
                        .await?
                        .ok_or_else(|| Error::NotFound("Comment".into()))?;
                    ForgeChangeObject::Comment(Box::new(
                        Self::comment_from_db_model(row, database).await?,
                    ))
                }
                ForgeChangeInput::Resolve { .. } => {
                    let row = database
                        .comments
                        .get_thread(&receipt.context_id, &object_id)
                        .await?
                        .ok_or_else(|| Error::NotFound("Thread".into()))?;
                    ForgeChangeObject::Thread(Box::new(
                        Self::thread_from_db_model(row, database).await?,
                    ))
                }
            }
        };
        Ok(ForgeChange {
            input: input.clone(),
            result: (!receipt.pending && saved.rejection.is_none()).then(|| original.clone()),
            original,
            rejection: saved.rejection,
            deleted_remote: saved.deleted_remote,
        })
    }

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
        Ok(Some(
            self.get_change_from_receipt(&self.database, &receipt, input)
                .await?,
        ))
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
        if self
            .begin_mutation::<ChangeReceipt>(
                &tx,
                &session.context_id,
                &actor.id,
                input.method(),
                Some(&guard),
                true,
            )
            .await?
            .is_some()
        {
            let receipt = tx
                .mutations
                .get_result(&session.context_id, &actor.id, &guard.request_key)
                .await?
                .ok_or_else(|| Error::NotFound("Remote acknowledgment".into()))?;
            let saved = self.get_change_from_receipt(&tx, &receipt, input).await?;
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
                    .get_comment_by_id(&session.context_id, comment_id)
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
                if matches!(thread.anchor, crate::models::Anchor::General) {
                    return Err(invalid("GitHub only supports resolving review threads"));
                }
                guard.validate_object(crate::models::ObjectKind::Thread, thread.id, &thread)?;
                ForgeChangeObject::Thread(Box::new(thread))
            }
        };
        let saved = ForgeChange {
            input: input.clone(),
            original,
            result: None,
            rejection: None,
            deleted_remote: None,
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
                result_json: serde_json::to_string(&ChangeReceipt::default())?,
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
        mut result: Option<ForgeChangeObject>,
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
        let mut saved = self.get_change_from_receipt(&tx, &receipt, input).await?;
        if saved.result.is_some() || saved.rejection.is_some() {
            tx.commit().await?;
            return Ok(saved);
        }
        if let Some(ForgeChangeObject::Thread(updated)) = &result {
            let mut current = self
                .load_thread(&tx, &updated.id, &session.context_id)
                .await?;
            current.state = updated.state.clone();
            current.updated_at = updated.updated_at.clone();
            current.resolved_at = updated.resolved_at.clone();
            current.resolved_by = updated.resolved_by.clone();
            current.updated_by = updated.updated_by.clone();
            result = Some(ForgeChangeObject::Thread(Box::new(current)));
        }
        match &result {
            Some(ForgeChangeObject::Comment(comment)) => {
                tx.comments
                    .update_comment(&comments::Model::from(comment.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Thread(thread)) => {
                tx.comments
                    .update_thread(&threads::Model::from(thread.as_ref()))
                    .await?;
            }
            Some(ForgeChangeObject::Deleted { comment_id }) => {
                let thread_id = match &saved.original {
                    ForgeChangeObject::Comment(comment) if comment.id == *comment_id => {
                        comment.thread_id
                    }
                    _ => return Err(invalid("Deletion receipt names another comment")),
                };
                tx.comments
                    .delete_remote_comment(&session.context_id, comment_id)
                    .await?;
                tx.comments
                    .delete_empty_thread(&session.context_id, &thread_id)
                    .await?;
                if let ForgeChangeObject::Comment(comment) = &saved.original {
                    saved.deleted_remote = comment
                        .forge_object_kind
                        .clone()
                        .zip(comment.forge_comment_id.clone());
                }
                // Keep only the retry receipt, not a copy of the deleted text.
                saved.original = ForgeChangeObject::Deleted {
                    comment_id: *comment_id,
                };
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
            .update_result(
                &receipt,
                &serde_json::to_string(&ChangeReceipt {
                    rejection: saved.rejection.clone(),
                    deleted_remote: saved.deleted_remote.clone(),
                })?,
                version,
                false,
            )
            .await?
        {
            return Err(Error::Conflict("Remote receipt changed".into()));
        }
        tx.commit().await?;
        Ok(saved)
    }
}
