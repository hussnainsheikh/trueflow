use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

use core_foundation::{
    array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayRef},
    base::{CFGetTypeID, CFHash, CFRelease, CFTypeRef, TCFType},
    boolean::CFBoolean,
    string::{CFString, CFStringGetTypeID, CFStringRef},
};
use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, KeyCode},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::{NSString, NSURL};
use tracing::warn;

use crate::insertion::{
    AccessibilityPermissionState, ClipboardSnapshot, InsertionBackend, InsertionError,
    InsertionErrorKind, TargetContext, TargetEditability,
};
use crate::rewrite::{
    DirectRewriteResult, RewriteBackend, RewriteError, RewriteErrorKind, SelectionSnapshot,
    Utf16Range,
};

const TRUEFLOW_BUNDLE_ID: &str = "com.trueflow.desktop";
// Some Electron, Office, and custom editors read the pasteboard asynchronously
// after receiving Cmd+V. Restoring it too quickly makes the event appear to
// succeed while those applications paste the previous value (or nothing).
const PASTEBOARD_RESTORE_DELAY: Duration = Duration::from_millis(1_200);
// A down/up pair posted back-to-back is not handled consistently by custom
// editor event loops. Keep the chord brief, but observable as a real key press.
const PASTE_KEY_HOLD_DELAY: Duration = Duration::from_millis(12);
const PASTE_MODIFIER_SETTLE_DELAY: Duration = Duration::from_millis(5);
// The hold-to-talk shortcut reports release as soon as Space rises, so Option is
// routinely still physically down when insertion runs a few hundred milliseconds
// later. Posting Command+V into that state makes the window server resolve the
// chord as Command+Option+V, which is unbound in editors that rely on their own
// key handling. Wait for the physical modifiers to clear before dispatching.
const PASTE_MODIFIER_CLEAR_TIMEOUT: Duration = Duration::from_millis(600);
const PASTE_MODIFIER_POLL_INTERVAL: Duration = Duration::from_millis(10);
const COMBINED_SESSION_STATE: i32 = 0;
/// Device-independent Shift, Control, Option, and Command bits. A held key in
/// this mask changes which command the synthetic paste chord resolves to.
const CONFLICTING_MODIFIER_MASK: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000;
// Synthetic typing posts the transcript as keyboard input. Long strings in a
// single event are truncated by some targets, so the text is chunked. The
// interval keeps a fast target from coalescing chunks out of order without
// making a normal transcript perceptibly slow.
const TYPING_CHUNK_UTF16: usize = 20;
const TYPING_CHUNK_DELAY: Duration = Duration::from_millis(2);
const AX_SUCCESS: i32 = 0;

type AXUIElementRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXValueGetValue(value: CFTypeRef, value_type: i32, out: *mut c_void) -> u8;
}

const AX_VALUE_CF_RANGE_TYPE: i32 = 4;

/// Mirrors `CFRange`, the payload of `AXSelectedTextRange`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AxRange {
    location: isize,
    length: isize,
}

pub struct MacOsInsertionBackend {
    /// Pasteboard change count of the transcript Trueflow itself last wrote.
    ///
    /// Trueflow writes exactly one `public.utf8-plain-text` item, which is
    /// indistinguishable by shape from a user-copied plain-text clipboard.
    /// Without this, the transcript left behind by the previous dictation is
    /// captured as user content worth restoring, and each run's clipboard
    /// handling depends on what the run before it left behind.
    last_written_change_count: AtomicI64,
}

pub fn open_external_https_url(value: &str) -> Result<(), InsertionError> {
    if !value.starts_with("https://") {
        return Err(InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            "refused to open a non-HTTPS external URL",
        ));
    }
    let url_string = NSString::from_str(value);
    let url = NSURL::URLWithString(&url_string).ok_or_else(|| {
        InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            "could not construct external URL",
        )
    })?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            "macOS rejected the external URL",
        ))
    }
}

pub fn open_microphone_settings() -> Result<(), InsertionError> {
    open_system_settings(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "Microphone",
    )
}

fn open_system_settings(value: &str, pane: &str) -> Result<(), InsertionError> {
    let url_string = NSString::from_str(value);
    let url = NSURL::URLWithString(&url_string).ok_or_else(|| {
        InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            format!("could not construct the {pane} settings URL"),
        )
    })?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            format!("macOS rejected the {pane} settings URL"),
        ))
    }
}

impl MacOsInsertionBackend {
    pub const fn new() -> Self {
        Self {
            // No real change count is negative, so nothing matches initially.
            last_written_change_count: AtomicI64::new(-1),
        }
    }

    /// True when the pasteboard still holds exactly what Trueflow wrote, meaning
    /// there is no user-owned content to preserve.
    fn holds_trueflow_transcript(&self, change_count: i64) -> bool {
        self.last_written_change_count.load(Ordering::Relaxed) == change_count
    }
}

