//! lgtm-desktop-gpui — native Rust review shell on top of Zed's gpui.
//!
//! Boot sequence:
//! 1. `gpui_platform::application().run(...)` starts the platform event loop.
//! 2. The callback opens one window and creates a [`ReviewView`] root entity.
//! 3. The view renders a two-pane layout against hard-coded stub data; a
//!    follow-up pass will swap the stub for live calls to lgtm-grpc.

mod review;
mod stub;

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

use crate::review::ReviewView;

fn main() {
    // Install env_logger so GPUI's internal `log::*` output surfaces when
    // RUST_LOG=debug (or similar) is set.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    eprintln!("[lgtm-gpui] starting; stub::files() has {} entries", crate::stub::files().len());

    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        eprintln!("[lgtm-gpui] opening window at {:?}", bounds);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(ReviewView::new),
        )
        .expect("open main window");
        cx.activate(true);
        eprintln!("[lgtm-gpui] window opened, activated");
    });
}
