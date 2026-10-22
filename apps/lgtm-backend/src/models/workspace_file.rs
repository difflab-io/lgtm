/// A UTF-8 text file at the selected content revision.
#[derive(Clone, Debug)]
pub struct WorkspaceFile {
    pub path: String,
    /// Original repository path for renamed remote files; empty for local snapshots.
    pub previous_path: String,
    pub content: String,
    pub content_hash: String,
    pub status: String,
}
