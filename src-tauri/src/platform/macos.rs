use std::{sync::mpsc::sync_channel, time::Duration};

use block2::RcBlock;
use objc2::{MainThreadMarker, runtime::Bool};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSFloatingWindowLevel, NSScreen, NSWindow,
    NSWindowCollectionBehavior,
};
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{ActivationPolicy, AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use tracing::{debug, info, warn};

/// Whether the overlay window is currently ordered on screen.
///
/// Insertion needs this to record the window state that was in effect when
/// synthetic input was posted into another application. Trueflow's own window
/// activity is the most plausible remaining influence on whether that input
/// reaches the target's focused responder, and it is invisible in the insertion
/// logs without this.
static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Reports whether the overlay was on screen, for insertion diagnostics.
pub fn overlay_is_visible() -> bool {
    OVERLAY_VISIBLE.load(Ordering::Relaxed)
}

/// Records an overlay hide performed through Tauri rather than AppKit.
pub fn note_overlay_hidden() {
    OVERLAY_VISIBLE.store(false, Ordering::Relaxed);
}

use crate::{
    audio::{MicrophonePermission, MicrophonePermissionState},
    dictation::DictationError,
};

const PERMISSION_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const OVERLAY_BOTTOM_MARGIN: f64 = 72.0;

pub struct MacOsMicrophonePermission;

pub fn create_and_configure_overlay_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    app.set_activation_policy(ActivationPolicy::Accessory)?;
    let window = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("index.html?window=overlay".into()),
    )
    .title("Trueflow Dictation")
    .inner_size(340.0, 112.0)
    .resizable(false)
    .fullscreen(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .center()
    .build()?;
    configure_overlay_window(&window)?;
    Ok(window)
}

fn configure_overlay_window(window: &WebviewWindow) -> Result<(), tauri::Error> {
    let ns_window = window.ns_window()?.cast::<NSWindow>();
    let supports_all_applications = objc2::available!(macos = 15.0);
    let activation_policy = MainThreadMarker::new()
        .map(NSApplication::sharedApplication)
        .map(|application| application.activationPolicy());

    // SAFETY: Tauri returns the NSWindow owned by this live WebviewWindow. This
    // runs during application setup on the AppKit main thread, and the pointer
    // is borrowed only for these synchronous property updates.
    unsafe {
        let ns_window = &*ns_window;
        let behavior =
            overlay_collection_behavior(ns_window.collectionBehavior(), supports_all_applications);
        ns_window.setCollectionBehavior(behavior);
        ns_window.setLevel(NSFloatingWindowLevel);
        ns_window.setHidesOnDeactivate(false);

        info!(
            subsystem = "overlay",
            event = "native_window_configured",
            collection_behavior = behavior.bits(),
            window_level = NSFloatingWindowLevel,
            can_join_all_applications = supports_all_applications,
            activation_policy = ?activation_policy,
            accessory_policy = activation_policy == Some(NSApplicationActivationPolicy::Accessory),
            "configured native macOS overlay behavior"
        );
    }
    Ok(())
}

