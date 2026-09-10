mod coordinator;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

pub use coordinator::RewriteCoordinator;

use crate::insertion::ClipboardSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Dictation,
    SelectionRewrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RewriteStage {
    CapturingSelection,
    ListeningForInstruction,
    FinalizingInstruction,
    TransformingSelection,
    RevalidatingTarget,
    ReplacingSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Utf16Range {
    pub location: u64,
    pub length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionSnapshot {
    pub process_id: i32,
    pub application_name: Option<String>,
    pub bundle_id: Option<String>,
    pub element_fingerprint: u64,
    pub element_identifier: Option<String>,
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub range: Utf16Range,
    pub selected_text: String,
}

impl SelectionSnapshot {
    pub fn target_name(&self) -> Option<String> {
        self.application_name
            .clone()
            .or_else(|| self.bundle_id.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RewriteOutcomeKind {
    RewrittenDirectly,
    RewrittenViaPaste,
    #[expect(
        dead_code,
        reason = "reserved for explicit transformed-result clipboard recovery without conflating it with success"
    )]
    RewriteResultCopiedToClipboard,
    RewriteCancelled,
    RewriteFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteOutcome {
    pub kind: RewriteOutcomeKind,
    pub target_application: Option<String>,
    pub duration_ms: u64,
    pub clipboard_restored: bool,
    pub fallback_used: bool,
    pub error: Option<RewriteError>,
}

impl RewriteOutcome {
    pub fn automatically_rewritten(&self) -> bool {
        matches!(
            self.kind,
            RewriteOutcomeKind::RewrittenDirectly | RewriteOutcomeKind::RewrittenViaPaste
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RewriteErrorKind {
    AccessibilityPermissionDenied,
    AccessibilityUnavailable,
    TargetApplicationUnavailable,
    FocusedElementUnavailable,
    TrueflowTargetExcluded,
    SecureField,
    TargetNotEditable,
    SelectionMissing,
    MultipleSelectionsUnsupported,
    SelectionUnsupported,
    TargetApplicationChanged,
    TargetElementChanged,
    SelectionRangeChanged,
    SelectionTextChanged,
    DirectReplacementFailed,
    PasteDispatchFailed,
    ClipboardUnavailable,
    ClipboardRestoreFailed,
    Cancelled,
    RuntimeFailed,
}

#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize)]
#[error("{diagnostic}")]
#[serde(rename_all = "camelCase")]
pub struct RewriteError {
    pub kind: RewriteErrorKind,
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip)]
    pub diagnostic: String,
    pub retryable: bool,
}

impl RewriteError {
    pub fn new(kind: RewriteErrorKind, diagnostic: impl Into<String>) -> Self {
        let (code, message, retryable) = error_metadata(kind);
        Self {
            kind,
            code,
            message,
            diagnostic: diagnostic.into(),
            retryable,
        }
    }
}

const fn error_metadata(kind: RewriteErrorKind) -> (&'static str, &'static str, bool) {
    match kind {
        RewriteErrorKind::AccessibilityPermissionDenied => (
            "REWRITE_ACCESSIBILITY_PERMISSION_DENIED",
            "Enable Trueflow in System Settings → Privacy & Security → Accessibility, then try again.",
            true,
        ),
        RewriteErrorKind::AccessibilityUnavailable => (
            "REWRITE_ACCESSIBILITY_UNAVAILABLE",
            "Accessibility is unavailable. The selection was not changed.",
            true,
        ),
        RewriteErrorKind::TargetApplicationUnavailable => (
            "REWRITE_TARGET_APPLICATION_UNAVAILABLE",
            "No external writing application was available. The selection was not changed.",
            true,
        ),
        RewriteErrorKind::FocusedElementUnavailable => (
            "REWRITE_FOCUSED_ELEMENT_UNAVAILABLE",
            "Select text in an editable field first, then try again.",
            true,
        ),
        RewriteErrorKind::TrueflowTargetExcluded => (
            "REWRITE_TRUEFLOW_TARGET_EXCLUDED",
            "Select text in another application first, then try again.",
            true,
        ),
        RewriteErrorKind::SecureField => (
            "REWRITE_SECURE_FIELD",
            "Trueflow will not read or rewrite a secure field.",
            true,
        ),
        RewriteErrorKind::TargetNotEditable => (
            "REWRITE_TARGET_NOT_EDITABLE",
            "The focused item is not editable. Select text in an editable field first.",
            true,
        ),
        RewriteErrorKind::SelectionMissing => (
            "REWRITE_SELECTION_MISSING",
            "Select text first, then hold Option + Shift + Space and speak your instruction.",
            true,
        ),
        RewriteErrorKind::MultipleSelectionsUnsupported => (
            "REWRITE_MULTIPLE_SELECTIONS_UNSUPPORTED",
            "Rewrite supports one contiguous selection at a time. The text was not changed.",
            true,
        ),
        RewriteErrorKind::SelectionUnsupported => (
            "REWRITE_SELECTION_UNSUPPORTED",
            "This application does not expose the selection safely. The text was not changed.",
            true,
        ),
        RewriteErrorKind::TargetApplicationChanged
        | RewriteErrorKind::TargetElementChanged
        | RewriteErrorKind::SelectionRangeChanged
        | RewriteErrorKind::SelectionTextChanged => (
            "REWRITE_TARGET_CHANGED",
            "The selection changed before replacement. Nothing was changed; try again.",
            true,
        ),
        RewriteErrorKind::DirectReplacementFailed => (
            "REWRITE_DIRECT_REPLACEMENT_FAILED",
            "Trueflow could not safely replace the selection. The original text was preserved.",
            true,
        ),
        RewriteErrorKind::PasteDispatchFailed => (
            "REWRITE_PASTE_FAILED",
            "Trueflow could not complete the replacement. The original text was preserved.",
            true,
        ),
        RewriteErrorKind::ClipboardUnavailable => (
            "REWRITE_CLIPBOARD_UNAVAILABLE",
            "Trueflow could not preserve the clipboard for a safe replacement. The original text was preserved.",
            true,
        ),
        RewriteErrorKind::ClipboardRestoreFailed => (
            "REWRITE_CLIPBOARD_RESTORE_FAILED",
            "The selection was rewritten, but the previous clipboard could not be restored.",
            true,
        ),
        RewriteErrorKind::Cancelled => (
            "REWRITE_CANCELLED",
            "Rewrite was cancelled. The original text was preserved.",
            true,
        ),
        RewriteErrorKind::RuntimeFailed => (
            "REWRITE_RUNTIME_FAILED",
            "Rewrite stopped unexpectedly. The original text was preserved.",
            true,
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectRewriteResult {
    Applied,
    Unsupported,
    ProvenNoMutation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewritePipelineEvent {
    RevalidationStarted,
    RevalidationSucceeded,
    RevalidationFailed,
    ReplaceRequested,
    ReplaceCompleted,
}

pub trait RewriteObserver: Send + Sync {
    fn event(&self, event: RewritePipelineEvent);
}

pub trait RewriteBackend: Send + Sync {
    fn capture_selection(&self) -> Result<SelectionSnapshot, RewriteError>;
    fn revalidate_selection(&self, snapshot: &SelectionSnapshot) -> Result<(), RewriteError>;
    fn replace_directly(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> Result<DirectRewriteResult, RewriteError>;
    fn capture_clipboard(&self) -> Result<ClipboardSnapshot, RewriteError>;
    fn write_clipboard(&self, text: &str) -> Result<u64, RewriteError>;
    fn dispatch_paste(&self, snapshot: &SelectionSnapshot) -> Result<(), RewriteError>;
    fn restore_clipboard(
        &self,
        snapshot: &ClipboardSnapshot,
        expected_change_count: u64,
    ) -> Result<bool, RewriteError>;
    fn restoration_delay(&self) -> Duration;
}

#[derive(Clone, Default)]
pub struct RewriteControl {
    cancelled: Arc<AtomicBool>,
}

impl RewriteControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[async_trait]
pub trait SelectionRewriter: Send + Sync {
    fn capture_selection(&self) -> Result<SelectionSnapshot, RewriteError>;

    async fn replace(
        &self,
        snapshot: SelectionSnapshot,
        replacement: String,
        control: RewriteControl,
        observer: Arc<dyn RewriteObserver>,
    ) -> RewriteOutcome;
}
