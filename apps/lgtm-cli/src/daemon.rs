use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Instance {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

impl Instance {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

pub fn state_dir(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = match path {
        Some(path) => path,
        None => dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .context("Cannot find a state directory; pass --state-dir")?
            .join("lgtm"),
    };
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

pub(crate) fn prepare(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

pub(crate) fn lock(dir: &Path, name: &str) -> Result<File> {
    let file = private_file(&dir.join(name))?;
    file.try_lock_exclusive()
        .context("Another LGTM instance is starting or running")?;
    Ok(file)
}

fn read(dir: &Path) -> Option<Instance> {
    serde_json::from_slice(&fs::read(dir.join("instance.json")).ok()?).ok()
}

pub(crate) fn write(dir: &Path, instance: &Instance) -> Result<()> {
    use std::io::Write;
    let temp = dir.join("instance.json.tmp");
    let mut file = private_file(&temp)?;
    file.set_len(0)?;
    file.write_all(&serde_json::to_vec(instance)?)?;
    // Windows does not replace an existing destination on rename.
    let path = dir.join("instance.json");
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .no_proxy()
        .build()?)
}

async fn running(client: &reqwest::Client, instance: &Instance) -> bool {
    let Ok(response) = client
        .get(format!("{}/_lgtm/status", instance.url()))
        .bearer_auth(&instance.token)
        .send()
        .await
    else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return false;
    };
    body["pid"].as_u64() == Some(instance.pid.into())
}

pub async fn up(port: u16, open_browser: bool, dir: &Path) -> Result<()> {
    prepare(dir)?;
    let _start_lock = lock(dir, "start.lock")?;
    let client = client()?;
    if let Some(instance) = read(dir) {
        if running(&client, &instance).await {
            announce(&instance, open_browser, dir);
            return Ok(());
        }
    }
    let log = private_file(&dir.join("server.log"))?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--state-dir")
        .arg(dir)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: the post-fork callback only invokes setsid and reads errno.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200); // detached + new process group
    }
    let mut child = command.spawn().context("Start the bundled LGTM server")?;
    for _ in 0..50 {
        if let Some(exit) = child.try_wait()? {
            bail!(
                "LGTM server exited ({exit}); check {}",
                dir.join("server.log").display()
            );
        }
        if let Some(instance) = read(dir) {
            if instance.pid == child.id() && running(&client, &instance).await {
                announce(&instance, open_browser, dir);
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    child.kill().ok();
    child.wait().ok();
    bail!(
        "LGTM did not become ready; check {}",
        dir.join("server.log").display()
    )
}

fn announce(instance: &Instance, open_browser: bool, dir: &Path) {
    println!("LGTM is running at {}", instance.url());
    println!("Logs: {}", dir.join("server.log").display());
    if open_browser {
        if let Err(error) = webbrowser::open(&instance.url()) {
            eprintln!("Could not open a browser ({error}); open the URL above.");
        }
    }
}

pub async fn down(dir: &Path) -> Result<()> {
    let client = client()?;
    let Some(instance) = read(dir) else {
        println!("LGTM is not running");
        return Ok(());
    };
    if !running(&client, &instance).await {
        println!("LGTM is not running");
        return Ok(());
    }
    client
        .post(format!("{}/_lgtm/shutdown", instance.url()))
        .bearer_auth(&instance.token)
        .send()
        .await?
        .error_for_status()?;
    for _ in 0..50 {
        if read(dir).is_none_or(|state| state.pid != instance.pid) {
            println!("LGTM stopped");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!(
        "LGTM is still shutting down; check {}",
        dir.join("server.log").display()
    )
}

pub async fn status(dir: &Path) -> Result<()> {
    let client = client()?;
    if let Some(instance) = read(dir) {
        if running(&client, &instance).await {
            println!(
                "LGTM is running at {} (pid {})",
                instance.url(),
                instance.pid
            );
            return Ok(());
        }
    }
    println!("LGTM is not running");
    Ok(())
}
