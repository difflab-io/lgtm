//! Hard-coded sample data used while the GPUI view is wired to the backend.
//! Replace with reqwest calls to lgtm-grpc once the two-pane layout is stable.

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Untracked,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Untracked => "untracked",
        }
    }
}

pub fn files() -> Vec<ChangedFile> {
    [
        ("Cargo.lock", Status::Modified),
        ("crates/globset/Cargo.toml", Status::Modified),
        ("crates/ignore/Cargo.toml", Status::Modified),
        ("crates/ignore/src/gitignore.rs", Status::Modified),
        ("crates/ignore/src/types.rs", Status::Modified),
        ("tests/util.rs", Status::Modified),
    ]
    .into_iter()
    .map(|(p, s)| ChangedFile { path: p.into(), status: s })
    .collect()
}

pub fn diff_for(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n\
         index 0000000..1111111 100644\n\
         --- a/{path}\n\
         +++ b/{path}\n\
         @@ -1,3 +1,3 @@\n\
         -placeholder diff for {path}\n\
         +placeholder diff for {path} (edited)\n\
          context line\n"
    )
}
