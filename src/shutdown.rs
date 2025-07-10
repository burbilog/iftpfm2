use std::sync::atomic::{AtomicBool, Ordering};

// Global flag to indicate if shutdown was requested
/// Global shutdown flag (atomic bool)
///
/// Set to true when shutdown is requested via signal.
/// Threads should check this flag regularly and exit cleanly.
pub static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

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