impl Default for MacOsInsertionBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InsertionBackend for MacOsInsertionBackend {
    fn permission_status(&self) -> AccessibilityPermissionState {
        // A null system-wide element means the Accessibility service itself is
        // unavailable, which is distinct from the process simply being denied.
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return AccessibilityPermissionState::Unavailable;
        }
        unsafe { CFRelease(system) };
        // SAFETY: AXIsProcessTrusted is a side-effect-free process trust query.
        if unsafe { AXIsProcessTrusted() } != 0 {
            AccessibilityPermissionState::Granted
        } else {
            AccessibilityPermissionState::Denied
        }
    }

    fn open_accessibility_settings(&self) -> Result<(), InsertionError> {
        open_system_settings(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "Accessibility",
        )
    }

    fn resolve_target(&self) -> Result<TargetContext, InsertionError> {
        let application = frontmost_application()?;
        let permission = self.permission_status();
        let element = match focused_element_for_process(application.process_id) {
            Ok(element) => Some(element),
            Err(error) => {
                tracing::warn!(
                    subsystem = "insertion",
                    event = "focused_element_unavailable",
                    process_id = application.process_id,
                    application_name = application.name.as_deref(),
                    bundle_id = application.bundle_id.as_deref(),
                    error_code = error.code,
                    error_detail = %error,
                    ?permission,
                    "focused Accessibility element is unavailable; target remains eligible for bounded paste"
                );
                None
            }
        };
        let focused_element_resolved = element.is_some();
        let role_attribute = CFString::new("AXRole");
        let subrole_attribute = CFString::new("AXSubrole");
        let selected_text_attribute = CFString::new("AXSelectedText");
        let role = element.as_ref().and_then(|element| {
            copy_string_attribute(element.0, role_attribute.as_concrete_TypeRef())
        });
        let subrole = element.as_ref().and_then(|element| {
            copy_string_attribute(element.0, subrole_attribute.as_concrete_TypeRef())
        });
        let direct_insertion_supported = element.as_ref().is_some_and(|element| {
            is_attribute_settable(element.0, selected_text_attribute.as_concrete_TypeRef())
        });
        let secure = subrole.as_deref() == Some("AXSecureTextField")
            || role.as_deref() == Some("AXSecureTextField");
        let editability = classify_editability(role.as_deref(), direct_insertion_supported, secure);
        let trueflow = application.bundle_id.as_deref() == Some(TRUEFLOW_BUNDLE_ID);

        let (application_frontmost, application_has_focused_window) =
            application_focus_state(application.process_id);
        tracing::info!(
            subsystem = "insertion",
            event = "target_resolved",
            application_frontmost,
            application_has_focused_window,
            overlay_visible = crate::platform::macos::overlay_is_visible(),
            process_id = application.process_id,
            application_name = application.name.as_deref(),
            bundle_id = application.bundle_id.as_deref(),
            role = role.as_deref(),
            subrole = subrole.as_deref(),
            ?editability,
            focused_element_resolved,
            direct_insertion_supported,
            secure,
            trueflow,
            "resolved macOS insertion target"
        );

        Ok(TargetContext {
            process_id: application.process_id,
            application_name: application.name,
            bundle_id: application.bundle_id,
            role,
            subrole,
            editability,
            direct_insertion_supported,
            secure,
            trueflow,
        })
    }

    fn insert_directly(&self, target: &TargetContext, text: &str) -> Result<(), InsertionError> {
        validate_frontmost_process(target.process_id)?;
        let element = focused_element_for_process(target.process_id)?;
        let subrole_attribute = CFString::new("AXSubrole");
        let selected_text_attribute = CFString::new("AXSelectedText");
        if copy_string_attribute(element.0, subrole_attribute.as_concrete_TypeRef()).as_deref()
            == Some("AXSecureTextField")
        {
            return Err(InsertionError::new(
                InsertionErrorKind::SecureField,
                "focused element became a secure field before direct insertion",
            ));
        }
        if !is_attribute_settable(element.0, selected_text_attribute.as_concrete_TypeRef()) {
            return Err(InsertionError::new(
                InsertionErrorKind::SelectionUnavailable,
                "AXSelectedText is no longer settable",
            ));
        }

        // Editors with their own text engines—Monaco, Sublime, and Office among
        // them—expose a settable AXSelectedText that accepts the write and
        // discards it, reporting success while inserting nothing. Record the
        // caret position so the write can be verified rather than assumed.
        let range_before = selected_text_range(element.0);
        let inserted_utf16_len = text.encode_utf16().count();

        let text = CFString::new(text);
        // SAFETY: `element` is a retained live AXUIElement and `text` is a live
        // CFString. Setting AXSelectedText uses the target's native cursor or
        // selection semantics and never reads or overwrites its full value.
        let result = unsafe {
            AXUIElementSetAttributeValue(
                element.0,
                selected_text_attribute.as_concrete_TypeRef(),
                text.as_CFTypeRef(),
            )
        };
        if result != AX_SUCCESS {
            return Err(InsertionError::new(
                InsertionErrorKind::DirectInsertionFailed,
                format!("AXSelectedText write returned AXError {result}"),
            ));
        }

        let range_after = selected_text_range(element.0);
        if selection_proves_write_was_discarded(range_before, range_after) {
            warn!(
                subsystem = "insertion",
                event = "direct_insert_discarded",
                process_id = target.process_id,
                bundle_id = target.bundle_id.as_deref(),
                role = target.role.as_deref(),
                inserted_utf16_len,
                "AXSelectedText reported success but the caret did not move; falling back to paste"
            );
            return Err(InsertionError::new(
                InsertionErrorKind::DirectInsertionFailed,
                "AXSelectedText write was accepted but discarded by the target",
            ));
        }
        Ok(())
    }

    fn capture_clipboard(&self) -> Result<ClipboardSnapshot, InsertionError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        // SAFETY: NSPasteboardTypeString is an immutable AppKit framework constant.
        let plain_text = unsafe { pasteboard.stringForType(NSPasteboardTypeString) }
            .map(|value| value.to_string());
        let types = pasteboard.types();
        let was_empty = types.as_ref().is_none_or(|types| types.count() == 0);
        let only_plain_text = types.as_ref().is_some_and(|types| {
            types.count() == 1
                && types
                    .firstObject()
                    .is_some_and(|value| value.to_string() == "public.utf8-plain-text")
        });
        let change_count = i64::try_from(pasteboard.changeCount()).unwrap_or(-1);
        let owned_by_trueflow = self.holds_trueflow_transcript(change_count);
        let safely_restorable =
            clipboard_is_restorable(was_empty, only_plain_text, owned_by_trueflow);

        tracing::debug!(
            subsystem = "insertion",
            event = "clipboard_captured",
            change_count,
            last_written_change_count = self.last_written_change_count.load(Ordering::Relaxed),
            was_empty,
            only_plain_text,
            owned_by_trueflow,
            safely_restorable,
            "captured prior pasteboard state"
        );

        Ok(ClipboardSnapshot {
            plain_text,
            was_empty,
            safely_restorable,
        })
    }

    fn write_clipboard(&self, text: &str) -> Result<u64, InsertionError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let text = NSString::from_str(text);
        // SAFETY: NSPasteboardTypeString is an immutable AppKit framework constant.
        if !unsafe { pasteboard.setString_forType(&text, NSPasteboardTypeString) } {
            return Err(InsertionError::new(
                InsertionErrorKind::ClipboardWriteFailed,
                "NSPasteboard rejected the transcript string",
            ));
        }
        let change_count = u64::try_from(pasteboard.changeCount()).map_err(|_| {
            InsertionError::new(
                InsertionErrorKind::ClipboardWriteFailed,
                "NSPasteboard returned a negative change count",
            )
        })?;
        // Remember what we wrote so the next dictation does not mistake this
        // transcript for the user's own clipboard.
        self.last_written_change_count
            .store(i64::try_from(change_count).unwrap_or(-1), Ordering::Relaxed);
        Ok(change_count)
    }

    fn type_text(&self, target: &TargetContext, text: &str) -> Result<(), InsertionError> {
        validate_frontmost_process(target.process_id)?;
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            InsertionError::new(
                InsertionErrorKind::TypingDispatchFailed,
                "could not create a CoreGraphics event source",
            )
        })?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let chunks = utf16_chunks(&utf16, TYPING_CHUNK_UTF16);
        tracing::debug!(
            subsystem = "insertion",
            event = "typing_started",
            process_id = target.process_id,
            bundle_id = target.bundle_id.as_deref(),
            utf16_len = utf16.len(),
            chunks = chunks.len(),
            "posting the transcript as synthetic keyboard input"
        );
        for chunk in chunks {
            let key_down = CGEvent::new_keyboard_event(source.clone(), 0, true).map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::TypingDispatchFailed,
                    "could not create a typing key-down event",
                )
            })?;
            let key_up = CGEvent::new_keyboard_event(source.clone(), 0, false).map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::TypingDispatchFailed,
                    "could not create a typing key-up event",
                )
            })?;
            // SAFETY: the buffer is well-formed UTF-16 produced by `str`, split
            // only on non-surrogate boundaries, so no event carries half of a
            // surrogate pair.
            key_down.set_string_from_utf16_unchecked(chunk);
            key_up.set_string_from_utf16_unchecked(chunk);
            key_down.post(CGEventTapLocation::HID);
            key_up.post(CGEventTapLocation::HID);
            std::thread::sleep(TYPING_CHUNK_DELAY);
        }
        Ok(())
    }

    fn dispatch_paste(&self, expected_process_id: i32) -> Result<(), InsertionError> {
        validate_frontmost_process(expected_process_id)?;
        // The user may still be holding Option from the hold-to-talk shortcut.
        // Dispatching into that state posts the wrong chord, so settle first and
        // report the residual state instead of silently claiming a clean paste.
        let held_modifiers = wait_for_modifier_release();
        if held_modifiers != 0 {
            warn!(
                subsystem = "insertion",
                event = "paste_modifiers_held",
                held_modifiers,
                timeout_ms = PASTE_MODIFIER_CLEAR_TIMEOUT.as_millis(),
                "dispatching Command+V while physical modifiers are still down"
            );
        }
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            InsertionError::new(
                InsertionErrorKind::PasteDispatchFailed,
                "could not create a CoreGraphics event source",
            )
        })?;
        // Microsoft Word stops accepting the paste entirely when Command is
        // carried only as a flag on the V key pair, so the explicit Command
        // FlagsChanged events are required rather than incidental. This was
        // established by removing them and observing Word regress from
        // intermittent success to none, not by inference.
        let command_down = CGEvent::new_keyboard_event(source.clone(), KeyCode::COMMAND, true)
            .map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::PasteDispatchFailed,
                    "could not create the Command key-down event",
                )
            })?;
        let key_down =
            CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true).map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::PasteDispatchFailed,
                    "could not create the V key-down event",
                )
            })?;
        let key_up =
            CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, false).map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::PasteDispatchFailed,
                    "could not create the V key-up event",
                )
            })?;
        let command_up =
            CGEvent::new_keyboard_event(source, KeyCode::COMMAND, false).map_err(|_| {
                InsertionError::new(
                    InsertionErrorKind::PasteDispatchFailed,
                    "could not create the Command key-up event",
                )
            })?;
        // A modifier the user is still physically holding is preserved, so the
        // trailing FlagsChanged never claims keys were released that were not.
        let residual = CGEventFlags::from_bits_truncate(held_modifiers);
        let chord = residual | CGEventFlags::CGEventFlagCommand;
        command_down.set_type(CGEventType::FlagsChanged);
        command_down.set_flags(chord);
        key_down.set_flags(chord);
        key_up.set_flags(chord);
        command_up.set_type(CGEventType::FlagsChanged);
        command_up.set_flags(residual);
        tracing::debug!(
            subsystem = "insertion",
            event = "paste_chord_dispatched",
            held_modifiers,
            flags = chord.bits(),
            "posting the Command+V chord through the HID tap"
        );
        // The foreground PID was revalidated immediately above, but the event
        // itself must pass through the window server. Electron and custom
        // editor canvases may accept a process-directed event without routing
        // it to their focused responder.
        command_down.post(CGEventTapLocation::HID);
        std::thread::sleep(PASTE_MODIFIER_SETTLE_DELAY);
        key_down.post(CGEventTapLocation::HID);
        std::thread::sleep(PASTE_KEY_HOLD_DELAY);
        key_up.post(CGEventTapLocation::HID);
        std::thread::sleep(PASTE_MODIFIER_SETTLE_DELAY);
        command_up.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn restore_clipboard(
        &self,
        snapshot: &ClipboardSnapshot,
        expected_change_count: u64,
    ) -> Result<bool, InsertionError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        if u64::try_from(pasteboard.changeCount()).ok() != Some(expected_change_count) {
            return Ok(false);
        }
        pasteboard.clearContents();
        if snapshot.was_empty {
            return Ok(true);
        }
        let Some(previous) = snapshot.plain_text.as_deref() else {
            return Ok(false);
        };
        let previous = NSString::from_str(previous);
        // SAFETY: NSPasteboardTypeString is an immutable AppKit framework constant.
        if unsafe { pasteboard.setString_forType(&previous, NSPasteboardTypeString) } {
            Ok(true)
        } else {
            Err(InsertionError::new(
                InsertionErrorKind::ClipboardRestoreFailed,
                "NSPasteboard rejected the previous plain-text value",
            ))
        }
    }

    fn restoration_delay(&self) -> Duration {
        PASTEBOARD_RESTORE_DELAY
    }
}

