//! Dummy forge and orchestration for integration tests only.
use db::{
    error::{Error, Result},
    models::*,
    services::ReviewService,
};
use lgtm_backend as db;
use lgtm_backend::forge::models::*;

// fixtures --------------------------------------------------------------------
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

#[allow(async_fn_in_trait)]
pub trait ReviewForgeProvider {
    fn is_stub(&self) -> bool;

    async fn fetch(&self, key: &ContextKey) -> Result<ForgeSnapshot>;

    async fn head(&self, key: &ContextKey) -> Result<String>;
}

/// A fixture is explicit and always labeled STUB; it never proves remote freshness.
pub struct FixtureProvider(pub ForgeSnapshot);

impl ReviewForgeProvider for FixtureProvider {
    fn is_stub(&self) -> bool {
        true
    }

    async fn fetch(&self, _: &ContextKey) -> Result<ForgeSnapshot> {
        Ok(self.0.clone())
    }

    async fn head(&self, _: &ContextKey) -> Result<String> {
        Ok(self.0.head.clone())
    }
}

pub fn rendered_body(c: &Comment) -> String {
    match c.author.model() {
        Some(model) => format!("{}\n\n> Generated via {model}", c.body),
        None => c.body.clone(),
    }
}

/// Provider work is performed here; persistence accepts completed observations and receipts.
#[allow(async_fn_in_trait)]
pub trait ForgeReview {
    async fn refresh<P: ReviewForgeProvider>(
        &self,
        context: &Context,
        provider: &P,
    ) -> Result<Context>;

    async fn prepare_post(
        &self,
        session_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        request_key: &str,
    ) -> Result<String>;
}

impl ForgeReview for ReviewService {
    async fn refresh<P: ReviewForgeProvider>(
        &self,
        context: &Context,
        provider: &P,
    ) -> Result<Context> {
        if !context.key.is_forge_backed() {
            return Err(invalid("Only PR contexts can be refreshed"));
        }
        let result = async {
            let snapshot = provider.fetch(&context.key).await?;
            if provider.head(&context.key).await? != snapshot.head {
                return Err(Error::Conflict(
                    "PR head changed during fetch; retry refresh".into(),
                ));
            }
            self.apply_remote(context, snapshot, provider.is_stub())
                .await
        }
        .await;
        if let Err(error) = &result {
            self.record_refresh_failure(context, &error.to_string())
                .await?;
        }
        result
    }

    async fn prepare_post(
        &self,
        session_id: &uuid::Uuid,
        comment_id: &uuid::Uuid,
        request_key: &str,
    ) -> Result<String> {
        let session = self.get_session(session_id).await?;
        let comment = self.get_comment(&session.context_id, comment_id).await?;
        let rendered = rendered_body(&comment);
        self.stage_post(session_id, comment_id, request_key, &comment, &rendered)
            .await
    }
}
