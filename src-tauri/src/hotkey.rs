use tauri::{AppHandle, Manager, Runtime, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::{info, warn};

use crate::{dictation::DictationError, state::AppState};

fn default_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT), Code::Space)
}

fn rewrite_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space)
}

fn shortcut_from_label(label: &str) -> Option<Shortcut> {
    match label {
        "Option+Space" => Some(default_shortcut()),
        "Control+Space" => Some(Shortcut::new(Some(Modifiers::CONTROL), Code::Space)),
        "Command+Shift+Space" => Some(Shortcut::new(
            Some(Modifiers::META | Modifiers::SHIFT),
            Code::Space,
        )),
        "Control+Option+Space" => Some(Shortcut::new(
            Some(Modifiers::CONTROL | Modifiers::ALT),
            Code::Space,
        )),
        "Option+Shift+Space" => Some(rewrite_shortcut()),
        "Control+Shift+Space" => Some(Shortcut::new(
            Some(Modifiers::CONTROL | Modifiers::SHIFT),
            Code::Space,
        )),
        "Command+Option+Space" => Some(Shortcut::new(
            Some(Modifiers::META | Modifiers::ALT),
            Code::Space,
        )),
        _ => None,
    }
}

pub(crate) fn shortcut_pair_supported(dictation: &str, rewrite: &str) -> bool {
    dictation != rewrite
        && matches!(
            dictation,
            "Option+Space" | "Control+Space" | "Command+Shift+Space" | "Control+Option+Space"
        )
        && matches!(
            rewrite,
            "Option+Shift+Space" | "Control+Shift+Space" | "Command+Option+Space"
        )
}

fn replace_with_rollback<E>(
    new_shortcuts: [Shortcut; 2],
    previous_shortcuts: [Shortcut; 2],
    mut unregister_all: impl FnMut() -> Result<(), E>,
    mut register: impl FnMut(Shortcut) -> Result<(), E>,
) -> Result<(), String>
where
    E: std::fmt::Display,
{
    unregister_all().map_err(|error| error.to_string())?;
    for shortcut in new_shortcuts {
        if let Err(error) = register(shortcut) {
            let message = error.to_string();
            let _ = unregister_all();
            for previous in previous_shortcuts {
                let _ = register(previous);
            }
            return Err(message);
        }
    }
    Ok(())
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let Ok(settings) = app.state::<AppState>().product.settings() else {
                return;
            };
            if settings.dictation_paused {
                return;
            }
            let dictation_shortcut =
                shortcut_from_label(&settings.dictation_shortcut).unwrap_or_else(default_shortcut);
            let rewrite_shortcut =
                shortcut_from_label(&settings.rewrite_shortcut).unwrap_or_else(rewrite_shortcut);
            let rewrite = shortcut == &rewrite_shortcut;
            if shortcut != &dictation_shortcut && !rewrite {
                return;
            }

            let dictation = app.state::<AppState>().dictation.clone();
            match event.state() {
                ShortcutState::Pressed => match if rewrite {
                    dictation.begin_rewrite_press()
                } else {
                    dictation.begin_press()
                } {
                    Ok(Some(operation_id)) => {
                        let service = dictation.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            service.start_capture(&operation_id);
                        });
                    }
                    Ok(None) => {}
                    Err(error) => warn!(
                        subsystem = "shortcut",
                        event = "press_failed",
                        error_code = error.code(),
                        error = %error,
                        "shortcut press could not begin dictation"
                    ),
                },
                ShortcutState::Released => {
                    let result = if rewrite {
                        dictation.handle_rewrite_release()
                    } else {
                        dictation.handle_release()
                    };
                    if let Err(error) = result {
                        warn!(
                            subsystem = "shortcut",
                            event = "release_failed",
                            error_code = error.code(),
                            error = %error,
                            "shortcut release could not finalize dictation"
                        );
                    }
                }
            }
        })
        .build()
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> Result<(), DictationError> {
    let settings = app
        .state::<AppState>()
        .product
        .settings()
        .map_err(|error| DictationError::ShortcutRegistrationFailed(error.to_string()))?;
    let dictation = shortcut_from_label(&settings.dictation_shortcut).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("invalid dictation shortcut".to_owned())
    })?;
    let rewrite = shortcut_from_label(&settings.rewrite_shortcut).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("invalid rewrite shortcut".to_owned())
    })?;
    app.global_shortcut()
        .register(dictation)
        .map_err(|error| DictationError::ShortcutRegistrationFailed(error.to_string()))?;
    app.global_shortcut()
        .register(rewrite)
        .map_err(|error| DictationError::ShortcutRegistrationFailed(error.to_string()))?;
    info!(
        subsystem = "shortcut",
        event = "registered",
        shortcut = settings.dictation_shortcut,
        "global hold-to-talk shortcut registered"
    );
    info!(
        subsystem = "shortcut",
        event = "registered",
        shortcut = settings.rewrite_shortcut,
        "global hold-to-rewrite shortcut registered"
    );
    Ok(())
}