pub fn show_overlay_without_activation(
    window: &WebviewWindow,
    operation_id: Option<&str>,
) -> Result<(), tauri::Error> {
    let operation_id = operation_id.unwrap_or("none").to_owned();
    info!(
        operation_id,
        subsystem = "overlay",
        event = "overlay_show_requested",
        "native overlay show requested"
    );

    let window = window.clone();
    window.clone().run_on_main_thread(move || {
        let main_thread = match MainThreadMarker::new() {
            Some(marker) => marker,
            None => {
                warn!(
                    operation_id,
                    subsystem = "overlay",
                    event = "overlay_show_failed",
                    error_code = "OVERLAY_NOT_ON_MAIN_THREAD",
                    "overlay show did not run on the AppKit main thread"
                );
                return;
            }
        };
        let ns_window = match window.ns_window() {
            Ok(ns_window) => ns_window.cast::<NSWindow>(),
            Err(error) => {
                warn!(
                    operation_id,
                    subsystem = "overlay",
                    event = "overlay_show_failed",
                    error_code = "OVERLAY_NATIVE_WINDOW_UNAVAILABLE",
                    error = %error,
                    "could not show the overlay through AppKit"
                );
                return;
            }
        };

        // SAFETY: The pointer was obtained from the live Tauri window on the
        // AppKit main thread and is used only for synchronous messages here.
        unsafe {
            let ns_window = &*ns_window;
            let target_screen = NSScreen::mainScreen(main_thread).or_else(|| ns_window.screen());
            let screen_name = target_screen
                .as_ref()
                .map(|screen| screen.localizedName().to_string())
                .unwrap_or_else(|| "unavailable".to_owned());

            let desired_origin = target_screen.as_ref().map(|screen| {
                let screen_frame = screen.visibleFrame();
                let window_frame = ns_window.frame();
                let (x, y) = overlay_origin(
                    screen_frame.origin.x,
                    screen_frame.origin.y,
                    screen_frame.size.width,
                    screen_frame.size.height,
                    window_frame.size.width,
                    window_frame.size.height,
                );
                let mut origin = screen_frame.origin;
                origin.x = x;
                origin.y = y;
                origin
            });

            // Re-ordering an already-correctly-placed overlay is a window-server
            // operation with no visible effect. Every dictation state change
            // published one, eight per dictation, the last a few milliseconds
            // before synthetic input is posted into the target application.
            // Skipping the redundant ones keeps window-server activity away from
            // the insertion; placement still happens whenever the overlay is
            // hidden or the focused screen changed.
            let current_origin = ns_window.frame().origin;
            let already_placed = ns_window.isVisible()
                && ns_window.isOnActiveSpace()
                && desired_origin.is_none_or(|origin| {
                    origin_is_current(current_origin.x, current_origin.y, origin.x, origin.y)
                });
            if already_placed {
                OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
                debug!(
                    operation_id,
                    subsystem = "overlay",
                    event = "overlay_show_skipped",
                    "overlay is already visible and correctly placed"
                );
                return;
            }

            if let Some(origin) = desired_origin {
                ns_window.setFrameOrigin(origin);
            }
            ns_window.orderFrontRegardless();
            OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
            info!(
                operation_id,
                subsystem = "overlay",
                event = "overlay_show_completed",
                overlay_is_visible = ns_window.isVisible(),
                overlay_on_active_space = ns_window.isOnActiveSpace(),
                overlay_is_key = ns_window.isKeyWindow(),
                overlay_is_main = ns_window.isMainWindow(),
                screen = screen_name,
                "native overlay show completed"
            );
        }
    })
}

/// True when the overlay already sits at the requested origin, within a
/// sub-pixel tolerance, so repositioning it would be a no-op.
fn origin_is_current(current_x: f64, current_y: f64, desired_x: f64, desired_y: f64) -> bool {
    (current_x - desired_x).abs() < 0.5 && (current_y - desired_y).abs() < 0.5
}

fn overlay_origin(
    screen_x: f64,
    screen_y: f64,
    screen_width: f64,
    screen_height: f64,
    window_width: f64,
    window_height: f64,
) -> (f64, f64) {
    let x = screen_x + ((screen_width - window_width) / 2.0).max(0.0);
    let highest_y = screen_y + (screen_height - window_height).max(0.0);
    let y = (screen_y + OVERLAY_BOTTOM_MARGIN).min(highest_y);
    (x, y)
}

fn overlay_collection_behavior(
    existing: NSWindowCollectionBehavior,
    supports_all_applications: bool,
) -> NSWindowCollectionBehavior {
    let mut behavior = existing;
    behavior.remove(
        NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenPrimary
            | NSWindowCollectionBehavior::FullScreenNone
            | NSWindowCollectionBehavior::Primary
            | NSWindowCollectionBehavior::Auxiliary,
    );
    behavior.insert(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    if supports_all_applications {
        behavior.insert(NSWindowCollectionBehavior::CanJoinAllApplications);
    } else {
        behavior.remove(NSWindowCollectionBehavior::CanJoinAllApplications);
    }
    behavior
}

impl MicrophonePermission for MacOsMicrophonePermission {
    fn status(&self) -> MicrophonePermissionState {
        let Some(media_type) = audio_media_type() else {
            return MicrophonePermissionState::Unavailable;
        };

        // SAFETY: `media_type` is Apple's static AVMediaTypeAudio value, which
        // is the documented valid input for this class method on macOS.
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
        map_authorization_status(status)
    }

    fn request(&self) -> Result<MicrophonePermissionState, DictationError> {
        let media_type =
            audio_media_type().ok_or(DictationError::MicrophonePermissionUnavailable)?;
        let (sender, receiver) = sync_channel(1);
        let completion: RcBlock<dyn Fn(Bool)> = RcBlock::new(move |granted: Bool| {
            let _ = sender.try_send(granted.as_bool());
        });

        // SAFETY: `media_type` is AVMediaTypeAudio and `completion` is a
        // heap-backed Objective-C block kept alive by AVFoundation until its
        // asynchronous completion callback executes.
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &completion);
        }

        let granted = receiver
            .recv_timeout(PERMISSION_REQUEST_TIMEOUT)
            .map_err(|_| DictationError::MicrophonePermissionTimeout)?;
        Ok(if granted {
            MicrophonePermissionState::Granted
        } else {
            match self.status() {
                MicrophonePermissionState::Restricted => MicrophonePermissionState::Restricted,
                _ => MicrophonePermissionState::Denied,
            }
        })
    }
}

