//! Generic branch-to-directory slug formatting and atomic file writes.

use std::{
    fs,
    io::{Result, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Converts a Git branch name into a lowercase hyphenated directory slug.
pub fn branch_to_path_slug(branch: &str) -> String {
    let trimmed = branch.trim();
    let stripped = trimmed.strip_prefix("worktree/").unwrap_or(trimmed);
    let mut slug = String::with_capacity(stripped.len());
    let mut last_dash = false;
    for ch in stripped.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || lower == '_' || lower == '.' {
            slug.push(lower);
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "worktree".to_owned()
    } else {
        slug
    }
}

/// Writes `bytes` to a sibling temporary file in the same directory and atomically renames it onto `path`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("state.tmp");
    let tmp_path = path.with_file_name(format!(".{file_name}.{}.{nanos}.tmp", std::process::id()));

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&tmp_path, path)?;
    Ok(())
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_branch_name {
        use super::*;

        mod when_converting_to_a_path_slug {
            use super::*;

            #[test]
            fn should_strip_worktree_prefix_and_hyphenate_slashes() {
                // Arrange
                let prefixed = "worktree/feature/diff-263";
                let plain = "feature/projects_ui";

                // Act
                let slug_prefixed = branch_to_path_slug(prefixed);
                let slug_plain = branch_to_path_slug(plain);

                // Assert
                assert_eq!(slug_prefixed, "feature-diff-263");
                assert_eq!(slug_plain, "feature-projects_ui");
            }
        }
    }
}
