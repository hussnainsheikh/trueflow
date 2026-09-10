//! Insertion transport probe.
//!
//! Determines empirically which synthetic-insertion transport a given macOS
//! application actually accepts. Successive Trueflow builds changed the paste
//! chord based on inference rather than measurement; this example replaces that
//! inference with an observation the tester can read directly off the screen.
//!
//! Each variant deposits a uniquely labelled marker. Whichever markers appear in
//! the focused application identify the transports that work there.
//!
//! ```bash
//! cargo run --manifest-path src-tauri/Cargo.toml --example insertion_probe
//! ```
//!
//! The probe posts synthetic events, so the process running it must be trusted
//! for Accessibility. When launched from a terminal that trust belongs to the
//! terminal application, not to Trueflow.

use std::{thread::sleep, time::Duration};

use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, KeyCode},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::NSString;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

const FOCUS_COUNTDOWN: u64 = 6;
const SETTLE_BETWEEN_VARIANTS: Duration = Duration::from_millis(2_500);
const KEY_HOLD: Duration = Duration::from_millis(12);
const MODIFIER_SETTLE: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
enum Transport {
    /// Command carried as a flag on the V key pair only.
    FlaggedKeyPair {
        tap: CGEventTapLocation,
        source: CGEventSourceStateID,
    },
    /// Explicit Command FlagsChanged events surrounding the V key pair.
    FlagsChangedChord { tap: CGEventTapLocation },
    /// No clipboard and no chord: the text is posted as synthetic typing.
    UnicodeTyping,
}

struct Variant {
    label: &'static str,
    description: &'static str,
    transport: Transport,
}

fn main() {
    let repeat = std::env::args().any(|argument| argument == "--repeat");
    // SAFETY: AXIsProcessTrusted is a side-effect-free process trust query.
    let trusted = unsafe { AXIsProcessTrusted() } != 0;
    println!("Trueflow insertion transport probe");
    println!("==================================");
    println!(
        "Accessibility trusted for this process: {}",
        if trusted { "YES" } else { "NO" }
    );
    if !trusted {
        println!();
        println!("Synthetic events will be discarded until the application running");
        println!("this probe is enabled in System Settings > Privacy & Security >");
        println!("Accessibility. Grant the terminal, then run the probe again.");
        return;
    }

    if repeat {
        run_repeat_probe();
        return;
    }

    let variants = variants();
    println!();
    println!("This probe will deposit {} markers.", variants.len());
    println!("Click into the target application and place the cursor in a document.");
    println!("Do not touch the keyboard once the countdown ends.");
    println!();
    for remaining in (1..=FOCUS_COUNTDOWN).rev() {
        println!("  starting in {remaining}...");
        sleep(Duration::from_secs(1));
    }

    match frontmost_application() {
        Some((name, pid)) => println!("\nFrontmost application: {name} (pid {pid})\n"),
        None => println!("\nFrontmost application could not be resolved\n"),
    }
    for variant in &variants {
        let marker = format!("[{}] ", variant.label);
        println!("dispatching {:<24} {}", variant.label, variant.description);
        match variant.transport {
            Transport::UnicodeTyping => type_unicode(&marker),
            _ => {
                write_clipboard(&marker);
                dispatch(variant.transport);
            }
        }
        sleep(SETTLE_BETWEEN_VARIANTS);
    }

    println!();
    println!("Done. Read the focused document and report which markers appeared,");
    println!("in the order they appear:");
    println!();
    for variant in &variants {
        println!("  [{}]  {}", variant.label, variant.description);
    }
    println!();
    println!("A marker that is absent means that transport does not reach this");
    println!("application. A marker that appears out of order means the target");
    println!("consumed the pasteboard later than the dispatch that wrote it.");
}

/// Posts the same synthetic typing Trueflow uses, repeatedly, with none of
/// Trueflow's overlay, state machine, or window ordering around it.
///
/// This isolates the alternating-failure question. Both the pasteboard chord and
/// synthetic typing alternate inside Trueflow, so the transport is not the
/// variable. If every marker below lands, the target accepts repeated synthetic
/// input and the fault is in what Trueflow does around the insertion. If the
/// markers alternate here too, the fault is in the target's handling of
/// synthetic input from this process and no insertion strategy will fix it.
fn run_repeat_probe() {
    const ATTEMPTS: usize = 6;
    println!();
    println!("Repeat mode: {ATTEMPTS} identical synthetic-typing attempts.");
    println!("Click into the target application and place the cursor in a document.");
    println!("Do not touch the keyboard once the countdown ends.");
    println!();
    for remaining in (1..=FOCUS_COUNTDOWN).rev() {
        println!("  starting in {remaining}...");
        sleep(Duration::from_secs(1));
    }

    match frontmost_application() {
        Some((name, pid)) => println!("\nFrontmost application: {name} (pid {pid})\n"),
        None => println!("\nFrontmost application could not be resolved\n"),
    }

    for attempt in 1..=ATTEMPTS {
        let marker = format!("[T{attempt}] ");
        println!("typing marker T{attempt}");
        type_unicode(&marker);
        sleep(SETTLE_BETWEEN_VARIANTS);
    }

    println!();
    println!("Done. Report which of T1..T{ATTEMPTS} appeared in the document.");
    println!();
    println!("  all {ATTEMPTS} present  -> the target accepts repeated synthetic input;");
    println!("                     the fault is in Trueflow's surrounding behaviour");
    println!("  alternating      -> the target itself rejects alternate synthetic");
    println!("                     events and no insertion strategy will fix it");
}

