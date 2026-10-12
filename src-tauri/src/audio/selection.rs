use std::sync::{Arc, PoisonError, RwLock};

use serde::Serialize;

const MAX_DEVICE_FIELD_CHARACTERS: usize = 512;

/// Shared, cheaply readable microphone preference. Capture reads it once per
/// recording start; Settings replaces it after the preference is persisted.
/// `None` follows the macOS system default input.
#[derive(Clone, Debug, Default)]
pub struct MicrophoneSelection(Arc<RwLock<Option<String>>>);

impl MicrophoneSelection {
    pub fn new(device_id: Option<String>) -> Self {
        Self(Arc::new(RwLock::new(device_id)))
    }

    pub fn current(&self) -> Option<String> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, device_id: Option<String>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = device_id;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneOptions {
    pub devices: Vec<InputDeviceInfo>,
    pub selected_id: Option<String>,
    pub selected_name: Option<String>,
    pub selected_available: bool,
}

impl MicrophoneOptions {
    pub fn new(
        devices: Vec<InputDeviceInfo>,
        selected_id: Option<String>,
        selected_name: Option<String>,
    ) -> Self {
        let selected_available = selected_id
            .as_deref()
            .is_none_or(|id| devices.iter().any(|device| device.id == id));
        Self {
            devices,
            selected_id,
            selected_name,
            selected_available,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceChoice {
    SystemDefault,
    Selected,
    /// The saved device is disconnected; capture continues on the system
    /// default rather than losing the user's speech.
    SelectedUnavailable,
}

pub fn resolve_device<D>(
    selected_id: Option<&str>,
    find_by_id: impl FnOnce(&str) -> Option<D>,
    system_default: impl FnOnce() -> Option<D>,
) -> Option<(D, DeviceChoice)> {
    let Some(id) = selected_id else {
        return system_default().map(|device| (device, DeviceChoice::SystemDefault));
    };
    match find_by_id(id) {
        Some(device) => Some((device, DeviceChoice::Selected)),
        None => system_default().map(|device| (device, DeviceChoice::SelectedUnavailable)),
    }
}

/// System default first, then alphabetical, so the picker is stable.
pub fn sort_devices(devices: &mut [InputDeviceInfo]) {
    devices.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn valid_device_field(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_DEVICE_FIELD_CHARACTERS
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str, is_default: bool) -> InputDeviceInfo {
        InputDeviceInfo {
            id: id.to_owned(),
            name: name.to_owned(),
            is_default,
        }
    }

    #[test]
    fn no_preference_uses_the_system_default_without_lookup() {
        let resolved = resolve_device(
            None,
            |_| -> Option<&str> { panic!("lookup must not run without a preference") },
            || Some("default"),
        );
        assert_eq!(resolved, Some(("default", DeviceChoice::SystemDefault)));
    }

    #[test]
    fn connected_preference_is_used() {
        let resolved = resolve_device(
            Some("coreaudio:usb"),
            |id| (id == "coreaudio:usb").then_some("usb"),
            || Some("default"),
        );
        assert_eq!(resolved, Some(("usb", DeviceChoice::Selected)));
    }

    #[test]
    fn disconnected_preference_falls_back_to_the_system_default() {
        let resolved = resolve_device(Some("coreaudio:gone"), |_| None, || Some("default"));
        assert_eq!(
            resolved,
            Some(("default", DeviceChoice::SelectedUnavailable))
        );
    }

    #[test]
    fn no_devices_at_all_resolves_to_none() {
        assert_eq!(
            resolve_device::<&str>(Some("coreaudio:gone"), |_| None, || None),
            None
        );
        assert_eq!(resolve_device::<&str>(None, |_| None, || None), None);
    }

    #[test]
    fn devices_sort_default_first_then_by_name() {
        let mut devices = vec![
            device("c", "zoom H1", false),
            device("b", "MacBook Microphone", true),
            device("a", "AirPods", false),
        ];
        sort_devices(&mut devices);
        let names: Vec<_> = devices.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["MacBook Microphone", "AirPods", "zoom H1"]);
    }

    #[test]
    fn options_report_whether_the_saved_device_is_connected() {
        let devices = vec![device("coreaudio:built-in", "MacBook Microphone", true)];
        assert!(MicrophoneOptions::new(devices.clone(), None, None).selected_available);
        assert!(
            MicrophoneOptions::new(
                devices.clone(),
                Some("coreaudio:built-in".to_owned()),
                Some("MacBook Microphone".to_owned()),
            )
            .selected_available
        );
        assert!(
            !MicrophoneOptions::new(
                devices,
                Some("coreaudio:usb".to_owned()),
                Some("USB Mic".to_owned()),
            )
            .selected_available
        );
    }

    #[test]
    fn device_fields_reject_empty_control_and_oversized_values() {
        assert!(valid_device_field("coreaudio:BuiltInMicrophoneDevice"));
        assert!(valid_device_field("Studio Mic — Ü"));
        assert!(!valid_device_field(""));
        assert!(!valid_device_field("   "));
        assert!(!valid_device_field("bad\nname"));
        assert!(!valid_device_field(&"x".repeat(513)));
    }

    #[test]
    fn selection_updates_are_visible_to_clones() {
        let selection = MicrophoneSelection::new(None);
        let capture_view = selection.clone();
        selection.set(Some("coreaudio:usb".to_owned()));
        assert_eq!(capture_view.current().as_deref(), Some("coreaudio:usb"));
        selection.set(None);
        assert_eq!(capture_view.current(), None);
    }
}
