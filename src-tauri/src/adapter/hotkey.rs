use std::str::FromStr;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::application::config::RuntimeConfigState;

pub const HOTKEY_DISABLED_MARKER: &str = "[HOTKEY_DISABLED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityAction {
    Hide,
    ShowAndFocus,
}

pub fn bootstrap<R: Runtime>(app: &AppHandle<R>) {
    let configured_hotkey = {
        let state = app.state::<RuntimeConfigState>();
        state.config.hotkey.trim().to_string()
    };

    match register_global_hotkey(app, &configured_hotkey) {
        Ok(()) => {
            tracing::info!(
                component = "hotkey",
                hotkey = %configured_hotkey,
                "global hotkey registered"
            );
        }
        Err(error) => {
            let warning = build_hotkey_disabled_warning(&configured_hotkey, &error);
            tracing::warn!(
                component = "hotkey",
                hotkey = %configured_hotkey,
                error = %error,
                warning = %warning,
                "global hotkey disabled, startup continues"
            );
            append_hotkey_disabled_marker(app);
        }
    }
}

fn register_global_hotkey<R: Runtime>(
    app: &AppHandle<R>,
    configured_hotkey: &str,
) -> Result<(), String> {
    let shortcut = Shortcut::from_str(configured_hotkey)
        .map_err(|error| format!("failed to parse hotkey `{configured_hotkey}`: {error}"))?;

    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }

            if let Err(error) = toggle_main_window_visibility(app) {
                tracing::warn!(
                    component = "hotkey",
                    error = %error,
                    "failed to toggle main window visibility by global hotkey"
                );
            }
        })
        .map_err(|error| format!("failed to register global hotkey `{configured_hotkey}`: {error}"))
}

fn toggle_main_window_visibility<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window not found".to_string());
    };

    let is_visible = window
        .is_visible()
        .map_err(|error| format!("failed to read window visibility: {error}"))?;

    match resolve_visibility_action(is_visible) {
        VisibilityAction::Hide => window
            .hide()
            .map_err(|error| format!("failed to hide main window: {error}"))?,
        VisibilityAction::ShowAndFocus => {
            let is_minimized = window
                .is_minimized()
                .map_err(|error| format!("failed to read minimized state: {error}"))?;
            if is_minimized {
                window
                    .unminimize()
                    .map_err(|error| format!("failed to unminimize main window: {error}"))?;
            }
            window
                .show()
                .map_err(|error| format!("failed to show main window: {error}"))?;
            window
                .set_focus()
                .map_err(|error| format!("failed to focus main window: {error}"))?;
        }
    }

    Ok(())
}

fn resolve_visibility_action(is_visible: bool) -> VisibilityAction {
    if is_visible {
        VisibilityAction::Hide
    } else {
        VisibilityAction::ShowAndFocus
    }
}

fn append_hotkey_disabled_marker<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!(
            component = "hotkey",
            "main window not found while setting hotkey disabled marker"
        );
        return;
    };

    let current_title = match window.title() {
        Ok(title) => title,
        Err(error) => {
            tracing::warn!(
                component = "hotkey",
                error = %error,
                "failed to read current window title for hotkey disabled marker"
            );
            return;
        }
    };

    let next_title = append_hotkey_marker_to_title(&current_title);
    if next_title == current_title {
        return;
    }

    if let Err(error) = window.set_title(&next_title) {
        tracing::warn!(
            component = "hotkey",
            error = %error,
            "failed to set hotkey disabled marker on window title"
        );
    }
}

fn append_hotkey_marker_to_title(current_title: &str) -> String {
    if current_title.contains(HOTKEY_DISABLED_MARKER) {
        current_title.to_string()
    } else {
        format!("{current_title} {HOTKEY_DISABLED_MARKER}")
    }
}

fn build_hotkey_disabled_warning(configured_hotkey: &str, error: &str) -> String {
    format!("global hotkey disabled: `{configured_hotkey}` registration failed ({error})")
}

#[cfg(test)]
mod tests {
    use super::{
        append_hotkey_marker_to_title, build_hotkey_disabled_warning, resolve_visibility_action,
        VisibilityAction, HOTKEY_DISABLED_MARKER,
    };

    #[test]
    fn resolves_hide_action_when_window_is_visible() {
        assert_eq!(resolve_visibility_action(true), VisibilityAction::Hide);
    }

    #[test]
    fn resolves_show_and_focus_action_when_window_is_hidden() {
        assert_eq!(
            resolve_visibility_action(false),
            VisibilityAction::ShowAndFocus
        );
    }

    #[test]
    fn appends_hotkey_disabled_marker_once() {
        let title = append_hotkey_marker_to_title("Remember [sqlite_only]");
        assert_eq!(title, "Remember [sqlite_only] [HOTKEY_DISABLED]");
    }

    #[test]
    fn keeps_existing_hotkey_disabled_marker() {
        let original = format!("Remember [sqlite_only] {HOTKEY_DISABLED_MARKER}");
        let title = append_hotkey_marker_to_title(&original);
        assert_eq!(title, original);
    }

    #[test]
    fn builds_hotkey_disabled_warning_with_hotkey_and_error() {
        let warning = build_hotkey_disabled_warning("Alt+Space", "already registered");
        assert!(warning.contains("Alt+Space"));
        assert!(warning.contains("already registered"));
        assert!(warning.contains("global hotkey disabled"));
    }
}
