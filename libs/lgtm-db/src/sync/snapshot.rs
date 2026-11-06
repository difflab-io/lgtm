//! A context-sized sync unit. Checkout associations and local retry receipts never travel.
use crate::{
    entities::*,
    error::{Result, invalid},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub context: review_contexts::Model,
    pub identities: Vec<identities::Model>,
    pub sessions: Vec<sessions::Model>,
    pub threads: Vec<threads::Model>,
    pub comments: Vec<comments::Model>,
    pub file_reviews: Vec<file_reviews::Model>,
}

impl Snapshot {
    // Public ------------------------------------------------------------------

    /// Remove device locations while keeping a valid, unique local context key.
    pub fn remove_device_paths(&mut self) {
        let placeholder = format!("/lgtm-unbound/{}", self.context.id);
        if self.context.notes_path.is_some() {
            self.context.notes_path = Some(placeholder.clone());
        }
        if self.context.git_dir.is_some() {
            self.context.git_dir = Some(placeholder);
        }
        self.context.current_notes_path = None;
        for thread in &mut self.threads {
            thread.source_path = None;
            if thread
                .current_path
                .as_ref()
                .is_some_and(|p| p.starts_with('/'))
            {
                thread.current_path = None;
            }
            if let Some(path) = &mut thread.file_path
                && path.starts_with('/')
            {
                *path = std::path::Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
        }
        for mark in &mut self.file_reviews {
            if mark.path.starts_with('/') {
                mark.path = std::path::Path::new(&mark.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
        }
    }

    /// Check ownership links before applying a payload; SQL constraints check record shapes.
    pub fn validate(&self) -> Result<()> {
        let context = self.context.id;
        let identities: HashSet<Uuid> = self.identities.iter().map(|r| r.id).collect();
        let sessions: HashSet<Uuid> = self.sessions.iter().map(|r| r.id).collect();
        let threads: HashSet<Uuid> = self.threads.iter().map(|r| r.id).collect();
        let comments: HashSet<Uuid> = self.comments.iter().map(|r| r.id).collect();
        let session_exists = |id: Option<Uuid>| id.is_none_or(|id| sessions.contains(&id));
        let actor_exists = |id: Option<Uuid>| id.is_none_or(|id| identities.contains(&id));
        if identities.len() != self.identities.len()
            || sessions.len() != self.sessions.len()
            || threads.len() != self.threads.len()
            || comments.len() != self.comments.len()
            || self.sessions.iter().any(|r| {
                r.context_id != context
                    || !identities.contains(&r.creator_id)
                    || !actor_exists(r.completed_by_id)
            })
            || self.threads.iter().any(|r| {
                r.context_id != context
                    || !identities.contains(&r.author_id)
                    || !actor_exists(r.resolved_by_id)
                    || !actor_exists(r.updated_by_id)
                    || !session_exists(r.origin_session_id)
            })
            || self.comments.iter().any(|r| {
                r.context_id != context
                    || !identities.contains(&r.author_id)
                    || !actor_exists(r.owner_id)
                    || !threads.contains(&r.thread_id)
                    || !session_exists(r.origin_session_id)
            })
            || self
                .file_reviews
                .iter()
                .any(|r| r.context_id != context || !identities.contains(&r.actor_id))
        {
            return Err(invalid(
                "Snapshot contains missing or cross-context references",
            ));
        }
        for thread in &self.threads {
            thread.validate_source()?;
            thread.validate_observation()?;
        }
        Ok(())
    }
}
