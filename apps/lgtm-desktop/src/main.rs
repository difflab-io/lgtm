#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
pub mod error;
pub mod events;
pub mod models;
pub mod services;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--export-bindings") => {
            let path = args
                .next()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../lgtm-web/src/lib/gen/bindings.ts")
                });
            app::export_typescript_bindings_to(&path)
                .expect("failed to export TypeScript bindings");
            return;
        }
        #[cfg(any(test, feature = "e2e-bridge"))]
        Some("--ipc-bridge") => {
            let home = args
                .next()
                .map(std::path::PathBuf::from)
                .expect("missing home directory for --ipc-bridge");
            services::IpcBridgeService::new(home).run();
            return;
        }
        _ => {}
    }
    #[cfg(target_os = "macos")]
    ensure_macos_aqua_session();
    app::run();
}

#[cfg(target_os = "macos")]
fn ensure_macos_aqua_session() {
    use std::io::Read;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::process::Command;
    use std::time::{Duration, Instant};

    if let Some(sock_path) = std::env::var_os("LGTM_AQUA_WATCH_SOCK") {
        if let Ok(mut stream) = UnixStream::connect(&sock_path) {
            std::thread::spawn(move || {
                let mut buf = [0u8; 1];
                let _ = stream.read(&mut buf);
                std::process::exit(0);
            });
        }
        return;
    }

    if std::env::var_os("LGTM_AQUA_SESSION").is_some()
        || std::env::var_os("LGTM_IPC_BRIDGE").is_some()
        || std::env::var_os("CI").is_some()
    {
        return;
    }

    let is_aqua = Command::new("launchctl")
        .arg("managername")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "Aqua")
        .unwrap_or(true);
    if is_aqua {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let Ok(uid) = Command::new("launchctl")
        .arg("manageruid")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    else {
        return;
    };
    if uid.is_empty() {
        return;
    }
    let pid = std::process::id();
    let label = format!("com.lgtm.desktop.aqua.{pid}");
    let temp_dir = std::env::temp_dir();
    let sock_path = temp_dir.join(format!("{label}.sock"));
    let plist_path = temp_dir.join(format!("{label}.plist"));
    let _ = std::fs::remove_file(&sock_path);

    let Ok(listener) = UnixListener::bind(&sock_path) else {
        return;
    };
    let _ = listener.set_nonblocking(true);

    let mut args_xml = String::new();
    for arg in std::iter::once(exe.to_string_lossy().into_owned()).chain(std::env::args().skip(1)) {
        args_xml.push_str("        <string>");
        args_xml.push_str(&xml_escape(&arg));
        args_xml.push_str("</string>\n");
    }

    let mut env_xml = String::new();
    for (k, v) in std::env::vars() {
        env_xml.push_str("        <key>");
        env_xml.push_str(&xml_escape(&k));
        env_xml.push_str("</key>\n        <string>");
        env_xml.push_str(&xml_escape(&v));
        env_xml.push_str("</string>\n");
    }
    env_xml.push_str("        <key>LGTM_AQUA_SESSION</key>\n        <string>1</string>\n");
    env_xml.push_str("        <key>LGTM_AQUA_WATCH_SOCK</key>\n        <string>");
    env_xml.push_str(&xml_escape(&sock_path.to_string_lossy()));
    env_xml.push_str("</string>\n");

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
             <key>Label</key>\n\
             <string>{label}</string>\n\
             <key>ProgramArguments</key>\n\
             <array>\n\
         {args_xml}    </array>\n\
             <key>WorkingDirectory</key>\n\
             <string>{cwd}</string>\n\
             <key>EnvironmentVariables</key>\n\
             <dict>\n\
         {env_xml}    </dict>\n\
             <key>RunAtLoad</key>\n\
             <true/>\n\
         </dict>\n\
         </plist>\n",
        label = xml_escape(&label),
        cwd = xml_escape(&cwd.to_string_lossy()),
    );

    if std::fs::write(&plist_path, plist).is_err() {
        let _ = std::fs::remove_file(&sock_path);
        return;
    }

    let domain = format!("gui/{uid}");
    let service = format!("{domain}/{label}");
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist_path)
        .status();
    let _ = std::fs::remove_file(&plist_path);

    if !status.map(|s| s.success()).unwrap_or(false) {
        let _ = std::fs::remove_file(&sock_path);
        return;
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut connected = None;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                connected = Some(stream);
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }

    if let Some(mut stream) = connected {
        let _ = stream.set_nonblocking(false);
        let mut buf = [0u8; 1];
        let _ = stream.read(&mut buf);
    }

    let _ = Command::new("launchctl")
        .args(["bootout", &service])
        .status();
    let _ = std::fs::remove_file(&sock_path);
    std::process::exit(0);
}

#[cfg(target_os = "macos")]
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
