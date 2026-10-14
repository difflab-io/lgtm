use crate::store::{Instance, Store};
use anyhow::{Context, Result, bail};
use std::{process::Command, time::Duration};

// api -------------------------------------------------------------------------

#[cfg(unix)]
pub fn detach(store: &Store) -> Result<()> {
    let log = crate::log::output(store)?;
    daemonize::Daemonize::new()
        .working_directory(store.dir())
        .pid_file(store.path("server.pid"))
        .umask(0o077)
        .stdout(log.try_clone()?)
        .stderr(log)
        .start()
        .context("Daemonize the LGTM server")
}

#[cfg(not(unix))]
pub fn detach(_store: &Store) -> Result<()> {
    bail!("Background mode requires macOS or Linux; use lgtm up --foreground")
}

pub async fn up(port: u16, open_browser: bool, store: &Store) -> Result<()> {
    let instance = ensure_running(port, store).await?;
    announce(&instance, store);
    if open_browser {
        crate::ui::open(&instance.url());
    }
    Ok(())
}

pub async fn down(store: &Store) -> Result<()> {
    let Some(instance) = active(&client()?, store).await else {
        println!("LGTM is not running");
        return Ok(());
    };
    #[cfg(unix)]
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(i32::try_from(instance.pid)?),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    #[cfg(not(unix))]
    bail!("Use Ctrl-C to stop the foreground server on this platform");
    for _ in 0..50 {
        if !store.path("server.port").exists() {
            println!("LGTM stopped");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!(
        "LGTM is still shutting down; check {}",
        store.path("server.log").display()
    )
}

pub async fn status(store: &Store, json: bool) -> Result<()> {
    let instance = active(&client()?, store).await;
    if json {
        let discovery = match instance {
            Some(instance) => serde_json::json!({
                "running": true,
                "pid": instance.pid,
                "ui_url": instance.url(),
                "server_url": instance.url(),
                "rpc_url": format!("{}/rpc", instance.url()),
            }),
            None => serde_json::json!({"running": false}),
        };
        println!("{discovery}");
    } else if let Some(instance) = instance {
        announce(&instance, store);
    } else {
        println!("LGTM is not running");
    }
    Ok(())
}

// helpers ---------------------------------------------------------------------

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .no_proxy()
        .build()?)
}

async fn active(client: &reqwest::Client, store: &Store) -> Option<Instance> {
    let instance = store.instance()?;
    let body = client
        .get(format!("{}/health", instance.url()))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;
    (body["pid"].as_u64() == Some(instance.pid.into())).then_some(instance)
}

pub(crate) async fn ensure_running(port: u16, store: &Store) -> Result<Instance> {
    let client = client()?;
    if let Some(instance) = active(&client, store).await {
        return Ok(instance);
    }
    let launch = Command::new(std::env::current_exe()?)
        .env("LGTM_HOME", store.dir())
        .args(["serve", "--daemonize", "--port"])
        .arg(port.to_string())
        .output()
        .context("Start the bundled LGTM server")?;
    if !launch.status.success() {
        bail!(
            "Could not daemonize the LGTM server: {}",
            String::from_utf8_lossy(&launch.stderr).trim()
        );
    }
    for _ in 0..50 {
        if let Some(instance) = active(&client, store).await {
            return Ok(instance);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!(
        "LGTM did not become ready; check {}. {}",
        store.path("server.log").display(),
        String::from_utf8_lossy(&launch.stderr).trim()
    )
}

fn announce(instance: &Instance, store: &Store) {
    println!("LGTM is running (pid {})", instance.pid);
    println!("UI: {}", instance.url());
    println!("Server: {}", instance.url());
    println!("Browser RPC: {}/rpc", instance.url());
    println!("Logs: {}", store.path("server.log").display());
}