impl RewriteBackend for MacOsInsertionBackend {
    fn capture_selection(&self) -> Result<SelectionSnapshot, RewriteError> {
        match self.permission_status() {
            AccessibilityPermissionState::Granted => {}
            AccessibilityPermissionState::Denied => {
                return Err(RewriteError::new(
                    RewriteErrorKind::AccessibilityPermissionDenied,
                    "AXIsProcessTrusted returned false",
                ));
            }
            AccessibilityPermissionState::Unknown | AccessibilityPermissionState::Unavailable => {
                return Err(RewriteError::new(
                    RewriteErrorKind::AccessibilityUnavailable,
                    "the macOS Accessibility service is unavailable",
                ));
            }
        }
        capture_current_selection()
    }

    fn revalidate_selection(&self, snapshot: &SelectionSnapshot) -> Result<(), RewriteError> {
        revalidated_element(snapshot).map(|_| ())
    }

    fn replace_directly(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> Result<DirectRewriteResult, RewriteError> {
        let element = revalidated_element(snapshot)?;
        let selected_text_attribute = CFString::new("AXSelectedText");
        if !is_attribute_settable(element.0, selected_text_attribute.as_concrete_TypeRef()) {
            return Ok(DirectRewriteResult::Unsupported);
        }

        let replacement_value = CFString::new(replacement);
        // SAFETY: the retained focused element and replacement CFString are
        // live for this synchronous AX call. The exact selection was
        // revalidated immediately above and is not cleared beforehand.
        let result = unsafe {
            AXUIElementSetAttributeValue(
                element.0,
                selected_text_attribute.as_concrete_TypeRef(),
                replacement_value.as_CFTypeRef(),
            )
        };
        if result != AX_SUCCESS {
            let range_after = selected_text_range(element.0).and_then(public_range);
            let selected_after =
                copy_string_attribute(element.0, selected_text_attribute.as_concrete_TypeRef());
            if range_after == Some(snapshot.range)
                && selected_after.as_deref() == Some(snapshot.selected_text.as_str())
            {
                return Ok(DirectRewriteResult::ProvenNoMutation);
            }
            return Err(RewriteError::new(
                RewriteErrorKind::DirectReplacementFailed,
                format!(
                    "AXSelectedText returned AXError {result} and the original selection could not be positively verified"
                ),
            ));
        }

        let expected_location = snapshot
            .range
            .location
            .saturating_add(u64::try_from(replacement.encode_utf16().count()).unwrap_or(u64::MAX));
        let range_after = selected_text_range(element.0).and_then(public_range);
        if range_after
            == Some(Utf16Range {
                location: expected_location,
                length: 0,
            })
        {
            return Ok(DirectRewriteResult::Applied);
        }

        let selected_after =
            copy_string_attribute(element.0, selected_text_attribute.as_concrete_TypeRef());
        if range_after == Some(snapshot.range)
            && selected_after.as_deref() == Some(snapshot.selected_text.as_str())
        {
            return Ok(DirectRewriteResult::ProvenNoMutation);
        }

        Err(RewriteError::new(
            RewriteErrorKind::DirectReplacementFailed,
            "AXSelectedText returned success but replacement could not be verified; no fallback was attempted to prevent duplication",
        ))
    }

    fn capture_clipboard(&self) -> Result<ClipboardSnapshot, RewriteError> {
        <Self as InsertionBackend>::capture_clipboard(self).map_err(rewrite_clipboard_error)
    }

    fn write_clipboard(&self, text: &str) -> Result<u64, RewriteError> {
        <Self as InsertionBackend>::write_clipboard(self, text).map_err(rewrite_clipboard_error)
    }

    fn dispatch_paste(&self, snapshot: &SelectionSnapshot) -> Result<(), RewriteError> {
        // This is the last possible synchronous validation before Command+V
        // reaches the target and performs the document mutation.
        revalidated_element(snapshot)?;
        <Self as InsertionBackend>::dispatch_paste(self, snapshot.process_id).map_err(|error| {
            RewriteError::new(RewriteErrorKind::PasteDispatchFailed, error.diagnostic)
        })
    }

    fn restore_clipboard(
        &self,
        snapshot: &ClipboardSnapshot,
        expected_change_count: u64,
    ) -> Result<bool, RewriteError> {
        <Self as InsertionBackend>::restore_clipboard(self, snapshot, expected_change_count)
            .map_err(|error| {
                RewriteError::new(RewriteErrorKind::ClipboardRestoreFailed, error.diagnostic)
            })
    }

    fn restoration_delay(&self) -> Duration {
        PASTEBOARD_RESTORE_DELAY
    }
}

/// Splits UTF-16 text into chunks that never end between a surrogate pair.
///
/// Splitting a surrogate pair across two events would deliver two replacement
/// characters instead of the intended character, silently corrupting emoji and
/// other astral-plane text in the user's transcript.
fn utf16_chunks(text: &[u16], limit: usize) -> Vec<&[u16]> {
    if text.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = usize::min(start.saturating_add(limit), text.len());
        // A high surrogate must keep its trailing low surrogate.
        if end < text.len() && (0xD800..=0xDBFF).contains(&text[end - 1]) {
            if end - 1 > start {
                // Defer the whole pair to the next chunk.
                end -= 1;
            } else {
                // The chunk is just the high surrogate, so take its pair with
                // it rather than emitting half a character. This is the only
                // case where a chunk may exceed the limit, and it exceeds it
                // by exactly one unit.
                end += 1;
            }
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

/// Reads a boolean Accessibility attribute, returning `None` when the target
/// does not publish it.
fn copy_bool_attribute(element: AXUIElementRef, attribute: CFStringRef) -> Option<bool> {
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: `element` is live, the attribute is an immutable framework
    // string, and the +1 reference received here is released below.
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut value) };
    if status != AX_SUCCESS || value.is_null() {
        return None;
    }
    // SAFETY: the value came from a Copy call, so this process owns it.
    let boolean = unsafe { CFBoolean::wrap_under_create_rule(value.cast()) };
    Some(boolean == CFBoolean::true_value())
}

/// Reports whether an Accessibility attribute is present at all.
fn attribute_is_present(element: AXUIElementRef, attribute: CFStringRef) -> bool {
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: `element` is live and any returned reference is released below.
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut value) };
    if status != AX_SUCCESS || value.is_null() {
        return false;
    }
    // SAFETY: the value came from a Copy call, so this process owns it.
    unsafe { CFRelease(value) };
    true
}

