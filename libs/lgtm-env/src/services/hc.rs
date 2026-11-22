use std::path::{Path, PathBuf};

/// Environment variable overriding the `herdr-core` (`hc`) sidecar binary path.
pub const LGTM_HC_BIN_ENV: &str = "LGTM_HC_BIN";

/// Resolves the bundled, staged, or user-installed `herdr-core` (`hc`) binary path for terminal runtime management.
#[derive(Debug, Clone, Default)]
pub struct HcBinaryService {
    explicit_override: Option<PathBuf>,
    search_roots: Vec<PathBuf>,
    search_system_paths: bool,
}

impl HcBinaryService {
    /// Creates an [`HcBinaryService`] that resolves `LGTM_HC_BIN`, adjacent bundled sidecar binaries, staged workspace binaries, and system `PATH`.
    pub fn new() -> Self {
        let mut search_roots = Vec::new();
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            search_roots.push(dir.to_path_buf());
        }
        if let Ok(cwd) = std::env::current_dir() {
            search_roots.push(cwd);
        }
        Self {
            explicit_override: std::env::var_os(LGTM_HC_BIN_ENV)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            search_roots,
            search_system_paths: true,
        }
    }

    /// Creates an isolated [`HcBinaryService`] scoped to `explicit_override` and `search_roots` without consulting process environment variables or system `PATH`.
    pub fn with_isolated_roots(
        explicit_override: Option<PathBuf>,
        search_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            explicit_override,
            search_roots,
            search_system_paths: false,
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns the Tauri sidecar target triple for the current host platform.
    pub fn current_target_triple() -> &'static str {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            "aarch64-apple-darwin"
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            "x86_64-apple-darwin"
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            "aarch64-unknown-linux-gnu"
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            "x86_64-unknown-linux-gnu"
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            "x86_64-pc-windows-msvc"
        }
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
        )))]
        {
            "unknown"
        }
    }

    /// Returns the staged sidecar filename (`hc-<triple>` or `hc-<triple>.exe`) for the current host platform.
    pub fn staged_binary_filename() -> String {
        let triple = Self::current_target_triple();
        if cfg!(windows) {
            format!("hc-{triple}.exe")
        } else {
            format!("hc-{triple}")
        }
    }

    /// Locates the first usable `herdr-core` (`hc`) binary in priority order: explicit override, bundled sidecar next to the executable, staged desktop binary, or user `PATH`.
    pub fn get_hc_binary_path(&self) -> Option<PathBuf> {
        if let Some(override_path) = &self.explicit_override {
            return Self::is_usable_binary(override_path).then(|| override_path.clone());
        }

        let staged_name = Self::staged_binary_filename();
        let bundled_name = Self::bundled_binary_name();

        for root in &self.search_roots {
            for direct in [root.join(bundled_name), root.join(&staged_name)] {
                if Self::is_usable_binary(&direct) {
                    return Some(direct);
                }
            }

            if let Some(staged) = Self::find_staged_in_ancestors(root, &staged_name) {
                return Some(staged);
            }
        }

        if !self.search_system_paths {
            return None;
        }

        Self::find_on_system_paths()
    }

    // Private -----------------------------------------------------------------

    fn bundled_binary_name() -> &'static str {
        if cfg!(windows) { "hc.exe" } else { "hc" }
    }

    fn find_staged_in_ancestors(start: &Path, staged_name: &str) -> Option<PathBuf> {
        for ancestor in start.ancestors() {
            let desktop_staged = ancestor
                .join("apps")
                .join("lgtm-desktop")
                .join("binaries")
                .join(staged_name);
            if Self::is_usable_binary(&desktop_staged) {
                return Some(desktop_staged);
            }

            let direct_binaries = ancestor.join("binaries").join(staged_name);
            if Self::is_usable_binary(&direct_binaries) {
                return Some(direct_binaries);
            }
        }
        None
    }

    fn find_on_system_paths() -> Option<PathBuf> {
        let names: &[&str] = if cfg!(windows) {
            &["herdr-core.exe", "hc.exe"]
        } else {
            &["herdr-core", "hc"]
        };

        if let Some(path_os) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_os) {
                for name in names {
                    let candidate = dir.join(name);
                    if Self::is_usable_binary(&candidate) {
                        return Some(candidate);
                    }
                }
            }
        }

        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for name in names {
                let candidate = home.join(".local").join("bin").join(name);
                if Self::is_usable_binary(&candidate) {
                    return Some(candidate);
                }
            }
        }

        None
    }

    fn is_usable_binary(path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    }
}
