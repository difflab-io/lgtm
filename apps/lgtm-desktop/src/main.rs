#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
pub mod error;
pub mod models;
pub mod services;

fn main() {
    #[cfg(any(test, feature = "e2e-bridge"))]
    {
        let mut args = std::env::args().skip(1);
        if let Some(flag) = args.next()
            && flag == "--ipc-bridge"
        {
            let home = args
                .next()
                .map(std::path::PathBuf::from)
                .expect("missing home directory for --ipc-bridge");
            services::IpcBridgeService::new(home).run();
            return;
        }
    }
    app::run();
}
