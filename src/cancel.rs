use std::sync::atomic::{AtomicBool, Ordering};

/// Global "cancel the current turn" flag, set by `/stop` or Escape.
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

/// Attendre l'annulation même si une requête réseau ne produit aucun événement.
pub async fn cancelled() {
    while !is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
