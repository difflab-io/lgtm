//! Filesystem identity observations for saved-file and directory scopes.
use filepath::FilePath;
use same_file::Handle;
use std::{collections::HashMap, fs::File, path::Path, sync::Mutex};
use uuid::Uuid;

/// Retains an OS handle while the backend serves a saved location. A candidate move
/// requires the original path to be absent and the destination to have the same identity.
#[derive(Default)]
pub(super) struct SourceLocations {
    files: Mutex<HashMap<(Uuid, String), File>>,
}

impl SourceLocations {
    // Public ------------------------------------------------------------------

    pub fn observe(&self, context: Uuid, path: &str) {
        if let Ok(file) = File::open(path) {
            self.files
                .lock()
                .unwrap()
                .insert((context, path.into()), file);
        }
    }

    pub fn relocate(&self, context: Uuid, previous: &str, current: &str) {
        let mut files = self.files.lock().unwrap();
        let affected: Vec<_> = files
            .keys()
            .filter(|(id, path)| *id == context && Path::new(path).starts_with(previous))
            .cloned()
            .collect();
        for key in affected {
            let relative = Path::new(&key.1).strip_prefix(previous).unwrap();
            let destination = if relative.as_os_str().is_empty() {
                current.to_owned()
            } else {
                Path::new(current)
                    .join(relative)
                    .to_string_lossy()
                    .into_owned()
            };
            if let Some(file) = files.remove(&key) {
                files.insert((context, destination), file);
            }
        }
        drop(files);
        self.observe(context, current);
    }

    pub fn get_moves(&self) -> Vec<(Uuid, String, String, bool)> {
        let files = self.files.lock().unwrap();
        let mut moves = Vec::new();
        for ((id, previous), file) in files.iter() {
            if let Some((current, directory)) = Self::get_destination(previous, file) {
                moves.push((*id, previous.clone(), current, directory));
            }
        }
        let parents: Vec<_> = moves
            .iter()
            .filter(|(_, _, _, directory)| *directory)
            .cloned()
            .collect();
        moves.retain(|(id, previous, current, _)| {
            !parents.iter().any(|(parent_id, from, to, _)| {
                id == parent_id
                    && previous != from
                    && Path::new(previous)
                        .strip_prefix(from)
                        .is_ok_and(|relative| Path::new(to).join(relative) == Path::new(current))
            })
        });
        moves
    }

    // Private -----------------------------------------------------------------

    fn get_destination(previous: &str, file: &File) -> Option<(String, bool)> {
        if !matches!(std::fs::symlink_metadata(previous), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            return None;
        }
        let current = file.path().ok()?;
        let original = Handle::from_file(file.try_clone().ok()?).ok()?;
        let destination = Handle::from_path(&current).ok()?;
        if original != destination || current == Path::new(previous) {
            return None;
        }
        Some((current.to_str()?.into(), file.metadata().ok()?.is_dir()))
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_coalesce_parent_moves_and_keep_child_identity_without_following_copies() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let parent = root.join("before");
        let destination = root.join("after");
        std::fs::create_dir(&parent).unwrap();
        let child = parent.join("note.md");
        std::fs::write(&child, b"note").unwrap();
        let id = Uuid::new_v4();
        let locations = SourceLocations::default();
        locations.observe(id, parent.to_str().unwrap());
        locations.observe(id, child.to_str().unwrap());
        std::fs::rename(&parent, &destination).unwrap();
        assert_eq!(
            locations.get_moves(),
            vec![(
                id,
                parent.to_str().unwrap().into(),
                destination.to_str().unwrap().into(),
                true
            )]
        );
        locations.relocate(id, parent.to_str().unwrap(), destination.to_str().unwrap());
        assert!(locations.get_moves().is_empty());
        let next = root.join("again");
        std::fs::rename(&destination, &next).unwrap();
        assert_eq!(
            locations.get_moves(),
            vec![(
                id,
                destination.to_str().unwrap().into(),
                next.to_str().unwrap().into(),
                true
            )]
        );
        locations.relocate(id, destination.to_str().unwrap(), next.to_str().unwrap());
        assert!(locations.get_moves().is_empty());
        let destination = next;
        let moved_child = destination.join("note.md");
        let renamed_child = root.join("renamed.md");
        std::fs::rename(&moved_child, &renamed_child).unwrap();
        assert_eq!(
            locations.get_moves(),
            vec![(
                id,
                moved_child.to_str().unwrap().into(),
                renamed_child.to_str().unwrap().into(),
                false
            )]
        );
        locations.relocate(
            id,
            moved_child.to_str().unwrap(),
            renamed_child.to_str().unwrap(),
        );
        std::fs::copy(&renamed_child, root.join("copy.md")).unwrap();
        std::fs::remove_file(&renamed_child).unwrap();
        assert!(locations.get_moves().is_empty());
    }
}
