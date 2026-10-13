use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
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

#[cfg(unix)]
pub fn detach(dir: &Path) -> Result<()> {
    prepare(dir)?;
    let log = private_file(&dir.join("server.log"))?;
    daemonize::Daemonize::new()
        .working_directory(dir)
        .pid_file(dir.join("server.pid"))
        .umask(0o077)
        .stdout(log.try_clone()?)
        .stderr(log)
        .start()
        .context("Daemonize the LGTM server")
}

#[cfg(not(unix))]
pub fn detach(_dir: &Path) -> Result<()> {
    bail!("Background mode requires macOS or Linux; use lgtm up --foreground")
}

async fn ensure_running(port: u16, dir: &Path) -> Result<Instance> {
    prepare(dir)?;
    let _start_lock = lock(dir, "start.lock")?;
    let client = client()?;
    if let Some(instance) = read(dir) {
        if running(&client, &instance).await {
            return Ok(instance);
        }
    }
    let exit = Command::new(std::env::current_exe()?)
        .arg("--state-dir")
        .arg(dir)
        .arg("serve")
        .arg("--daemonize")
        .arg("--port")
        .arg(port.to_string())
        .status()
        .context("Start the bundled LGTM server")?;
    if !exit.success() {
        bail!("Could not daemonize the LGTM server ({exit})");
    }
    for _ in 0..50 {
        if let Some(instance) = read(dir) {
            if running(&client, &instance).await {
                return Ok(instance);
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!(
        "LGTM did not become ready; check {}",
        dir.join("server.log").display()
    )
}

pub async fn up(port: u16, open_browser: bool, dir: &Path) -> Result<()> {
    let instance = ensure_running(port, dir).await?;
    announce(&instance, dir);
    if open_browser {
        open(&instance.url());
    }
    Ok(())
}

pub async fn review(port: u16, open_browser: bool, dir: &Path) -> Result<()> {
    let path = std::env::current_dir()?;
    let path = path
        .to_str()
        .context("The current directory is not valid UTF-8")?;
    let instance = ensure_running(port, dir).await?;
    let mut url = reqwest::Url::parse(&instance.url())?;
    url.query_pairs_mut().append_pair("path", path);
    println!("{url}");
    if open_browser {
        open(url.as_str());
    }
    Ok(())
}

fn announce(instance: &Instance, dir: &Path) {
    println!("LGTM is running (pid {})", instance.pid);
    println!("UI: {}", instance.url());
    println!("Server: {}", instance.url());
    println!("Browser RPC: {}/rpc", instance.url());
    println!("Logs: {}", dir.join("server.log").display());
}

fn open(url: &str) {
    if let Err(error) = webbrowser::open(url) {
        eprintln!("Could not open a browser ({error}); open the URL above.");
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

pub async fn status(dir: &Path, json: bool) -> Result<()> {
    let client = client()?;
    if let Some(instance) = read(dir) {
        if running(&client, &instance).await {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "running": true,
                        "pid": instance.pid,
                        "ui_url": instance.url(),
                        "server_url": instance.url(),
                        "rpc_url": format!("{}/rpc", instance.url()),
                    })
                );
            } else {
                announce(&instance, dir);
            }
            return Ok(());
        }
    }
    if json {
        println!("{}", serde_json::json!({"running": false}));
    } else {
        println!("LGTM is not running");
    }
    Ok(())
}
