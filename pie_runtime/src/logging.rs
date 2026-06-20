use std::sync::atomic::{AtomicBool, Ordering};

static LOGGING_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() -> bool {
    LOGGING_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub fn is_initialized() -> bool {
    LOGGING_INITIALIZED.load(Ordering::SeqCst)
}
