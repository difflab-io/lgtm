use crate::store::Store;
use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions},
    sync::Mutex,
};
use tracing_subscriber::EnvFilter;

// api -------------------------------------------------------------------------

pub fn init(store: &Store) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .with_writer(Mutex::new(output(store)?))
        .try_init()
        .map_err(anyhow::Error::msg)
}

pub fn output(store: &Store) -> Result<File> {
    store.ensure_exists()?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(store.path("server.log"))
        .context("Open the server log")
}