fn variants() -> Vec<Variant> {
    vec![
        Variant {
            label: "1-hid-flagged",
            description: "flag-on-V, HID tap, HID source (current build)",
            transport: Transport::FlaggedKeyPair {
                tap: CGEventTapLocation::HID,
                source: CGEventSourceStateID::HIDSystemState,
            },
        },
        Variant {
            label: "2-hid-flagschanged",
            description: "Command FlagsChanged chord, HID tap (previous build)",
            transport: Transport::FlagsChangedChord {
                tap: CGEventTapLocation::HID,
            },
        },
        Variant {
            label: "3-session-flagged",
            description: "flag-on-V, Session tap",
            transport: Transport::FlaggedKeyPair {
                tap: CGEventTapLocation::Session,
                source: CGEventSourceStateID::HIDSystemState,
            },
        },
        Variant {
            label: "4-annotated-flagged",
            description: "flag-on-V, AnnotatedSession tap",
            transport: Transport::FlaggedKeyPair {
                tap: CGEventTapLocation::AnnotatedSession,
                source: CGEventSourceStateID::HIDSystemState,
            },
        },
        Variant {
            label: "5-private-source",
            description: "flag-on-V, HID tap, Private source state",
            transport: Transport::FlaggedKeyPair {
                tap: CGEventTapLocation::HID,
                source: CGEventSourceStateID::Private,
            },
        },
        Variant {
            label: "6-session-flagschanged",
            description: "Command FlagsChanged chord, Session tap",
            transport: Transport::FlagsChangedChord {
                tap: CGEventTapLocation::Session,
            },
        },
        Variant {
            label: "7-unicode-typing",
            description: "synthetic typing, no clipboard and no chord",
            transport: Transport::UnicodeTyping,
        },
    ]
}

fn dispatch(transport: Transport) {
    match transport {
        Transport::FlaggedKeyPair { tap, source } => {
            let Ok(source) = CGEventSource::new(source) else {
                println!("    ! could not create the event source");
                return;
            };
            let (Ok(down), Ok(up)) = (
                CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true),
                CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, false),
            ) else {
                println!("    ! could not create the V key events");
                return;
            };
            down.set_flags(CGEventFlags::CGEventFlagCommand);
            up.set_flags(CGEventFlags::CGEventFlagCommand);
            down.post(tap);
            sleep(KEY_HOLD);
            up.post(tap);
        }
        Transport::FlagsChangedChord { tap } => {
            let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
                println!("    ! could not create the event source");
                return;
            };
            let (Ok(command_down), Ok(down), Ok(up), Ok(command_up)) = (
                CGEvent::new_keyboard_event(source.clone(), KeyCode::COMMAND, true),
                CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, true),
                CGEvent::new_keyboard_event(source.clone(), KeyCode::ANSI_V, false),
                CGEvent::new_keyboard_event(source, KeyCode::COMMAND, false),
            ) else {
                println!("    ! could not create the chord events");
                return;
            };
            command_down.set_type(CGEventType::FlagsChanged);
            command_down.set_flags(CGEventFlags::CGEventFlagCommand);
            down.set_flags(CGEventFlags::CGEventFlagCommand);
            up.set_flags(CGEventFlags::CGEventFlagCommand);
            command_up.set_type(CGEventType::FlagsChanged);
            command_up.set_flags(CGEventFlags::empty());
            command_down.post(tap);
            sleep(MODIFIER_SETTLE);
            down.post(tap);
            sleep(KEY_HOLD);
            up.post(tap);
            sleep(MODIFIER_SETTLE);
            command_up.post(tap);
        }
        Transport::UnicodeTyping => type_unicode(""),
    }
}

fn type_unicode(text: &str) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        println!("    ! could not create the event source");
        return;
    };
    let (Ok(down), Ok(up)) = (
        CGEvent::new_keyboard_event(source.clone(), 0, true),
        CGEvent::new_keyboard_event(source, 0, false),
    ) else {
        println!("    ! could not create the typing events");
        return;
    };
    down.set_string(text);
    up.set_string(text);
    down.post(CGEventTapLocation::HID);
    sleep(KEY_HOLD);
    up.post(CGEventTapLocation::HID);
}

fn write_clipboard(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let value = NSString::from_str(text);
    // SAFETY: NSPasteboardTypeString is an immutable AppKit framework constant.
    if !unsafe { pasteboard.setString_forType(&value, NSPasteboardTypeString) } {
        println!("    ! NSPasteboard rejected the marker");
    }
}

fn frontmost_application() -> Option<(String, i32)> {
    let application = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let name = application
        .localizedName()
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    Some((name, application.processIdentifier()))
}
