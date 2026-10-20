//! Versioned saved-file and directory moves. Paths move without changing original anchor evidence.
use super::*;

impl ReviewService {
    pub(super) async fn apply_location_move(
        &self,
        tx: &db::Transaction,
        context: &Context,
        scope: &ReviewScope,
        previous: &str,
        current: &str,
    ) -> Result<()> {
        use std::path::Path;
        let ContextKey::Local {
            location: LocalLocation::Notes { path: original },
        } = &context.key
        else {
            return Err(invalid(
                "Only saved-file scopes and directory reviews have movable locations",
            ));
        };
        let previous = Self::get_notes_path(previous)?;
        let current = Self::get_notes_path(current)?;
        let known = context.current_notes_path.as_ref().unwrap_or(original);
        if previous == current || matches!(scope, ReviewScope::Review { .. }) && &previous != known
        {
            return Err(Error::Conflict(
                "Move does not name the current saved location".into(),
            ));
        }
        match std::fs::symlink_metadata(&previous) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => {
                return Err(Error::Conflict(
                    "Move source still exists; a copy is not a confirmed move".into(),
                ));
            }
        }
        let directory = match scope {
            ReviewScope::Notes { path } => {
                if Self::get_notes_path(path)? != previous || !Path::new(&current).is_file() {
                    return Err(invalid(
                        "Saved-file move requires its original file scope and an existing destination file",
                    ));
                }
                false
            }
            ReviewScope::Review { .. } => {
                if !Path::new(&current).is_dir() {
                    return Err(invalid(
                        "Directory move requires an existing destination directory",
                    ));
                }
                true
            }
        };
        if let Some(other) = tx.review_contexts.get_notes_for_path(&current).await?
            && other.id != context.id
            && (other.notes_path.as_deref() == Some(&current)
                || other.current_notes_path.as_deref() == Some(&current))
        {
            return Err(Error::Conflict(
                "Destination already belongs to another saved conversation".into(),
            ));
        }
        for row in tx.comments.get_threads(&context.id).await? {
            let mut thread = Self::thread_from_db_model(row, tx).await?;
            let path = match &thread.anchor {
                Anchor::File { path } | Anchor::Line { path, .. } => Some(path),
                Anchor::General => None,
            };
            let known_path = thread
                .current_path
                .as_ref()
                .or(thread.source_path.as_ref())
                .map(|path| {
                    if Path::new(path).is_absolute() {
                        std::path::PathBuf::from(path)
                    } else {
                        Path::new(known).join(path)
                    }
                })
                .or_else(|| {
                    path.map(|path| {
                        if directory || &previous != known {
                            Path::new(known).join(path)
                        } else {
                            previous.clone().into()
                        }
                    })
                });
            let destination = if directory {
                known_path.and_then(|path| {
                    path.strip_prefix(&previous)
                        .ok()
                        .map(|relative| Path::new(&current).join(relative))
                })
            } else if (known_path.is_some() || &previous == known)
                && known_path
                    .as_ref()
                    .is_none_or(|path| path == Path::new(&previous))
            {
                Some(current.clone().into())
            } else {
                None
            };
            if let Some(destination) = destination {
                thread.current_path = Some(destination.to_string_lossy().into_owned());
                tx.comments
                    .update_thread(&threads::Model::from(&thread))
                    .await?;
                tx.workspace
                    .insert_path(&workspace_paths::Model {
                        context_id: context.id,
                        path: destination.to_string_lossy().into_owned(),
                    })
                    .await?;
            }
        }
        if &previous == known {
            tx.review_contexts
                .set_notes_location(&context.id, &current)
                .await?;
        }
        tx.workspace
            .insert_path(&workspace_paths::Model {
                context_id: context.id,
                path: current,
            })
            .await?;
        Ok(())
    }
}
