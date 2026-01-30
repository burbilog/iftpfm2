use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// Global flag to indicate if shutdown was requested
/// Global shutdown flag (atomic bool)
///
/// Set to true when shutdown is requested via signal.
/// Threads should check this flag regularly and exit cleanly.
pub static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global flag to track which signal triggered shutdown
///
/// Values: 0 = none, 1 = SIGINT, 2 = SIGTERM
pub static SIGNAL_TYPE: AtomicU8 = AtomicU8::new(0);

// Check if shutdown was requested
/// Checks if graceful shutdown has been requested
///
/// Threads should call this regularly and exit cleanly if true
///
/// # Returns
/// `true` if shutdown was requested via signal
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

// Signal that shutdown is requested
/// Signals all threads to shutdown gracefully
///
/// Sets global flag that threads should check via is_shutdown_requested()
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

/// Request shutdown with signal type for later logging
///
/// This is async-signal-safe and only sets atomic flags.
/// Logging should be done in the main thread.
///
/// # Arguments
/// * `signal_type` - 1 for SIGINT, 2 for SIGTERM
pub fn request_shutdown_with_signal(signal_type: u8) {
    SIGNAL_TYPE.store(signal_type, Ordering::SeqCst);
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

/// Get the signal type that triggered shutdown
///
/// # Returns
/// * `None` if no signal received
/// * `Some(1)` for SIGINT (Ctrl+C)
/// * `Some(2)` for SIGTERM
pub fn get_signal_type() -> Option<u8> {
    let signal_type = SIGNAL_TYPE.load(Ordering::SeqCst);
    if signal_type == 0 {
        None
    } else {
        Some(signal_type)
    }
}

/// Reset the shutdown flag (for testing purposes only)
///
/// # Warning
/// This should only be used in tests to reset state between test cases.
#[cfg(test)]
pub fn reset_shutdown_for_tests() {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    SIGNAL_TYPE.store(0, Ordering::SeqCst);
}
