use anyhow::{Context, Result};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

// types -----------------------------------------------------------------------

pub struct Store {
    dir: PathBuf,
}

pub(crate) struct Instance {
    pub pid: u32,
    pub port: u16,
}

impl Instance {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

// api -------------------------------------------------------------------------

impl Store {
    pub fn new() -> Result<Self> {
        let dir = std::env::var_os("LGTM_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".lgtm")))
            .context("Cannot find a home directory; set LGTM_HOME")?;
        let dir = if dir.is_absolute() {
            dir
        } else {
            std::env::current_dir()?.join(dir)
        };
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn ensure_exists(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    pub(crate) fn instance(&self) -> Option<Instance> {
        Some(Instance {
            pid: fs::read_to_string(self.path("server.pid"))
                .ok()?
                .trim()
                .parse()
                .ok()?,
            port: fs::read_to_string(self.path("server.port"))
                .ok()?
                .trim()
                .parse()
                .ok()?,
        })
    }

    pub fn write_port(&self, port: u16) -> Result<()> {
        fs::write(self.path("server.port"), port.to_string())?;
        Ok(())
    }

    pub fn clear_port(&self) {
        fs::remove_file(self.path("server.port")).ok();
    }

    pub fn foreground_pid_lock(&self) -> Result<File> {
        let mut file = private_file(&self.path("server.pid"))?;
        file.try_lock_exclusive()
            .context("Another LGTM instance is starting or running")?;
        file.set_len(0)?;
        write!(file, "{}", std::process::id())?;
        Ok(file)
    }
}

// helpers ---------------------------------------------------------------------

fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