/// Collects the window and activation state of the target application.
///
/// Returns `(is_frontmost, has_focused_window)`. Both transports Trueflow has
/// tried fail intermittently while every other logged field is identical
/// between a success and a failure, so the state that decides whether synthetic
/// input reaches the focused responder must be recorded here.
fn application_focus_state(process_id: i32) -> (Option<bool>, bool) {
    // SAFETY: AXUIElementCreateApplication returns a retained element or null.
    let application = unsafe { AXUIElementCreateApplication(process_id) };
    if application.is_null() {
        return (None, false);
    }
    let application = OwnedAxElement(application);
    let frontmost = CFString::new("AXFrontmost");
    let focused_window = CFString::new("AXFocusedWindow");
    (
        copy_bool_attribute(application.0, frontmost.as_concrete_TypeRef()),
        attribute_is_present(application.0, focused_window.as_concrete_TypeRef()),
    )
}

/// Decides whether the prior pasteboard holds user content worth restoring.
///
/// Restoration exists to protect the user's clipboard. A pasteboard still
/// holding Trueflow's own transcript has nothing to protect, and restoring it
/// would overwrite the transcript the target application may not have read yet.
const fn clipboard_is_restorable(
    was_empty: bool,
    only_plain_text: bool,
    owned_by_trueflow: bool,
) -> bool {
    if owned_by_trueflow {
        return false;
    }
    was_empty || only_plain_text
}

/// Reads `AXSelectedTextRange`, returning `None` when the target does not
/// publish a usable caret position.
fn selected_text_range(element: AXUIElementRef) -> Option<AxRange> {
    let attribute = CFString::new("AXSelectedTextRange");
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: `element` is a retained live AXUIElement, the attribute is an
    // immutable framework string, and `value` receives a +1 reference that is
    // released below before returning.
    let status = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if status != AX_SUCCESS || value.is_null() {
        return None;
    }
    let mut range = AxRange {
        location: 0,
        length: 0,
    };
    // SAFETY: `value` is a live AXValue and `range` is a correctly sized,
    // correctly aligned CFRange that AXValueGetValue fills only when the value
    // actually carries a CFRange payload.
    let filled = unsafe {
        AXValueGetValue(
            value,
            AX_VALUE_CF_RANGE_TYPE,
            (&raw mut range).cast::<c_void>(),
        )
    };
    // SAFETY: `value` came from a Copy call, so this process owns the reference.
    unsafe { CFRelease(value) };
    (filled != 0).then_some(range)
}

