//! Repository slug hashing, branch-to-directory slug formatting, and atomic file writes.

use std::{
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use crate::{error::Result, git::resolve_git_paths};

const ADJECTIVES: [&str; 8] = [
    "amber", "brisk", "calm", "crisp", "lunar", "nimble", "quiet", "swift",
];
const NOUNS: [&str; 8] = [
    "beacon", "canyon", "comet", "harbor", "meadow", "orbit", "ridge", "spruce",
];

/// Computes a deterministic, filesystem-safe `<repo-name>-<12-hex-chars>` slug for a repository's shared `common_git_dir`.
pub fn compute_repo_slug(git_dir_or_worktree: &Path) -> Result<String> {
    let resolved = resolve_git_paths(git_dir_or_worktree)?;
    let repo_name = resolved
        .main_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    let mut hasher = Sha256::new();
    hasher.update(resolved.common_git_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let full_hex = format!("{digest:x}");
    let short_hash = &full_hex[..12];

    Ok(format!("{repo_name}-{short_hash}"))
}

/// Converts a Git branch name into a lowercase hyphenated directory slug matching Herdr (`~/.lgtm/worktrees/<project>/<branch-slug>`).
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

/// Generates a default `worktree/<adjective>-<noun>-<4-hex>` branch name from `seed`.
pub fn generated_branch_slug(seed: u64) -> String {
    let adj = ADJECTIVES[(seed as usize) % ADJECTIVES.len()];
    let noun = NOUNS[((seed >> 4) as usize) % NOUNS.len()];
    let suffix = (seed & 0xffff) as u16;
    format!("worktree/{adj}-{noun}-{suffix:04x}")
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
        .unwrap_or("state.json");
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
