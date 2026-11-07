//! Application-wide event emitter wired in `app.rs` to publish backend events to the webview and test bridge.

use std::sync::Arc;

use tauri::Emitter;

#[cfg(any(test, feature = "e2e-bridge"))]
use crate::services::BridgeEventForwarder;

type EventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Application-wide event emitter publishing backend events to the frontend webview and E2E test bridge.
#[derive(Clone)]
pub struct AppEvents(EventSink);

impl AppEvents {
    #[cfg(any(test, feature = "e2e-bridge"))]
    pub(crate) fn from_app_handle<R: tauri::Runtime>(
        app_handle: &tauri::AppHandle<R>,
        event_forwarder: Option<BridgeEventForwarder>,
    ) -> Self {
        let emit_handle = app_handle.clone();
        Self(Arc::new(move |event, payload| {
            if let Some(cb) = &event_forwarder {
                cb(event, payload.clone());
            }
            let _ = emit_handle.emit(event, payload);
        }))
    }

    #[cfg(not(any(test, feature = "e2e-bridge")))]
    pub(crate) fn from_app_handle<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Self {
        let emit_handle = app_handle.clone();
        Self(Arc::new(move |event, payload| {
            let _ = emit_handle.emit(event, payload);
        }))
    }

    // Public ------------------------------------------------------------------

    /// Emits a named desktop event to the frontend webview and any registered test bridge forwarder.
    pub fn emit<T: serde::Serialize>(&self, event: &str, payload: &T) {
        if let Ok(value) = serde_json::to_value(payload) {
            (self.0)(event, value);
        }
    }
}
