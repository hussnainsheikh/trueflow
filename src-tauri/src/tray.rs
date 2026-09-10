use tauri::{
    AppHandle, Emitter, Manager, Runtime,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tracing::warn;

use crate::{projects::ProjectContextSource, state::AppState};

const NAVIGATE_EVENT: &str = "product://navigate";

pub fn create<R: Runtime>(app: &AppHandle<R>) -> Result<(), tauri::Error> {
    let settings = app.state::<AppState>().product.settings().ok();
    let project = app.state::<AppState>().projects.active_snapshot().ok();
    let status = MenuItem::with_id(
        app,
        "status",
        tray_status(
            &app.state::<AppState>(),
            settings
                .as_ref()
                .is_some_and(|value| value.dictation_paused),
        ),
        false,
        None::<&str>,
    )?;
    let context = project.as_ref().map_or_else(
        || "Project: unavailable".to_owned(),
        |value| {
            format!(
                "Project: {} · Voice: {} · Mode: {}",
                value.project_name, value.voice.name, value.mode.name
            )
        },
    );
    let context = MenuItem::with_id(app, "context", context, false, None::<&str>)?;
    let shortcuts = settings.as_ref().map_or_else(
        || "Shortcuts: unavailable".to_owned(),
        |value| {
            format!(
                "Dictate: {} · Rewrite: {}",
                value.dictation_shortcut, value.rewrite_shortcut
            )
        },
    );
    let shortcuts = MenuItem::with_id(app, "shortcuts", shortcuts, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Trueflow", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause Dictation", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Trueflow", true, None::<&str>)?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &context,
            &shortcuts,
            &separator_one,
            &open,
            &settings_item,
            &history,
            &pause,
            &separator_two,
            &quit,
        ],
    )?;
    let pause_handle = pause.clone();
    let status_handle = status.clone();
    let status_for_open = status.clone();
    let context_for_open = context.clone();
    let shortcuts_for_open = shortcuts.clone();
    let pause_for_open = pause.clone();

    TrayIconBuilder::with_id("trueflow")
        .icon(tauri::include_image!("icons/32x32.png"))
        .icon_as_template(true)
        .tooltip("Trueflow")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_tray_icon_event(move |tray, _event| {
            let app = tray.app_handle();
            let state = app.state::<AppState>();
            if let Ok(settings) = state.product.settings() {
                let _ = pause_for_open.set_text(if settings.dictation_paused {
                    "Resume Dictation"
                } else {
                    "Pause Dictation"
                });
                let _ = shortcuts_for_open.set_text(format!(
                    "Dictate: {} · Rewrite: {}",
                    settings.dictation_shortcut, settings.rewrite_shortcut
                ));
                let _ = status_for_open.set_text(tray_status(&state, settings.dictation_paused));
            }
            if let Ok(project) = state.projects.active_snapshot() {
                let _ = context_for_open.set_text(format!(
                    "Project: {} · Voice: {} · Mode: {}",
                    project.project_name, project.voice.name, project.mode.name
                ));
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show(app, "home"),
            "settings" => show(app, "settings"),
            "history" => show(app, "history"),
            "pause" => match app.state::<AppState>().product.toggle_pause() {
                Ok(settings) => {
                    let _ = pause_handle.set_text(if settings.dictation_paused {
                        "Resume Dictation"
                    } else {
                        "Pause Dictation"
                    });
                    let _ = status_handle.set_text(if settings.dictation_paused {
                        "Status: Paused"
                    } else {
                        "Status: Ready"
                    });
                }
                Err(error) => warn!(
                    subsystem = "tray",
                    event = "pause_failed",
                    error_code = error.code(),
                    "could not update dictation pause state"
                ),
            },
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn tray_status(state: &AppState, paused: bool) -> &'static str {
    if paused {
        return "Status: Paused";
    }
    let Ok(snapshot) = state.dictation.status() else {
        return "Status: Needs attention";
    };
    if snapshot.microphone_permission != crate::audio::MicrophonePermissionState::Granted {
        return "Status: Needs Microphone Permission";
    }
    if snapshot.accessibility_permission != crate::insertion::AccessibilityPermissionState::Granted
    {
        return "Status: Needs Accessibility Permission";
    }
    match snapshot.state {
        crate::dictation::DictationState::Starting
        | crate::dictation::DictationState::Listening => "Status: Listening",
        crate::dictation::DictationState::Finalizing => "Status: Finalizing",
        crate::dictation::DictationState::Transforming => "Status: Transforming",
        crate::dictation::DictationState::Inserting => "Status: Inserting",
        crate::dictation::DictationState::RecoverableError
        | crate::dictation::DictationState::FatalError => "Status: Needs attention",
        _ => {
            if state
                .product
                .account_status()
                .is_ok_and(|account| !account.cloud_ready)
            {
                "Status: Sign in required"
            } else {
                "Status: Ready"
            }
        }
    }
}

fn show<R: Runtime>(app: &AppHandle<R>, destination: &'static str) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.show().and_then(|_| window.set_focus()) {
            warn!(subsystem="window", event="show_failed", error=%error, "main window could not be shown");
        }
        let _ = app.emit(NAVIGATE_EVENT, destination);
    }
}
