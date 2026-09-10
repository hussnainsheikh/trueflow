use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionErrorKind {
    AccessibilityPermissionDenied,
    AccessibilityUnavailable,
    TargetApplicationUnavailable,
    FocusedElementUnavailable,
    TargetNotEditable,
    #[expect(
        dead_code,
        reason = "part of the normalized Milestone 3 platform error taxonomy"
    )]
    UnsupportedTextElement,
    SelectionUnavailable,
    DirectInsertionFailed,
    TypingDispatchFailed,
    PasteDispatchFailed,
    #[expect(
        dead_code,
        reason = "part of the normalized Milestone 3 platform error taxonomy"
    )]
    ClipboardReadFailed,
    ClipboardWriteFailed,
    ClipboardRestoreFailed,
    InsertionTargetChanged,
    #[expect(
        dead_code,
        reason = "part of the normalized Milestone 3 platform error taxonomy"
    )]
    InsertionCancelled,
    SecureField,
    TrueflowTargetExcluded,
    RuntimeFailed,
}

#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize)]
#[error("{diagnostic}")]
#[serde(rename_all = "camelCase")]
pub struct InsertionError {
    pub kind: InsertionErrorKind,
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip)]
    pub diagnostic: String,
    pub retryable: bool,
    pub transcript_recoverable: bool,
    pub fallback_eligible: bool,
}

impl InsertionError {
    pub fn new(kind: InsertionErrorKind, diagnostic: impl Into<String>) -> Self {
        let (code, message, retryable, fallback_eligible) = metadata(kind);
        Self {
            kind,
            code,
            message,
            diagnostic: diagnostic.into(),
            retryable,
            transcript_recoverable: true,
            fallback_eligible,
        }
    }
}

const fn metadata(kind: InsertionErrorKind) -> (&'static str, &'static str, bool, bool) {
    match kind {
        InsertionErrorKind::AccessibilityPermissionDenied => (
            "ACCESSIBILITY_PERMISSION_DENIED",
            "Enable Trueflow in System Settings → Privacy & Security → Accessibility. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::AccessibilityUnavailable => (
            "ACCESSIBILITY_UNAVAILABLE",
            "Accessibility is unavailable. Text was preserved for recovery.",
            true,
            true,
        ),
        InsertionErrorKind::TargetApplicationUnavailable => (
            "INSERTION_TARGET_APPLICATION_UNAVAILABLE",
            "No target application was available. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::FocusedElementUnavailable => (
            "INSERTION_FOCUSED_ELEMENT_UNAVAILABLE",
            "No editable field was focused. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::TargetNotEditable => (
            "INSERTION_TARGET_NOT_EDITABLE",
            "The focused item is not editable. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::UnsupportedTextElement => (
            "INSERTION_TEXT_ELEMENT_UNSUPPORTED",
            "This text field does not support direct insertion.",
            true,
            true,
        ),
        InsertionErrorKind::SelectionUnavailable => (
            "INSERTION_SELECTION_UNAVAILABLE",
            "The current text selection could not be used safely.",
            true,
            true,
        ),
        InsertionErrorKind::DirectInsertionFailed => (
            "DIRECT_INSERTION_FAILED",
            "Direct insertion failed; Trueflow tried a safe fallback.",
            true,
            true,
        ),
        InsertionErrorKind::TypingDispatchFailed => (
            "TYPING_DISPATCH_FAILED",
            "Couldn't insert automatically — text copied to clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::PasteDispatchFailed => (
            "PASTE_DISPATCH_FAILED",
            "Couldn't insert automatically — text copied to clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::ClipboardReadFailed => (
            "CLIPBOARD_READ_FAILED",
            "The clipboard could not be preserved before insertion.",
            true,
            true,
        ),
        InsertionErrorKind::ClipboardWriteFailed => (
            "CLIPBOARD_WRITE_FAILED",
            "Automatic insertion and clipboard recovery failed. The transcript remains available in Trueflow.",
            true,
            false,
        ),
        InsertionErrorKind::ClipboardRestoreFailed => (
            "CLIPBOARD_RESTORE_FAILED",
            "Text was inserted, but the previous clipboard could not be restored.",
            true,
            false,
        ),
        InsertionErrorKind::InsertionTargetChanged => (
            "INSERTION_TARGET_CHANGED",
            "The focused application changed before insertion. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::InsertionCancelled => (
            "INSERTION_CANCELLED",
            "Insertion was cancelled. The transcript remains available in Trueflow.",
            true,
            false,
        ),
        InsertionErrorKind::SecureField => (
            "INSERTION_SECURE_FIELD",
            "Trueflow will not insert or copy dictated text while a secure field is focused.",
            true,
            false,
        ),
        InsertionErrorKind::TrueflowTargetExcluded => (
            "INSERTION_TRUEFLOW_TARGET_EXCLUDED",
            "Trueflow's dictation overlay cannot be an insertion target. Text was copied to the clipboard.",
            true,
            true,
        ),
        InsertionErrorKind::RuntimeFailed => (
            "INSERTION_RUNTIME_FAILED",
            "Insertion stopped unexpectedly. The transcript remains available in Trueflow.",
            true,
            false,
        ),
    }
}