/// Decides whether an `AX_SUCCESS` write was silently discarded.
///
/// This deliberately requires positive proof of failure: an unmoved caret that
/// both reads reported. An unreadable caret, or any movement at all, is treated
/// as inserted. A false negative here would send an already-inserted transcript
/// down the paste fallback and duplicate the user's text, which is far worse
/// than leaving an unverifiable target on the existing behavior.
const fn selection_proves_write_was_discarded(
    before: Option<AxRange>,
    after: Option<AxRange>,
) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            before.location == after.location && before.length == after.length
        }
        _ => false,
    }
}

/// Returns the Shift/Control/Option/Command bits that are still physically held.
const fn conflicting_modifiers(flags: u64) -> u64 {
    flags & CONFLICTING_MODIFIER_MASK
}

/// Polls the window server until no conflicting modifier is physically held, or
/// the bounded settle budget expires. Returns the modifiers still held, so the
/// caller can post an honest chord instead of waiting indefinitely.
fn wait_for_modifier_release() -> u64 {
    let deadline = Instant::now() + PASTE_MODIFIER_CLEAR_TIMEOUT;
    loop {
        // SAFETY: `CGEventSourceFlagsState` reads global window-server modifier
        // state, takes a plain enum discriminant, and returns a bitmask by
        // value. It borrows no memory from this process.
        let held =
            conflicting_modifiers(unsafe { CGEventSourceFlagsState(COMBINED_SESSION_STATE) });
        if held == 0 || Instant::now() >= deadline {
            return held;
        }
        std::thread::sleep(PASTE_MODIFIER_POLL_INTERVAL);
    }
}

fn classify_editability(
    role: Option<&str>,
    direct_insertion_supported: bool,
    secure: bool,
) -> TargetEditability {
    if secure {
        return TargetEditability::NotEditable;
    }
    if direct_insertion_supported
        || matches!(
            role,
            Some("AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField")
        )
    {
        return TargetEditability::Editable;
    }
    if matches!(
        role,
        Some(
            "AXApplication"
                | "AXButton"
                | "AXCheckBox"
                | "AXDisclosureTriangle"
                | "AXImage"
                | "AXLink"
                | "AXMenu"
                | "AXMenuBar"
                | "AXMenuItem"
                | "AXProgressIndicator"
                | "AXRadioButton"
                | "AXSlider"
                | "AXStaticText"
                | "AXTabGroup"
                | "AXToolbar"
                | "AXWindow"
        )
    ) {
        TargetEditability::NotEditable
    } else {
        // Electron, web, Office, and custom native editors frequently expose a
        // container or vendor-specific role even while their caret accepts
        // ordinary paste. Treat those roles as unknown and let the bounded,
        // PID-revalidated paste strategy decide instead of rejecting them.
        TargetEditability::Unknown
    }
}

struct ApplicationIdentity {
    process_id: i32,
    name: Option<String>,
    bundle_id: Option<String>,
}

fn frontmost_application() -> Result<ApplicationIdentity, InsertionError> {
    let application = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .ok_or_else(|| {
            InsertionError::new(
                InsertionErrorKind::TargetApplicationUnavailable,
                "NSWorkspace has no frontmost application",
            )
        })?;
    Ok(ApplicationIdentity {
        process_id: application.processIdentifier(),
        name: application.localizedName().map(|value| value.to_string()),
        bundle_id: application
            .bundleIdentifier()
            .map(|value| value.to_string()),
    })
}

fn validate_frontmost_process(expected_process_id: i32) -> Result<(), InsertionError> {
    let current = frontmost_application()?;
    if current.process_id == expected_process_id {
        Ok(())
    } else {
        Err(InsertionError::new(
            InsertionErrorKind::InsertionTargetChanged,
            format!(
                "frontmost process changed from {expected_process_id} to {}",
                current.process_id
            ),
        ))
    }
}

struct OwnedAxElement(AXUIElementRef);

impl Drop for OwnedAxElement {
    fn drop(&mut self) {
        // SAFETY: Copy/Create returned this retained Core Foundation object and
        // this guard releases it exactly once.
        unsafe { CFRelease(self.0) };
    }
}

fn focused_element_for_process(process_id: i32) -> Result<OwnedAxElement, InsertionError> {
    match focused_element_from_system() {
        Ok(element) => {
            tracing::debug!(
                subsystem = "insertion",
                event = "focused_element_resolved",
                process_id,
                lookup = "system_wide",
                "resolved focused Accessibility element"
            );
            Ok(element)
        }
        Err(system_error) => match focused_element_from_application(process_id) {
            Ok(element) => {
                tracing::info!(
                    subsystem = "insertion",
                    event = "focused_element_resolved",
                    process_id,
                    lookup = "application_scoped",
                    "resolved focused Accessibility element after system-wide lookup failed"
                );
                Ok(element)
            }
            Err(app_error) => {
                let manual_accessibility_enabled = enable_manual_accessibility(process_id);
                if manual_accessibility_enabled {
                    if let Ok(element) = focused_element_from_application(process_id) {
                        tracing::info!(
                            subsystem = "insertion",
                            event = "focused_element_resolved",
                            process_id,
                            lookup = "application_scoped_manual_accessibility",
                            "resolved focused Accessibility element after enabling the target's manual Accessibility tree"
                        );
                        return Ok(element);
                    }
                }
                Err(InsertionError::new(
                    InsertionErrorKind::FocusedElementUnavailable,
                    format!(
                        "system-wide focused element lookup failed ({system_error}); application-scoped lookup for PID {process_id} also failed ({app_error}); manual Accessibility enable accepted: {manual_accessibility_enabled}"
                    ),
                ))
            }
        },
    }
}

fn focused_element_from_system() -> Result<OwnedAxElement, InsertionError> {
    // SAFETY: The returned system-wide element follows the Create rule.
    let system = OwnedAxElement(unsafe { AXUIElementCreateSystemWide() });
    if system.0.is_null() {
        return Err(InsertionError::new(
            InsertionErrorKind::AccessibilityUnavailable,
            "AXUIElementCreateSystemWide returned null",
        ));
    }
    copy_focused_element(system.0, "system-wide")
}

fn focused_element_from_application(process_id: i32) -> Result<OwnedAxElement, InsertionError> {
    // SAFETY: The PID was obtained from NSWorkspace's frontmost application and
    // the returned application element follows the Create rule.
    let application = OwnedAxElement(unsafe { AXUIElementCreateApplication(process_id) });
    if application.0.is_null() {
        return Err(InsertionError::new(
            InsertionErrorKind::FocusedElementUnavailable,
            format!("AXUIElementCreateApplication returned null for PID {process_id}"),
        ));
    }
    copy_focused_element(application.0, "application-scoped")
}

