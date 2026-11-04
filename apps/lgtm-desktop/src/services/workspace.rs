use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use lgtm_lsp::LspService;

use crate::{
    error::{Result, error},
    models::{FileContent, Workspace, WorkspaceListing, WorkspaceNode},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Browses and mutates entries within the active local workspace and coordinates LSP document lifecycle.
pub struct WorkspaceService {
    /// The default workspace directory under the user's home directory.
    default_workspace: Workspace,
    /// The active workspace location shared by file operations.
    workspace: Workspace,
    /// Whether the active workspace was explicitly chosen on disk.
    custom_root: bool,
    /// Optional injected LSP service bounded to the active workspace lifecycle.
    lsp_service: Option<Arc<LspService>>,
}

impl WorkspaceService {
    /// Selects the default workspace under the supplied home directory without an attached LSP service.
    pub fn from_home(home: PathBuf) -> Self {
        let default_workspace = Workspace::from_home(home);
        Self {
            workspace: default_workspace.clone(),
            default_workspace,
            custom_root: false,
            lsp_service: None,
        }
    }

    /// Selects the default workspace under `home` and binds `lsp_service` to workspace lifecycle operations.
    pub fn with_lsp(home: PathBuf, lsp_service: Arc<LspService>) -> Self {
        let default_workspace = Workspace::from_home(home);
        Self {
            workspace: default_workspace.clone(),
            default_workspace,
            custom_root: false,
            lsp_service: Some(lsp_service),
        }
    }

    // Public ------------------------------------------------------------------

    /// Opens the active workspace or switches to a chosen directory on disk, warming up workspace LSP servers.
    pub fn open(&mut self, path: Option<&str>) -> Result<WorkspaceListing> {
        let previous_root = self.get_root().ok();
        if let Some(target) = path.map(str::trim).filter(|s| !s.is_empty()) {
            let candidate = PathBuf::from(target);
            if candidate == self.default_workspace.root {
                self.workspace = self.default_workspace.clone();
                self.custom_root = false;
            } else {
                if !candidate.is_dir() {
                    return Err(error("invalidWorkspace", "Choose an existing folder."));
                }
                self.workspace = Workspace::from_path(candidate);
                self.custom_root = true;
            }
        }
        let root = self.get_root()?;
        if let Some(lsp_service) = &self.lsp_service {
            if let Some(prev) = previous_root
                && prev != root
            {
                lsp_service.shutdown_workspace(&prev);
            }
            let _ = lsp_service.prepare_workspace(&root);
        }
        Ok(WorkspaceListing {
            root: root.to_string_lossy().into_owned(),
            entries: self.get_entries("")?,
        })
    }

    /// Lists visible entries inside a workspace-relative directory.
    pub fn get_entries(&self, relative: &str) -> Result<Vec<WorkspaceNode>> {
        let directory = self.get_entry_path(relative)?;
        let root = self.get_root()?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| error("invalidName", "A workspace filename is not valid Unicode."))?;
            if name == ".lgtm" || name.starts_with(".lgtm-save-") {
                continue;
            }
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let canonical = entry.path().canonicalize();
            let kind = match canonical {
                Ok(p) if p.starts_with(&root) && p.is_dir() => "directory",
                Ok(p) if p.starts_with(&root) && p.is_file() => "file",
                _ => "unavailable",
            };
            entries.push(WorkspaceNode { path, name, kind });
        }
        entries.sort_by(|a, b| {
            (a.kind != "directory")
                .cmp(&(b.kind != "directory"))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(entries)
    }

    /// Reads UTF-8 text and revision hash for a workspace-relative file and synchronizes it with its LSP server.
    pub fn get_file_content(&self, relative: &str) -> Result<FileContent> {
        if relative.is_empty() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        let root = self.get_root()?;
        let path = self.get_entry_path(relative)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        let mut file = fs::File::open(&path)?;
        if !file.metadata()?.is_file() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error(
                "tooLarge",
                "This file is larger than the 16 MB viewing limit.",
            ));
        }
        let content = FileContent::from_bytes(bytes)?;
        if let Some(lsp_service) = &self.lsp_service {
            let (language_id, lsp_status) =
                lsp_service.open_or_sync_file(&root, &path, &content.content);
            return Ok(content.with_lsp(language_id, lsp_status));
        }
        Ok(content)
    }

    /// Writes UTF-8 text to an existing workspace-relative file, notifies LSP `didSave`, and returns its new revision.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<FileContent> {
        if relative.is_empty() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        let root = self.get_root()?;
        let original = root.join(relative);
        if fs::symlink_metadata(&original)?.file_type().is_symlink() {
            return Err(error("unsupported", "Linked files cannot be modified."));
        }
        let path = self.get_entry_path(relative)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        if content.len() as u64 > MAX_BYTES {
            return Err(error(
                "tooLarge",
                "This file is larger than the 16 MB viewing limit.",
            ));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        let written = FileContent::from_bytes(content.as_bytes().to_vec())?;
        if let Some(lsp_service) = &self.lsp_service {
            let _ = lsp_service.did_save(&root, &path, content);
        }
        Ok(written)
    }

    /// Creates a new file inside a workspace-relative directory without overwriting existing files.
    pub fn create_file(&self, name: &str, parent: &str) -> Result<WorkspaceNode> {
        let name = WorkspaceNode::validate_name(name)?;
        let name = if Path::new(name).extension().is_none() {
            format!("{name}.md")
        } else {
            name.to_owned()
        };
        let title = Path::new(&name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let target = self.get_entry_path(parent)?.join(&name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(&target).is_ok()
                {
                    error(
                        "nameUnavailable",
                        "A file or folder with this name already exists.",
                    )
                } else {
                    e.into()
                }
            })?;
        file.write_all(format!("# {title}\n\n").as_bytes())?;
        file.sync_all()?;
        Ok(WorkspaceNode {
            path: if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            },
            name,
            kind: "file",
        })
    }

    /// Creates a new directory inside a workspace-relative directory.
    pub fn create_folder(&self, name: &str, parent: &str) -> Result<WorkspaceNode> {
        let name = WorkspaceNode::validate_name(name)?;
        fs::create_dir(self.get_entry_path(parent)?.join(name)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                error(
                    "nameUnavailable",
                    "A file or folder with this name already exists.",
                )
            } else {
                e.into()
            }
        })?;
        Ok(WorkspaceNode {
            path: if parent.is_empty() {
                name.to_owned()
            } else {
                format!("{parent}/{name}")
            },
            name: name.to_owned(),
            kind: "directory",
        })
    }

    /// Renames a file or folder exclusively without overwriting an existing entry and closes its old LSP document.
    pub fn rename(&self, relative: &str, name: &str) -> Result<WorkspaceNode> {
        let name = WorkspaceNode::validate_name(name)?;
        let source = self.get_entry_path(relative)?;
        let root = self.get_root()?;
        let original = root.join(relative);
        if fs::symlink_metadata(&original)?.file_type().is_symlink() {
            return Err(error("unsupported", "Linked entries cannot be renamed."));
        }
        let kind = if source.is_dir() { "directory" } else { "file" };
        let target = source.parent().unwrap().join(name);
        if source != target {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                &source,
                rustix::fs::CWD,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(|e| {
                let io_err = std::io::Error::from(e);
                if io_err.kind() == std::io::ErrorKind::AlreadyExists {
                    error(
                        "nameUnavailable",
                        "A file or folder with this name already exists.",
                    )
                } else {
                    error("io", io_err.to_string())
                }
            })?;
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt;
                let source_wide: Vec<u16> =
                    source.as_os_str().encode_wide().chain(Some(0)).collect();
                let target_wide: Vec<u16> =
                    target.as_os_str().encode_wide().chain(Some(0)).collect();
                // SAFETY: Both pointers refer to live, NUL-terminated UTF-16 path buffers.
                let moved = unsafe {
                    windows_sys::Win32::Storage::FileSystem::MoveFileW(
                        source_wide.as_ptr(),
                        target_wide.as_ptr(),
                    )
                };
                if moved == 0 {
                    let io_err = std::io::Error::last_os_error();
                    if io_err.kind() == std::io::ErrorKind::AlreadyExists {
                        return Err(error(
                            "nameUnavailable",
                            "A file or folder with this name already exists.",
                        ));
                    }
                    return Err(io_err.into());
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
            {
                return Err(error(
                    "unsupported",
                    "Renaming is not supported on this platform yet.",
                ));
            }
        }
        if let Some(lsp_service) = &self.lsp_service {
            let _ = lsp_service.did_close(&root, &source);
        }
        let path = target
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        Ok(WorkspaceNode {
            path,
            name: name.to_owned(),
            kind,
        })
    }

    /// Moves a workspace entry to the system Trash (or permanently removes it in E2E harness runs) and closes its LSP document.
    pub fn delete(&self, relative: &str) -> Result<()> {
        if relative.is_empty() {
            return Err(error(
                "outsideWorkspace",
                "The workspace itself cannot be deleted.",
            ));
        }
        let root = self.get_root()?;
        let path = self.get_entry_path(relative)?;
        if fs::symlink_metadata(root.join(relative))?
            .file_type()
            .is_symlink()
        {
            return Err(error("unsupported", "Linked entries cannot be deleted."));
        }
        #[cfg(any(test, feature = "e2e-bridge"))]
        if std::env::var_os("LGTM_E2E_PERMANENT_DELETE").is_some() {
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
            if let Some(lsp_service) = &self.lsp_service {
                let _ = lsp_service.did_close(&root, &path);
            }
            return Ok(());
        }
        trash::delete(&path).map_err(|e| error("io", e.to_string()))?;
        if let Some(lsp_service) = &self.lsp_service {
            let _ = lsp_service.did_close(&root, &path);
        }
        Ok(())
    }

    /// Returns the canonical root directory of the active workspace.
    pub fn get_root(&self) -> Result<PathBuf> {
        if !self.custom_root {
            fs::create_dir_all(&self.workspace.root)?;
        } else if !self.workspace.root.is_dir() {
            return Err(error("invalidWorkspace", "Choose an existing folder."));
        }
        Ok(self.workspace.root.canonicalize()?)
    }

    /// Resolves a workspace-relative path to a canonical path inside the active workspace.
    pub fn get_entry_path(&self, relative: &str) -> Result<PathBuf> {
        let path = WorkspaceNode::validate_relative_path(relative)?;
        let root = self.get_root()?;
        let resolved = root.join(path).canonicalize()?;
        if !resolved.starts_with(&root) {
            return Err(error(
                "outsideWorkspace",
                "Links outside the workspace cannot be opened.",
            ));
        }
        Ok(resolved)
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // fixtures ----------------------------------------------------------------

    struct WorkspaceFixture {
        home: TempDir,
        service: WorkspaceService,
    }

    impl WorkspaceFixture {
        fn new() -> Self {
            let home = TempDir::new().unwrap();
            let service = WorkspaceService::from_home(home.path().to_path_buf());
            Self { home, service }
        }

        fn default_root(&self) -> PathBuf {
            self.home.path().join(".lgtm").join("workspace")
        }
    }

    // tests -------------------------------------------------------------------

    mod given_a_workspace_service {
        use super::*;

        mod when_opening_and_listing_workspace {
            use super::*;

            #[test]
            fn should_create_default_workspace_on_initial_launch() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();

                // Act
                let opened = fixture.service.open(None).unwrap();

                // Assert
                assert!(fixture.default_root().is_dir());
                assert_eq!(
                    PathBuf::from(&opened.root),
                    fixture.default_root().canonicalize().unwrap()
                );
                assert!(opened.entries.is_empty());
            }

            #[test]
            fn should_switch_active_root_when_opening_any_existing_directory_on_disk() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                let other_dir = TempDir::new().unwrap();
                fs::write(other_dir.path().join("lib.rs"), "pub fn demo() {}\n").unwrap();
                fs::create_dir(other_dir.path().join("src")).unwrap();

                // Act
                let opened = fixture
                    .service
                    .open(Some(other_dir.path().to_str().unwrap()))
                    .unwrap();

                // Assert
                assert_eq!(
                    PathBuf::from(&opened.root),
                    other_dir.path().canonicalize().unwrap()
                );
                assert_eq!(opened.entries.len(), 2);
                assert_eq!(opened.entries[0].name, "src");
                assert_eq!(opened.entries[0].kind, "directory");
                assert_eq!(opened.entries[1].name, "lib.rs");
                assert_eq!(opened.entries[1].kind, "file");
            }

            #[test]
            fn should_reject_nonexistent_workspace_directory() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                let missing = fixture.home.path().join("does-not-exist");

                // Act
                let result = fixture.service.open(Some(missing.to_str().unwrap()));

                // Assert
                assert_eq!(result.unwrap_err().code, "invalidWorkspace");
            }

            #[test]
            fn should_reject_path_traversal_outside_workspace() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();

                // Act
                let list_escape = fixture.service.get_entries("../outside");
                let read_escape = fixture.service.get_file_content("../outside.md");

                // Assert
                assert_eq!(list_escape.unwrap_err().code, "outsideWorkspace");
                assert_eq!(read_escape.unwrap_err().code, "outsideWorkspace");
            }

            #[cfg(unix)]
            #[test]
            fn should_mark_external_symlinks_unavailable_and_reject_reading_them() {
                // Arrange
                use std::os::unix::fs::symlink;
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                let outside = fixture.home.path().join("secret.md");
                fs::write(&outside, "secret").unwrap();
                symlink(&outside, fixture.default_root().join("escape.md")).unwrap();

                // Act
                let entries = fixture.service.get_entries("").unwrap();
                let read_result = fixture.service.get_file_content("escape.md");

                // Assert
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].kind, "unavailable");
                assert_eq!(read_result.unwrap_err().code, "outsideWorkspace");
            }
        }

        mod when_creating_files_and_folders {
            use super::*;

            #[test]
            fn should_create_files_and_directories_exclusively_and_report_collisions() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();

                // Act
                let folder = fixture.service.create_folder("  Research  ", "").unwrap();
                let file = fixture
                    .service
                    .create_file("Untitled.md", "Research")
                    .unwrap();
                let duplicate_folder = fixture.service.create_folder("Research", "");
                let duplicate_file = fixture.service.create_file("Untitled.md", "Research");

                // Assert
                assert_eq!(folder.path, "Research");
                assert_eq!(folder.kind, "directory");
                assert_eq!(file.path, "Research/Untitled.md");
                assert_eq!(file.kind, "file");
                assert_eq!(duplicate_folder.unwrap_err().code, "nameUnavailable");
                assert_eq!(duplicate_file.unwrap_err().code, "nameUnavailable");
            }

            #[test]
            fn should_reject_invalid_entry_names() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                let invalid_names = ["", " ", ".", "..", "a/b", "a\\b", "a:b", ".lgtm-save-1"];

                // Act
                let codes: Vec<&str> = invalid_names
                    .into_iter()
                    .map(|invalid| fixture.service.create_folder(invalid, "").unwrap_err().code)
                    .collect();

                // Assert
                assert!(codes.into_iter().all(|code| code == "invalidName"));
            }
        }

        mod when_renaming_entries {
            use super::*;

            #[test]
            fn should_rename_without_clobbering_existing_targets() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                fixture.service.create_file("One.md", "").unwrap();
                fixture.service.create_file("Two.md", "").unwrap();

                // Act
                let conflict = fixture.service.rename("One.md", "Two.md");
                let renamed = fixture.service.rename("One.md", "Renamed.md").unwrap();

                // Assert
                assert_eq!(conflict.unwrap_err().code, "nameUnavailable");
                assert_eq!(renamed.path, "Renamed.md");
                assert!(fixture.default_root().join("Renamed.md").is_file());
                assert!(fixture.default_root().join("Two.md").is_file());
            }
        }

        mod when_reading_and_deleting_entries {
            use super::*;

            #[test]
            fn should_read_utf8_content_with_sha256_revision_and_reject_invalid_delete_targets() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                fixture.service.create_file("Notes.md", "").unwrap();

                // Act
                let read = fixture.service.get_file_content("Notes.md").unwrap();
                let delete_root = fixture.service.delete("");
                let delete_outside = fixture.service.delete("../outside");

                // Assert
                assert_eq!(read.content, "# Notes\n\n");
                assert_eq!(read.revision.len(), 64);
                assert_eq!(delete_root.unwrap_err().code, "outsideWorkspace");
                assert_eq!(delete_outside.unwrap_err().code, "outsideWorkspace");
            }

            #[test]
            fn should_write_and_read_updated_utf8_content() {
                // Arrange
                let mut fixture = WorkspaceFixture::new();
                fixture.service.open(None).unwrap();
                fixture.service.create_file("Notes.md", "").unwrap();

                // Act
                let written = fixture
                    .service
                    .write_file("Notes.md", "# Notes\n\nUpdated body.\n")
                    .unwrap();
                let reread = fixture.service.get_file_content("Notes.md").unwrap();

                // Assert
                assert_eq!(written.content, "# Notes\n\nUpdated body.\n");
                assert_eq!(reread, written);
            }
        }
    }
}
