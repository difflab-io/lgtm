//! lgtm-desktop-gpui — native Rust review shell on top of Zed's gpui.
//!
//! Boot sequence:
//! 1. `gpui_platform::application().run(...)` starts the platform event loop.
//! 2. The callback reads `LGTM_ADDR` / `LGTM_REPO` / `LGTM_BASE` into a
//!    [`ReviewConfig`], opens one window, and mounts a [`ReviewView`].
//! 3. The view spawns background requests to the lgtm-server and renders
//!    changed files + colored, syntax-highlighted diffs + comment threads.

mod client;
mod diff;
mod highlight;
mod proto;
mod review;
mod review_state;

use gpui::{
    App, AppContext, Bounds, Point, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, point, px, size,
};
use gpui_platform::application;

use crate::review::ReviewView;
use crate::review_state::ReviewConfig;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = ReviewConfig::from_env();
    eprintln!(
        "[lgtm-gpui] backend={} repo={} base={}",
        config.backend, config.repo_path, config.base_ref
    );

    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let config = config.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Appear-transparent titlebar lets the root view's background
                // bleed into the titlebar region, so the system chrome matches
                // the rest of the app instead of showing default white/grey.
                titlebar: Some(TitlebarOptions {
                    title: Some("lgtm".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(Point { x: px(9.), y: px(9.) }),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| ReviewView::new(config, cx)),
        )
        .expect("open main window");
        cx.activate(true);
    });

    // `point` is kept imported above for symmetry even if unused elsewhere.
    let _ = point::<gpui::Pixels>;
}
