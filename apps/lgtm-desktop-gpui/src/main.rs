//! lgtm-desktop-gpui — native Rust review shell on top of gpui-kit.
//!
//! `gpui-kit` is the longbridge-packaged fork of GPUI that ships GPUI itself
//! (`use gpui_kit::*` IS GPUI) plus the `gpui-component` widget library:
//! `Input`, `Textarea`, `Button`, etc. Using it in place of Zed's raw gpui
//! git source lets the composer use a real text input (selection, arrows,
//! clipboard, IME) instead of a hand-rolled key handler.

mod client;
mod diff;
mod highlight;
mod proto;
mod review;
mod review_state;

use gpui_kit::assets::Assets;
use gpui_kit::*;

use crate::review::{SubmitComposer, CancelComposer, ReviewView};
use crate::review_state::ReviewConfig;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = ReviewConfig::from_env();
    eprintln!(
        "[lgtm-gpui] backend={} repo={} base={}",
        config.backend, config.repo_path, config.base_ref
    );

    let app = gpui_kit::application().with_assets(Assets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);

        // Keyboard shortcuts for the composer. Scoped to the "Composer"
        // key_context on the composer div (see review.rs) so they only fire
        // while a Textarea is focused in a composer, not globally.
        cx.bind_keys([
            KeyBinding::new("cmd-enter", SubmitComposer, Some("Composer")),
            KeyBinding::new("escape", CancelComposer, Some("Composer")),
        ]);

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("lgtm".into()),
                appears_transparent: true,
                traffic_light_position: Some(Point { x: px(9.), y: px(9.) }),
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };

        let config = config.clone();
        gpui_kit::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| ReviewView::new(config, window, cx))
        })
        .expect("open main window");
        cx.activate(true);
    });
}