fn audio_media_type() -> Option<&'static AVMediaType> {
    // SAFETY: This reads an immutable AVFoundation framework constant. The
    // Option accounts for weak-linking on unsupported Apple platforms.
    unsafe { AVMediaTypeAudio }
}

fn map_authorization_status(status: AVAuthorizationStatus) -> MicrophonePermissionState {
    if status == AVAuthorizationStatus::NotDetermined {
        MicrophonePermissionState::NotDetermined
    } else if status == AVAuthorizationStatus::Authorized {
        MicrophonePermissionState::Granted
    } else if status == AVAuthorizationStatus::Denied {
        MicrophonePermissionState::Denied
    } else if status == AVAuthorizationStatus::Restricted {
        MicrophonePermissionState::Restricted
    } else {
        MicrophonePermissionState::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use objc2_app_kit::NSWindowCollectionBehavior;
    use objc2_av_foundation::AVAuthorizationStatus;

    use super::{
        MicrophonePermissionState, map_authorization_status, origin_is_current,
        overlay_collection_behavior, overlay_origin,
    };

    #[test]
    fn maps_all_documented_authorization_states() {
        assert_eq!(
            map_authorization_status(AVAuthorizationStatus::NotDetermined),
            MicrophonePermissionState::NotDetermined
        );
        assert_eq!(
            map_authorization_status(AVAuthorizationStatus::Authorized),
            MicrophonePermissionState::Granted
        );
        assert_eq!(
            map_authorization_status(AVAuthorizationStatus::Denied),
            MicrophonePermissionState::Denied
        );
        assert_eq!(
            map_authorization_status(AVAuthorizationStatus::Restricted),
            MicrophonePermissionState::Restricted
        );
        assert_eq!(
            map_authorization_status(AVAuthorizationStatus(99)),
            MicrophonePermissionState::Unavailable
        );
    }

    #[test]
    fn legacy_overlay_joins_spaces_as_full_screen_auxiliary() {
        let existing = NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenPrimary
            | NSWindowCollectionBehavior::Transient;
        let behavior = overlay_collection_behavior(existing, false);

        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(behavior.contains(NSWindowCollectionBehavior::Transient));
        assert!(!behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));
        assert!(!behavior.contains(NSWindowCollectionBehavior::FullScreenPrimary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::CanJoinAllApplications));
    }

    #[test]
    fn modern_overlay_can_join_other_applications() {
        let existing =
            NSWindowCollectionBehavior::Auxiliary | NSWindowCollectionBehavior::FullScreenNone;
        let behavior = overlay_collection_behavior(existing, true);

        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllApplications));
        assert!(!behavior.contains(NSWindowCollectionBehavior::Auxiliary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::FullScreenNone));
    }

    #[test]
    fn an_already_placed_overlay_is_not_reordered() {
        // Redundant ordering puts window-server activity microseconds before
        // synthetic input is posted into the target application.
        assert!(origin_is_current(100.0, 200.0, 100.0, 200.0));
        assert!(origin_is_current(100.2, 199.8, 100.0, 200.0));
    }

    #[test]
    fn a_moved_overlay_is_still_repositioned() {
        // Placement must still follow the focused application across screens.
        assert!(!origin_is_current(100.0, 200.0, 900.0, 200.0));
        assert!(!origin_is_current(100.0, 200.0, 100.0, -400.0));
    }

    #[test]
    fn overlay_is_positioned_on_the_active_screen() {
        assert_eq!(
            overlay_origin(1920.0, -120.0, 2560.0, 1440.0, 340.0, 112.0),
            (3030.0, -48.0)
        );
    }

    #[test]
    fn overlay_position_stays_inside_a_small_screen() {
        assert_eq!(
            overlay_origin(-800.0, 0.0, 300.0, 100.0, 340.0, 112.0),
            (-800.0, 0.0)
        );
    }
}
