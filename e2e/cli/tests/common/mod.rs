//! Real-process fixtures, isolated from the user's data and credentials.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------
pub struct Fixture {
    pub home: TempDir,
    pub files: TempDir,
    pub child: Child,
    pub endpoint_url: String,
}

impl Fixture {
    pub fn cli(&self, args: &[&str]) -> std::process::Output {
        Command::new(binary("lgtm"))
            .args(args)
            .env("LGTM_HOME", self.home.path())
            .env("HOME", self.home.path())
            .env("LGTM_RPC_URL", &self.endpoint_url)
            .env("LGTM_NAME", "E2E reviewer")
            .env("LGTM_EMAIL", "e2e@example.invalid")
            .current_dir(self.files.path())
            .output()
            .unwrap()
    }

    pub fn json(&self, args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        let output = self.cli(&args);
        assert!(
            output.status.success(),
            "args={args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    pub fn restart(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(self.home.path().join("server.port"));
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.path().join("backend.log"))
            .unwrap();
        self.child = Command::new(binary("lgtm"))
            .args(["up", "--foreground", "--no-open", "--port", "0"])
            .env("LGTM_HOME", self.home.path())
            .env("HOME", self.home.path())
            .env_remove("LGTM_DATABASE_URL")
            .env_remove("LGTM_RPC_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Ok(port) = std::fs::read_to_string(self.home.path().join("server.port")) {
                self.endpoint_url = format!("http://127.0.0.1:{}/rpc", port.trim());
                break;
            }
            assert!(self.child.try_wait().unwrap().is_none());
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    pub fn start() -> Self {
        let artifacts = binary("lgtm")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("e2e/cli");
        std::fs::create_dir_all(&artifacts).unwrap();
        let home = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(artifacts)
            .unwrap();
        let files = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(home.path().join("backend.log")).unwrap();
        let child = Command::new(binary("lgtm"))
            .args(["up", "--foreground", "--no-open", "--port", "0"])
            .env("LGTM_HOME", home.path())
            .env("HOME", home.path())
            .env_remove("LGTM_DATABASE_URL")
            .env_remove("LGTM_RPC_URL")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let mut fixture = Self {
            home,
            files,
            child,
            endpoint_url: String::new(),
        };
        let start = Instant::now();
        loop {
            if let Ok(port) = std::fs::read_to_string(fixture.home.path().join("server.port")) {
                fixture.endpoint_url = format!("http://127.0.0.1:{}/rpc", port.trim());
                break;
            }
            assert!(
                fixture.child.try_wait().unwrap().is_none(),
                "backend exited during startup"
            );
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "backend startup timed out"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
        fixture
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if std::thread::panicking() {
            self.home.disable_cleanup(true);
            self.files.disable_cleanup(true);
            eprintln!(
                "Failed CLI fixture retained at {} (workspace {})",
                self.home.path().display(),
                self.files.path().display()
            );
            if let Ok(log) = std::fs::read_to_string(self.home.path().join("backend.log")) {
                eprintln!("{log}");
            }
        }
    }
}

pub fn binary(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    target.join("debug").join(name)
}
