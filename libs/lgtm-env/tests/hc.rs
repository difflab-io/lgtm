//! Integration tests for [`HcBinaryService`] sidecar and staged binary resolution.
//!
//! Fixture layout:
//! ```text
//! <tempdir>/
//! ├── apps/lgtm-desktop/binaries/hc-<target-triple>[.exe]
//! ├── override/custom-hc
//! └── nested/subdir/
//! ```

use std::fs;
use std::path::Path;

use lgtm_env::HcBinaryService;
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

fn create_executable(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, "#!/bin/sh\necho hc-test\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

// tests -----------------------------------------------------------------------

mod given_no_hc_binary_exists {
    use super::*;

    mod when_resolving_from_isolated_roots {
        use super::*;

        #[test]
        fn should_return_none() {
            // Arrange
            let dir = TempDir::new().unwrap();
            let service =
                HcBinaryService::with_isolated_roots(None, vec![dir.path().to_path_buf()]);

            // Act
            let resolved = service.get_hc_binary_path();

            // Assert
            assert_eq!(resolved, None);
        }
    }
}

mod given_a_staged_desktop_sidecar_binary {
    use super::*;

    mod when_resolving_from_a_nested_workspace_directory {
        use super::*;

        #[test]
        fn should_locate_the_staged_sidecar_binary() {
            // Arrange
            let dir = TempDir::new().unwrap();
            let staged = dir
                .path()
                .join("apps/lgtm-desktop/binaries")
                .join(HcBinaryService::staged_binary_filename());
            create_executable(&staged);
            let nested = dir.path().join("libs/lgtm-terminal");
            fs::create_dir_all(&nested).unwrap();
            let service = HcBinaryService::with_isolated_roots(None, vec![nested]);

            // Act
            let resolved = service.get_hc_binary_path();

            // Assert
            assert_eq!(resolved, Some(staged));
        }
    }

    mod when_an_explicit_override_is_configured {
        use super::*;

        #[test]
        fn should_prefer_the_explicit_override_path() {
            // Arrange
            let dir = TempDir::new().unwrap();
            let staged = dir
                .path()
                .join("apps/lgtm-desktop/binaries")
                .join(HcBinaryService::staged_binary_filename());
            create_executable(&staged);
            let custom = dir.path().join("override/custom-hc");
            create_executable(&custom);
            let service = HcBinaryService::with_isolated_roots(
                Some(custom.clone()),
                vec![dir.path().to_path_buf()],
            );

            // Act
            let resolved = service.get_hc_binary_path();

            // Assert
            assert_eq!(resolved, Some(custom));
        }
    }
}
