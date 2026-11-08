//! Native application menu bar construction and menu event routing for `lgtm-desktop`.

use crate::events::{AppEvent, AppEvents, FileOpenedEvent};

/// Builds the native Tauri application menu bar with global (`CmdOrCtrl+,`) and project (`CmdOrCtrl+Shift+,`) settings actions.
pub fn build_app_menu<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let pkg_info = app_handle.package_info();
    let config = app_handle.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let settings_item = MenuItem::with_id(
        app_handle,
        "open-global-config",
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let project_settings_item = MenuItem::with_id(
        app_handle,
        "open-local-config",
        "Project Settings…",
        true,
        Some("CmdOrCtrl+Shift+,"),
    )?;

    let window_menu = Submenu::with_id_and_items(
        app_handle,
        tauri::menu::WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app_handle, None)?,
            &PredefinedMenuItem::maximize(app_handle, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::close_window(app_handle, None)?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        app_handle,
        tauri::menu::HELP_SUBMENU_ID,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app_handle, None, Some(about_metadata.clone()))?,
        ],
    )?;

    Menu::with_items(
        app_handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app_handle, None, Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &settings_item,
                    &project_settings_item,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::services(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::hide(app_handle, None)?,
                    &PredefinedMenuItem::hide_others(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::quit(app_handle, None)?,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                app_handle,
                "File",
                true,
                &[
                    #[cfg(not(target_os = "macos"))]
                    &settings_item,
                    #[cfg(not(target_os = "macos"))]
                    &project_settings_item,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::close_window(app_handle, None)?,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::quit(app_handle, None)?,
                ],
            )?,
            &Submenu::with_items(
                app_handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app_handle, None)?,
                    &PredefinedMenuItem::redo(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::cut(app_handle, None)?,
                    &PredefinedMenuItem::copy(app_handle, None)?,
                    &PredefinedMenuItem::paste(app_handle, None)?,
                    &PredefinedMenuItem::select_all(app_handle, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app_handle, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )
}

/// Dispatches native menu item selections onto the backend [`AppEvents`] bus.
pub fn handle_menu_event(events: &AppEvents, menu_id: &str) {
    match menu_id {
        "open-global-config" => {
            events.dispatch(AppEvent::FileOpened(FileOpenedEvent {
                path: "~/.lgtm/config.json".to_string(),
                title: Some("Settings".to_string()),
                offset: None,
            }));
        }
        "open-local-config" => {
            events.dispatch(AppEvent::FileOpened(FileOpenedEvent {
                path: ".lgtm/config.json".to_string(),
                title: Some("Settings (project)".to_string()),
                offset: None,
            }));
        }
        _ => {}
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // tests -------------------------------------------------------------------

    mod given_an_app_menu_event_id {
        use super::*;

        mod when_handling_menu_events {
            use super::*;

            #[test]
            fn should_dispatch_global_and_local_config_events_and_ignore_unknown_ids() {
                // Arrange
                let recorded: Arc<Mutex<Vec<FileOpenedEvent>>> = Arc::new(Mutex::new(Vec::new()));
                let recorded_clone = recorded.clone();
                let events = AppEvents::default();
                events.set_handler(move |event| {
                    if let AppEvent::FileOpened(payload) = event {
                        recorded_clone.lock().unwrap().push(payload);
                    }
                });

                // Act
                handle_menu_event(&events, "open-global-config");
                handle_menu_event(&events, "open-local-config");
                handle_menu_event(&events, "unrelated-menu-item");

                // Assert
                assert_eq!(
                    *recorded.lock().unwrap(),
                    vec![
                        FileOpenedEvent {
                            path: "~/.lgtm/config.json".to_string(),
                            title: Some("Settings".to_string()),
                            offset: None,
                        },
                        FileOpenedEvent {
                            path: ".lgtm/config.json".to_string(),
                            title: Some("Settings (project)".to_string()),
                            offset: None,
                        },
                    ]
                );
            }
        }
    }
}
