#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod events;
#[cfg(any(test, feature = "e2e-bridge"))]
mod ipc_bridge;
mod menus;

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
            ipc_bridge::IpcBridgeService::new(home).run();
            return;
        }
        _ => {}
    }
    app::run();
}