fn enable_manual_accessibility(process_id: i32) -> bool {
    // Electron exposes this application-level attribute so an assistive client
    // can request creation of its otherwise-lazy Accessibility tree.
    let application = OwnedAxElement(unsafe { AXUIElementCreateApplication(process_id) });
    if application.0.is_null() {
        return false;
    }
    let attribute = CFString::new("AXManualAccessibility");
    let enabled = CFBoolean::true_value();
    // SAFETY: The application element and Core Foundation values are live for
    // the duration of the synchronous Accessibility call.
    unsafe {
        AXUIElementSetAttributeValue(
            application.0,
            attribute.as_concrete_TypeRef(),
            enabled.as_CFTypeRef(),
        ) == AX_SUCCESS
    }
}

fn copy_focused_element(
    source: AXUIElementRef,
    lookup_kind: &str,
) -> Result<OwnedAxElement, InsertionError> {
    let mut value: CFTypeRef = ptr::null();
    let focused_attribute = CFString::new("AXFocusedUIElement");
    // SAFETY: `source` is live, the attribute is an immutable framework
    // constant, and `value` is a valid out pointer initialized to null.
    let result = unsafe {
        AXUIElementCopyAttributeValue(source, focused_attribute.as_concrete_TypeRef(), &mut value)
    };
    if result != AX_SUCCESS || value.is_null() {
        return Err(InsertionError::new(
            InsertionErrorKind::FocusedElementUnavailable,
            format!("{lookup_kind} focused AX element lookup returned AXError {result}"),
        ));
    }
    Ok(OwnedAxElement(value.cast()))
}

fn is_attribute_settable(element: AXUIElementRef, attribute: CFStringRef) -> bool {
    let mut settable = 0_u8;
    // SAFETY: The element and framework attribute are live, and `settable` is
    // a valid out pointer for the duration of the call.
    (unsafe { AXUIElementIsAttributeSettable(element, attribute, &mut settable) == AX_SUCCESS })
        && settable != 0
}

fn copy_string_attribute(element: AXUIElementRef, attribute: CFStringRef) -> Option<String> {
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: The element and framework attribute are live, and `value` is a
    // valid initialized out pointer.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut value) };
    if result != AX_SUCCESS || value.is_null() {
        return None;
    }
    // SAFETY: The Copy call returned an owned CF object. Type-check before
    // wrapping it as CFString; otherwise release the unknown object directly.
    unsafe {
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return None;
        }
        Some(CFString::wrap_under_create_rule(value.cast()).to_string())
    }
}

fn capture_current_selection() -> Result<SelectionSnapshot, RewriteError> {
    let application = frontmost_application().map_err(|error| {
        RewriteError::new(
            RewriteErrorKind::TargetApplicationUnavailable,
            error.diagnostic,
        )
    })?;
    if application.bundle_id.as_deref() == Some(TRUEFLOW_BUNDLE_ID) {
        return Err(RewriteError::new(
            RewriteErrorKind::TrueflowTargetExcluded,
            "frontmost application is Trueflow",
        ));
    }
    let element = focused_element_for_process(application.process_id).map_err(|error| {
        RewriteError::new(
            RewriteErrorKind::FocusedElementUnavailable,
            error.diagnostic,
        )
    })?;
    selection_from_element(application, &element)
}

fn selection_from_element(
    application: ApplicationIdentity,
    element: &OwnedAxElement,
) -> Result<SelectionSnapshot, RewriteError> {
    let role_attribute = CFString::new("AXRole");
    let subrole_attribute = CFString::new("AXSubrole");
    let selected_text_attribute = CFString::new("AXSelectedText");
    let editable_attribute = CFString::new("AXEditable");
    let role = copy_string_attribute(element.0, role_attribute.as_concrete_TypeRef());
    let subrole = copy_string_attribute(element.0, subrole_attribute.as_concrete_TypeRef());
    let secure = role.as_deref() == Some("AXSecureTextField")
        || subrole.as_deref() == Some("AXSecureTextField");
    if secure {
        // Do not query AXSelectedText or any value-bearing attribute from a
        // detectable secure field.
        return Err(RewriteError::new(
            RewriteErrorKind::SecureField,
            "focused Accessibility element is secure",
        ));
    }

    let settable = is_attribute_settable(element.0, selected_text_attribute.as_concrete_TypeRef());
    let editable_attribute_value =
        copy_bool_attribute(element.0, editable_attribute.as_concrete_TypeRef());
    let selection_attributes_present =
        attribute_is_present(element.0, selected_text_attribute.as_concrete_TypeRef())
            && attribute_is_present(
                element.0,
                CFString::new("AXSelectedTextRange").as_concrete_TypeRef(),
            );
    let editable = settable
        || editable_attribute_value == Some(true)
        || (editable_attribute_value.is_none()
            && (matches!(
                role.as_deref(),
                Some("AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField")
            ) || selection_attributes_present));
    if !editable {
        return Err(RewriteError::new(
            RewriteErrorKind::TargetNotEditable,
            format!("focused role {role:?} is not editable"),
        ));
    }
    if let Some(count) = selected_text_ranges_count(element.0)
        && count > 1
    {
        return Err(RewriteError::new(
            RewriteErrorKind::MultipleSelectionsUnsupported,
            format!("AXSelectedTextRanges contains {count} ranges"),
        ));
    }
    let range = selected_text_range(element.0)
        .and_then(public_range)
        .ok_or_else(|| {
            RewriteError::new(
                RewriteErrorKind::SelectionUnsupported,
                "AXSelectedTextRange is unavailable or invalid",
            )
        })?;
    if range.length == 0 {
        return Err(RewriteError::new(
            RewriteErrorKind::SelectionMissing,
            "AXSelectedTextRange has zero length",
        ));
    }
    let selected_text =
        copy_string_attribute(element.0, selected_text_attribute.as_concrete_TypeRef())
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                RewriteError::new(
                    RewriteErrorKind::SelectionMissing,
                    "AXSelectedText is empty or unavailable",
                )
            })?;
    let utf16_length = u64::try_from(selected_text.encode_utf16().count()).unwrap_or(u64::MAX);
    if utf16_length != range.length {
        return Err(RewriteError::new(
            RewriteErrorKind::SelectionUnsupported,
            format!(
                "AX range length {} differs from selected UTF-16 length {utf16_length}",
                range.length
            ),
        ));
    }

    let identifier = element_identifier(element.0);
    // SAFETY: the retained AX element is a live CF object. CFHash returns a
    // stable semantic hash for its lifetime without retaining or mutating it.
    let fingerprint = unsafe { CFHash(element.0.cast()) };
    Ok(SelectionSnapshot {
        process_id: application.process_id,
        application_name: application.name,
        bundle_id: application.bundle_id,
        element_fingerprint: u64::try_from(fingerprint).unwrap_or(u64::MAX),
        element_identifier: identifier,
        role,
        subrole,
        range,
        selected_text,
    })
}

