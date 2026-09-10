mod coordinator;
mod error;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::Serialize;

pub use coordinator::InsertionCoordinator;
pub use error::{InsertionError, InsertionErrorKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessibilityPermissionState {
    #[default]
    Unknown,
    Granted,
    Denied,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionOutcomeKind {
    InsertedDirectly,
    InsertedByTyping,
    InsertedViaPaste,
    CopiedToClipboard,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionStrategy {
    DirectAccessibility,
    SyntheticTyping,
    Paste,
    ClipboardOnly,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertionOutcome {
    pub kind: InsertionOutcomeKind,
    pub strategy: InsertionStrategy,
    pub target_application: Option<String>,
    pub duration_ms: u64,
    pub clipboard_restored: bool,
    pub fallback_used: bool,
    pub degraded_transcript: bool,
    pub error: Option<InsertionError>,
}

impl InsertionOutcome {
    pub fn automatically_inserted(&self) -> bool {
        matches!(
            self.kind,
            InsertionOutcomeKind::InsertedDirectly
                | InsertionOutcomeKind::InsertedByTyping
                | InsertionOutcomeKind::InsertedViaPaste
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsertionContext {
    pub operation_id: String,
    pub degraded_transcript: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetEditability {
    Editable,
    Unknown,
    NotEditable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContext {
    pub process_id: i32,
    pub application_name: Option<String>,
    pub bundle_id: Option<String>,
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub editability: TargetEditability,
    pub direct_insertion_supported: bool,
    pub secure: bool,
    pub trueflow: bool,
}

impl TargetContext {
    pub fn display_name(&self) -> Option<String> {
        self.application_name
            .clone()
            .or_else(|| self.bundle_id.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardSnapshot {
    pub plain_text: Option<String>,
    pub was_empty: bool,
    pub safely_restorable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertionPipelineEvent {
    TargetResolved,
    DirectInsertStarted,
    DirectInsertCompleted,
    TypingStarted,
    TypingCompleted,
    PasteFallbackStarted,
    PasteDispatched,
    ClipboardRestoreStarted,
    ClipboardRestoreCompleted,
    ClipboardOnlyFallback,
}

pub trait InsertionObserver: Send + Sync {
    fn event(&self, event: InsertionPipelineEvent);
}

pub trait InsertionBackend: Send + Sync {
    fn permission_status(&self) -> AccessibilityPermissionState;
    fn open_accessibility_settings(&self) -> Result<(), InsertionError>;
    fn resolve_target(&self) -> Result<TargetContext, InsertionError>;
    fn insert_directly(&self, target: &TargetContext, text: &str) -> Result<(), InsertionError>;
    /// Posts the text as synthetic keyboard input, using neither the pasteboard
    /// nor a Command+V chord.
    fn type_text(&self, target: &TargetContext, text: &str) -> Result<(), InsertionError>;
    fn capture_clipboard(&self) -> Result<ClipboardSnapshot, InsertionError>;
    fn write_clipboard(&self, text: &str) -> Result<u64, InsertionError>;
    fn dispatch_paste(&self, expected_process_id: i32) -> Result<(), InsertionError>;
    fn restore_clipboard(
        &self,
        snapshot: &ClipboardSnapshot,
        expected_change_count: u64,
    ) -> Result<bool, InsertionError>;
    fn restoration_delay(&self) -> Duration;
}

#[async_trait]
pub trait TextInserter: Send + Sync {
    fn permission_status(&self) -> AccessibilityPermissionState;
    fn open_accessibility_settings(&self) -> Result<(), InsertionError>;
    async fn insert(
        &self,
        context: InsertionContext,
        text: String,
        observer: Arc<dyn InsertionObserver>,
    ) -> InsertionOutcome;
}
