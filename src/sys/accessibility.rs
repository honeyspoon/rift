use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use tracing::{error, info};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;

    static kAXTrustedCheckOptionPrompt: *const c_void;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFBooleanFalse: *const c_void;
}

const AX_POLL_INTERVAL: Duration = Duration::from_millis(250);
const AX_POLL_TIMEOUT: Duration = Duration::from_secs(30);

#[inline]
fn ax_is_trusted() -> bool {
    unsafe {
        autoreleasepool(|_| {
            let keys: [*mut AnyObject; 1] = [kAXTrustedCheckOptionPrompt as *mut AnyObject];
            let vals: [*mut AnyObject; 1] = [kCFBooleanFalse as *mut AnyObject];
            let dict: *mut AnyObject = msg_send![
                class!(NSDictionary),
                dictionaryWithObjects: vals.as_ptr(),
                forKeys:              keys.as_ptr(),
                count:                1usize
            ];

            AXIsProcessTrustedWithOptions(dict.cast())
        })
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn prompt_ax_trust_dialog() {
    autoreleasepool(|_| {
        let keys: [*mut AnyObject; 1] = [kAXTrustedCheckOptionPrompt as *mut AnyObject];
        let vals: [*mut AnyObject; 1] = [kCFBooleanTrue as *mut AnyObject];

        let dict: *mut AnyObject = msg_send![
            class!(NSDictionary),
            dictionaryWithObjects: vals.as_ptr(),
            forKeys:              keys.as_ptr(),
            count:                1usize
        ];

        let _ = AXIsProcessTrustedWithOptions(dict.cast());
    });
}

pub fn ensure_accessibility_permission() {
    if ax_is_trusted() {
        return;
    }

    info!("Accessibility permission is not granted; prompting user for permission now.");

    unsafe { prompt_ax_trust_dialog() };

    let start = Instant::now();
    loop {
        if ax_is_trusted() {
            info!("Accessibility permission granted");
            return;
        }

        if start.elapsed() >= AX_POLL_TIMEOUT {
            break;
        }

        thread::sleep(AX_POLL_INTERVAL);
    }

    println!(
        "Rift still does not have accessibility permission. Enable it in System Settings > Privacy & Security > Accessibility, then restart Rift."
    );

    std::process::exit(1);
}

const INPUT_MONITORING_POLL_INTERVAL: Duration = Duration::from_secs(60);

fn input_monitoring_granted() -> bool { unsafe { CGPreflightListenEventAccess() } }

fn log_input_monitoring(granted: bool) {
    if granted {
        info!("INPUT_MONITORING_GRANTED");
    } else {
        error!(
            "INPUT_MONITORING_MISSING: hotkeys will not work. Allow rift under System Settings > \
Privacy & Security > Input Monitoring, then restart rift."
        );
    }
}

/// Hotkeys come from an HID-level keyboard tap. Without Input Monitoring macOS
/// still creates the tap and feeds it mouse events, but never key events, so
/// nothing fails visibly. Check at startup, ask macOS to list rift in the
/// Input Monitoring pane if it is missing, and keep watching so a grant lost
/// while running is logged too.
pub fn watch_input_monitoring_permission() {
    let mut granted = input_monitoring_granted();
    log_input_monitoring(granted);
    if !granted {
        unsafe { CGRequestListenEventAccess() };
    }
    thread::spawn(move || {
        loop {
            thread::sleep(INPUT_MONITORING_POLL_INTERVAL);
            let now = input_monitoring_granted();
            if now != granted {
                log_input_monitoring(now);
                granted = now;
            }
        }
    });
}
