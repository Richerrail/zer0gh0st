use std::sync::atomic::{AtomicBool, Ordering};

/// Global "cancel the current turn" flag, set by `/stop`.
static CANCELLED: AtomicBool = AtomicBool::new(false);

pub fn request() {
    CANCELLED.store(true, Ordering::SeqCst);
}

pub fn clear() {
    CANCELLED.store(false, Ordering::SeqCst);
}

pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}