fn revalidated_element(snapshot: &SelectionSnapshot) -> Result<OwnedAxElement, RewriteError> {
    let application = frontmost_application().map_err(|error| {
        RewriteError::new(
            RewriteErrorKind::TargetApplicationUnavailable,
            error.diagnostic,
        )
    })?;
    if application.process_id != snapshot.process_id || application.bundle_id != snapshot.bundle_id
    {
        return Err(RewriteError::new(
            RewriteErrorKind::TargetApplicationChanged,
            format!(
                "expected PID {} and bundle {:?}, found PID {} and bundle {:?}",
                snapshot.process_id,
                snapshot.bundle_id,
                application.process_id,
                application.bundle_id
            ),
        ));
    }
    let element = focused_element_for_process(application.process_id).map_err(|error| {
        RewriteError::new(
            RewriteErrorKind::FocusedElementUnavailable,
            error.diagnostic,
        )
    })?;
    let role_attribute = CFString::new("AXRole");
    let subrole_attribute = CFString::new("AXSubrole");
    let role = copy_string_attribute(element.0, role_attribute.as_concrete_TypeRef());
    let subrole = copy_string_attribute(element.0, subrole_attribute.as_concrete_TypeRef());
    if role.as_deref() == Some("AXSecureTextField")
        || subrole.as_deref() == Some("AXSecureTextField")
    {
        return Err(RewriteError::new(
            RewriteErrorKind::SecureField,
            "focused element became secure during rewrite",
        ));
    }
    // SAFETY: the element is retained and live for this synchronous hash.
    let fingerprint = unsafe { CFHash(element.0.cast()) };
    let fingerprint = u64::try_from(fingerprint).unwrap_or(u64::MAX);
    let identifier = element_identifier(element.0);
    let identifiers_mismatch = snapshot.element_identifier.is_some()
        && identifier.is_some()
        && snapshot.element_identifier != identifier;
    if fingerprint != snapshot.element_fingerprint
        || identifiers_mismatch
        || role != snapshot.role
        || subrole != snapshot.subrole
    {
        return Err(RewriteError::new(
            RewriteErrorKind::TargetElementChanged,
            "focused Accessibility element identity no longer matches the captured element",
        ));
    }
    if let Some(count) = selected_text_ranges_count(element.0)
        && count > 1
    {
        return Err(RewriteError::new(
            RewriteErrorKind::MultipleSelectionsUnsupported,
            format!("AXSelectedTextRanges now contains {count} ranges"),
        ));
    }
    let range = selected_text_range(element.0).and_then(public_range);
    if range != Some(snapshot.range) {
        return Err(RewriteError::new(
            RewriteErrorKind::SelectionRangeChanged,
            format!("expected {:?}, found {range:?}", snapshot.range),
        ));
    }
    let selected_text_attribute = CFString::new("AXSelectedText");
    let selected_text =
        copy_string_attribute(element.0, selected_text_attribute.as_concrete_TypeRef());
    if selected_text.as_deref() != Some(snapshot.selected_text.as_str()) {
        return Err(RewriteError::new(
            RewriteErrorKind::SelectionTextChanged,
            "selected text no longer matches the captured source",
        ));
    }
    Ok(element)
}

fn public_range(range: AxRange) -> Option<Utf16Range> {
    Some(Utf16Range {
        location: u64::try_from(range.location).ok()?,
        length: u64::try_from(range.length).ok()?,
    })
}

fn element_identifier(element: AXUIElementRef) -> Option<String> {
    ["AXIdentifier", "AXDOMIdentifier"]
        .into_iter()
        .find_map(|name| {
            let attribute = CFString::new(name);
            copy_string_attribute(element, attribute.as_concrete_TypeRef())
        })
}

fn selected_text_ranges_count(element: AXUIElementRef) -> Option<isize> {
    let attribute = CFString::new("AXSelectedTextRanges");
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: element and attribute are live; a successful Copy returns an
    // owned CF object released before returning.
    let status = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if status != AX_SUCCESS || value.is_null() {
        return None;
    }
    // SAFETY: type ID queries borrow the live copied object.
    let is_array = unsafe { CFGetTypeID(value) == CFArrayGetTypeID() };
    let count = is_array.then(|| {
        // SAFETY: the type check above proves this object is a CFArray.
        unsafe { CFArrayGetCount(value.cast::<c_void>() as CFArrayRef) }
    });
    // SAFETY: value came from a Copy call and is released exactly once.
    unsafe { CFRelease(value) };
    count
}