pub fn reconfigure<R: Runtime>(
    app: &AppHandle<R>,
    dictation_label: &str,
    rewrite_label: &str,
) -> Result<(), DictationError> {
    let dictation = shortcut_from_label(dictation_label).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("unsupported dictation shortcut".to_owned())
    })?;
    let rewrite = shortcut_from_label(rewrite_label).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("unsupported rewrite shortcut".to_owned())
    })?;
    if dictation == rewrite {
        return Err(DictationError::ShortcutRegistrationFailed(
            "shortcuts must be different".to_owned(),
        ));
    }
    let previous = app
        .state::<AppState>()
        .product
        .settings()
        .map_err(|error| DictationError::ShortcutRegistrationFailed(error.to_string()))?;
    let old_dictation = shortcut_from_label(&previous.dictation_shortcut).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("invalid stored dictation shortcut".to_owned())
    })?;
    let old_rewrite = shortcut_from_label(&previous.rewrite_shortcut).ok_or_else(|| {
        DictationError::ShortcutRegistrationFailed("invalid stored rewrite shortcut".to_owned())
    })?;
    replace_with_rollback(
        [dictation, rewrite],
        [old_dictation, old_rewrite],
        || app.global_shortcut().unregister_all(),
        |shortcut| app.global_shortcut().register(shortcut),
    )
    .map_err(DictationError::ShortcutRegistrationFailed)
}

pub fn unregister<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.global_shortcut().unregister_all() {
        warn!(
            subsystem = "shortcut",
            event = "cleanup_failed",
            error = %error,
            "failed to unregister global shortcuts during shutdown"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{replace_with_rollback, shortcut_from_label};

    #[test]
    fn supported_shortcuts_parse_and_unknown_values_fail_closed() {
        for label in [
            "Option+Space",
            "Control+Space",
            "Command+Shift+Space",
            "Control+Option+Space",
            "Option+Shift+Space",
            "Control+Shift+Space",
            "Command+Option+Space",
        ] {
            assert!(shortcut_from_label(label).is_some(), "{label}");
        }
        assert!(shortcut_from_label("Space").is_none());
        assert!(shortcut_from_label("Option+AnyKey").is_none());
    }

    #[test]
    fn failed_reconfiguration_restores_both_previous_shortcuts() {
        let new_dictation = shortcut_from_label("Control+Space").expect("new dictation");
        let new_rewrite = shortcut_from_label("Control+Shift+Space").expect("new rewrite");
        let old_dictation = shortcut_from_label("Option+Space").expect("old dictation");
        let old_rewrite = shortcut_from_label("Option+Shift+Space").expect("old rewrite");
        let registered = std::cell::RefCell::new(Vec::new());
        let attempts = std::cell::Cell::new(0_u8);
        let result = replace_with_rollback(
            [new_dictation, new_rewrite],
            [old_dictation, old_rewrite],
            || {
                registered.borrow_mut().clear();
                Ok::<_, &'static str>(())
            },
            |shortcut| {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 2 {
                    Err("conflict")
                } else {
                    registered.borrow_mut().push(shortcut);
                    Ok(())
                }
            },
        );
        assert_eq!(result, Err("conflict".to_owned()));
        assert_eq!(*registered.borrow(), vec![old_dictation, old_rewrite]);
    }
}
