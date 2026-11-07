use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use lgtm_config::LgtmConfigPatch;

use crate::{
    error::{Result, error},
    models::{FileContent, Workspace, WorkspaceListing, WorkspaceNode},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_BIOME_JSON: &str = "{\n  \"$schema\": \"https://biomejs.dev/schemas/latest/schema.json\",\n  \"json\": {\n    \"parser\": {\n      \"allowComments\": false\n    }\n  }\n}\n";

/// Browses and mutates entries within the active local workspace directory.
pub struct WorkspaceService {
    /// The global `.lgtm` home directory under the user's home directory.
    lgtm_home: PathBuf,
    /// The default workspace directory under the user's home directory.
    default_workspace: Workspace,
    /// The active workspace location shared by file operations.
    workspace: Workspace,
    /// Whether the active workspace was explicitly chosen on disk.
    custom_root: bool,
}

impl WorkspaceService {
    /// Selects the default workspace under the supplied home directory.
    pub fn from_home(home: PathBuf) -> Self {
        Self::new(home, None)
    }

    /// Creates a workspace service rooted at `workspace_path` when provided and valid on disk (otherwise `<home>/.lgtm/workspace`).
    pub fn new(home: PathBuf, workspace_path: Option<PathBuf>) -> Self {
        let lgtm_home = home.join(".lgtm");
        let default_workspace = Workspace::from_home(home);
        if let Some(path) = workspace_path.filter(|path| path.is_dir())
            && path != default_workspace.root
        {
            return Self {
                lgtm_home,
                workspace: Workspace::from_path(path),
                default_workspace,
                custom_root: true,
            };
        }
        Self {
            lgtm_home,
            workspace: default_workspace.clone(),
            default_workspace,
            custom_root: false,
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns whether the active workspace was explicitly chosen rather than the default workspace.
    pub fn is_custom_root(&self) -> bool {
        self.custom_root
    }

    /// Opens the active workspace or switches to a chosen directory on disk.
    pub fn open(&mut self, path: Option<&str>) -> Result<WorkspaceListing> {
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
        Ok(WorkspaceListing {
            root: root.to_string_lossy().into_owned(),
            entries: self.get_entries("")?,
        })
    }

    /// Ensures the global `~/.lgtm/config.json` file exists and reads its UTF-8 content.
    pub fn open_global_config(&self) -> Result<FileContent> {
        self.ensure_global_config_file()?;
        self.get_file_content("~/.lgtm/config.json")
    }

    /// Ensures the global `~/.lgtm/config.json` exists and reads `.lgtm/config.json`, or returns an empty JSON temp buffer if `.lgtm/config.json` is absent.
    pub fn open_local_config(&self) -> Result<FileContent> {
        self.ensure_global_config_file()?;
        let root = self.get_root()?;
        let config_path = root.join(".lgtm").join("config.json");
        if config_path.is_file() {
            return self.get_file_content(".lgtm/config.json");
        }
        FileContent::from_bytes(b"{\n}\n".to_vec())
    }

    /// Returns whether `path` points to the global `~/.lgtm/config.json` or active workspace `.lgtm/config.json`.
    pub fn is_config_path(&self, path: &Path) -> bool {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let global_path = self.lgtm_home.join("config.json");
        if global_path.canonicalize().unwrap_or(global_path) == canon {
            return true;
        }
        self.get_root().ok().is_some_and(|root| {
            let local = root.join(".lgtm").join("config.json");
            local.canonicalize().unwrap_or(local) == canon
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

    /// Reads UTF-8 text and revision hash for a workspace-relative file.
    pub fn get_file_content(&self, relative: &str) -> Result<FileContent> {
        if relative.is_empty() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
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
        FileContent::from_bytes(bytes)
    }

    /// Writes UTF-8 text to an existing workspace-relative or config file and returns its new revision.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<FileContent> {
        if relative.is_empty() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        if content.len() as u64 > MAX_BYTES {
            return Err(error(
                "tooLarge",
                "This file is larger than the 16 MB viewing limit.",
            ));
        }
        if relative == ".lgtm/config.json" {
            let root = self.get_root()?;
            let lgtm_dir = root.join(".lgtm");
            fs::create_dir_all(&lgtm_dir)?;
            let path = lgtm_dir.join("config.json");
            if let Ok(meta) = fs::symlink_metadata(&path)
                && meta.file_type().is_symlink()
            {
                return Err(error("unsupported", "Linked files cannot be modified."));
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            return FileContent::from_bytes(content.as_bytes().to_vec());
        }
        if !self.is_global_config_reference(relative) {
            let root = self.get_root()?;
            let original = root.join(relative);
            if fs::symlink_metadata(&original)?.file_type().is_symlink() {
                return Err(error("unsupported", "Linked files cannot be modified."));
            }
        }
        let path = self.get_entry_path(relative)?;
        if !fs::metadata(&path)?.is_file() {
            return Err(error("unsupported", "Choose a file inside the workspace."));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        FileContent::from_bytes(content.as_bytes().to_vec())
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

    /// Renames a file or folder exclusively without overwriting an existing entry.
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

    /// Moves a workspace entry to the system Trash (or permanently removes it in E2E harness runs).
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
            return Ok(());
        }
        trash::delete(&path).map_err(|e| error("io", e.to_string()))?;
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

    /// Resolves a workspace-relative path (or the global `~/.lgtm/config.json` path) to a canonical path on disk.
    pub fn get_entry_path(&self, relative: &str) -> Result<PathBuf> {
        if self.is_global_config_reference(relative) {
            let global_path = self.lgtm_home.join("config.json");
            return Ok(global_path.canonicalize().unwrap_or(global_path));
        }
        if relative == ".lgtm/config.json" {
            let root = self.get_root()?;
            let local_path = root.join(".lgtm").join("config.json");
            return Ok(local_path.canonicalize().unwrap_or(local_path));
        }
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

    // Private -----------------------------------------------------------------

    fn ensure_global_config_file(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.lgtm_home)?;
        let biome_path = self.lgtm_home.join("biome.json");
        if !biome_path.exists() {
            fs::write(&biome_path, DEFAULT_BIOME_JSON)?;
        }
        let config_path = self.lgtm_home.join("config.json");
        if !config_path.is_file() {
            fs::write(&config_path, LgtmConfigPatch::default_document())?;
        }
        Ok(config_path)
    }

    fn is_global_config_reference(&self, reference: &str) -> bool {
        if reference == "~/.lgtm/config.json" {
            return true;
        }
        let Ok(canon_ref) = Path::new(reference).canonicalize() else {
            return false;
        };
        self.lgtm_home
            .join("config.json")
            .canonicalize()
            .is_ok_and(|global| global == canon_ref)
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
            fn should_open_configured_workspace_path_when_provided() {
                // Arrange
                let home = TempDir::new().unwrap();
                let workspace_dir = TempDir::new().unwrap();
                fs::write(workspace_dir.path().join("main.rs"), "fn main() {}\n").unwrap();
                let mut service = WorkspaceService::new(
                    home.path().to_path_buf(),
                    Some(workspace_dir.path().to_path_buf()),
                );

                // Act
                let opened = service.open(None).unwrap();

                // Assert
                assert_eq!(
                    PathBuf::from(&opened.root),
                    workspace_dir.path().canonicalize().unwrap()
                );
                assert_eq!(opened.entries.len(), 1);
                assert_eq!(opened.entries[0].name, "main.rs");
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