fn rewrite_clipboard_error(error: InsertionError) -> RewriteError {
    RewriteError::new(RewriteErrorKind::ClipboardUnavailable, error.diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_editor_roles_remain_eligible_for_paste() {
        for role in [
            None,
            Some("AXGroup"),
            Some("AXWebArea"),
            Some("AXScrollArea"),
            Some("AXUnknown"),
        ] {
            assert_eq!(
                classify_editability(role, false, false),
                TargetEditability::Unknown
            );
        }
    }

    #[test]
    fn standard_text_and_direct_targets_are_editable() {
        assert_eq!(
            classify_editability(Some("AXTextArea"), false, false),
            TargetEditability::Editable
        );
        assert_eq!(
            classify_editability(Some("AXGroup"), true, false),
            TargetEditability::Editable
        );
    }

    #[test]
    fn secure_and_known_noneditable_targets_are_rejected() {
        assert_eq!(
            classify_editability(Some("AXTextArea"), true, true),
            TargetEditability::NotEditable
        );
        assert_eq!(
            classify_editability(Some("AXButton"), false, false),
            TargetEditability::NotEditable
        );
    }

    #[test]
    fn utf16_ranges_preserve_emoji_and_reject_negative_values() {
        assert_eq!(
            "Kael 👋".encode_utf16().count(),
            usize::try_from(
                public_range(AxRange {
                    location: 4,
                    length: 7,
                })
                .expect("valid range")
                .length
            )
            .expect("length")
        );
        assert!(
            public_range(AxRange {
                location: -1,
                length: 2,
            })
            .is_none()
        );
    }

    #[test]
    fn paste_timing_allows_async_consumers_to_read_before_restore() {
        assert!(PASTE_KEY_HOLD_DELAY >= Duration::from_millis(10));
        assert!(PASTEBOARD_RESTORE_DELAY >= Duration::from_secs(1));
    }

    #[test]
    fn paste_chord_preserves_a_genuinely_held_modifier() {
        // The chord must never claim a key the user is still pressing was
        // released, and must always carry Command.
        let residual = CGEventFlags::from_bits_truncate(conflicting_modifiers(0x0008_0000));
        let chord = residual | CGEventFlags::CGEventFlagCommand;
        assert!(chord.contains(CGEventFlags::CGEventFlagCommand));
        assert!(chord.contains(CGEventFlags::CGEventFlagAlternate));

        let clean = CGEventFlags::from_bits_truncate(conflicting_modifiers(0));
        assert_eq!(
            clean | CGEventFlags::CGEventFlagCommand,
            CGEventFlags::CGEventFlagCommand
        );
    }

    #[test]
    fn paste_chord_settle_intervals_remain_observable() {
        assert!(PASTE_MODIFIER_SETTLE_DELAY >= Duration::from_millis(5));
    }

    const fn range(location: isize, length: isize) -> AxRange {
        AxRange { location, length }
    }

    #[test]
    fn typing_chunks_never_split_a_surrogate_pair() {
        // A waving hand is one astral-plane character stored as two UTF-16
        // units. Splitting it across events would deliver two replacement
        // characters and corrupt the transcript.
        let text: Vec<u16> = "aaa👋bbb".encode_utf16().collect();
        for limit in 1..=text.len() {
            for chunk in utf16_chunks(&text, limit) {
                let last = chunk[chunk.len() - 1];
                assert!(
                    !(0xD800..=0xDBFF).contains(&last),
                    "chunk ended on a high surrogate at limit {limit}"
                );
            }
        }
    }

    #[test]
    fn typing_chunks_reassemble_to_the_original_text() {
        let original = "héllo 👋 second line\nwith punctuation — and emoji 🎉";
        let utf16: Vec<u16> = original.encode_utf16().collect();
        for limit in 1..=40 {
            let rejoined: Vec<u16> = utf16_chunks(&utf16, limit)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            assert_eq!(
                String::from_utf16(&rejoined).expect("valid UTF-16"),
                original,
                "text was corrupted at chunk limit {limit}"
            );
        }
    }

    #[test]
    fn typing_chunks_handle_degenerate_input() {
        assert!(utf16_chunks(&[], 10).is_empty());
        let text: Vec<u16> = "abc".encode_utf16().collect();
        assert!(utf16_chunks(&text, 0).is_empty());
    }

    #[test]
    fn typing_chunk_limit_stays_within_a_responsive_budget() {
        // A 400-character transcript should not take longer than the paste it
        // replaces by an amount the user would notice.
        let chunks = 400_usize.div_ceil(TYPING_CHUNK_UTF16);
        let worst_case = TYPING_CHUNK_DELAY * u32::try_from(chunks).expect("chunk count fits");
        assert!(worst_case <= Duration::from_millis(120));
    }

    #[test]
    fn our_own_transcript_is_never_restored_over() {
        // Trueflow writes a single public.utf8-plain-text item, so the previous
        // dictation's transcript is shaped exactly like a user-copied plain
        // string. Restoring it would clobber the transcript the target may not
        // have read yet, and would make each run depend on the run before it.
        assert!(!clipboard_is_restorable(false, true, true));
        assert!(!clipboard_is_restorable(true, false, true));
    }

    #[test]
    fn genuine_user_clipboard_is_still_restored() {
        assert!(clipboard_is_restorable(true, false, false));
        assert!(clipboard_is_restorable(false, true, false));
    }

    #[test]
    fn rich_clipboard_is_never_approximated() {
        assert!(!clipboard_is_restorable(false, false, false));
    }

    #[test]
    fn backend_starts_without_claiming_the_pasteboard() {
        let backend = MacOsInsertionBackend::new();
        assert!(!backend.holds_trueflow_transcript(0));
        assert!(!backend.holds_trueflow_transcript(1));
    }

    #[test]
    fn unmoved_caret_proves_the_write_was_discarded() {
        // Monaco, Sublime, and Office accept AXSelectedText and drop it.
        assert!(selection_proves_write_was_discarded(
            Some(range(42, 0)),
            Some(range(42, 0))
        ));
        assert!(selection_proves_write_was_discarded(
            Some(range(7, 13)),
            Some(range(7, 13))
        ));
    }

    #[test]
    fn any_caret_movement_counts_as_inserted() {
        // Native controls advance the caret past the inserted text and collapse
        // the selection. Only a completely unmoved caret may fall back.
        assert!(!selection_proves_write_was_discarded(
            Some(range(10, 0)),
            Some(range(15, 0))
        ));
        assert!(!selection_proves_write_was_discarded(
            Some(range(10, 5)),
            Some(range(10, 0))
        ));
    }

    #[test]
    fn unverifiable_targets_never_trigger_a_duplicate_paste() {
        // A missing caret read must not be mistaken for a discarded write, or an
        // already-inserted transcript would be pasted a second time.
        for (before, after) in [
            (None, Some(range(3, 0))),
            (Some(range(3, 0)), None),
            (None, None),
        ] {
            assert!(!selection_proves_write_was_discarded(before, after));
        }
    }

    #[test]
    fn shortcut_modifiers_are_detected_as_conflicting() {
        // Option is the modifier the hold-to-talk shortcut leaves down.
        assert_eq!(conflicting_modifiers(0x0008_0000), 0x0008_0000);
        assert_eq!(conflicting_modifiers(0x0002_0000), 0x0002_0000);
        assert_eq!(conflicting_modifiers(0x0004_0000), 0x0004_0000);
        assert_eq!(conflicting_modifiers(0x0010_0000), 0x0010_0000);
    }

    #[test]
    fn incidental_flags_do_not_block_paste_dispatch() {
        // Caps lock, fn, numeric pad, and non-coalesced bits do not change which
        // command Command+V resolves to, so they must not delay dispatch.
        for flags in [0x0001_0000, 0x0080_0000, 0x0020_0000, 0x0000_0100, 0] {
            assert_eq!(conflicting_modifiers(flags), 0);
        }
    }

    #[test]
    fn held_modifiers_are_preserved_in_the_dispatched_chord() {
        let residual = CGEventFlags::from_bits_truncate(conflicting_modifiers(0x0008_0000));
        assert!(residual.contains(CGEventFlags::CGEventFlagAlternate));
        assert!(
            (residual | CGEventFlags::CGEventFlagCommand)
                .contains(CGEventFlags::CGEventFlagCommand)
        );
    }

    #[test]
    fn modifier_settle_budget_stays_within_the_latency_target() {
        assert!(PASTE_MODIFIER_CLEAR_TIMEOUT <= Duration::from_millis(600));
        assert!(PASTE_MODIFIER_POLL_INTERVAL <= PASTE_MODIFIER_CLEAR_TIMEOUT);
    }
}
