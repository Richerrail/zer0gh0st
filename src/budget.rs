//! Budget d'appels LLM **par tour utilisateur**, sous-agents inclus.
//!
//! Pourquoi : les plafonds existants (`MAX_ITERATIONS`, `MAX_TOOL_CALLS`) sont
//! locaux à un `run_turn`. Or un seul appel d'outil `0box` lance un débat
//! (3 perspectives × plusieurs itérations, potentiellement récursif). Un message
//! de l'utilisateur pouvait donc déclencher des dizaines de requêtes — un
//! « déni de portefeuille » sur un forfait gratuit.
//!
//! Le compteur est global au processus et **remis à zéro au premier niveau**
//! seulement : un sous-agent hérite du budget de son tour parent, il ne
//! recommence pas le compte.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Plafond par défaut d'appels HTTP au provider pour un tour utilisateur.
/// Surchargeable par `ZER0_MAX_LLM_CALLS`.
pub const DEFAULT_MAX_LLM_CALLS: usize = 120;

static DEPTH: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

/// Lit le plafond à partir d'une valeur d'environnement (fonction **pure** :
/// les tests n'ont pas à muter l'environnement, ce qui serait une course).
pub fn max_calls_from(env: Option<&str>) -> usize {
    env.and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_LLM_CALLS)
}

pub fn max_calls() -> usize {
    max_calls_from(std::env::var("ZER0_MAX_LLM_CALLS").ok().as_deref())
}

/// Garde de tour : décrémente la profondeur à la sortie, même en cas d'erreur.
pub struct TurnGuard {
    _priv: (),
}

impl TurnGuard {
    /// Entre dans un tour. Au niveau 0 (tour utilisateur), remet le compteur à zéro.
    pub fn enter() -> Self {
        if DEPTH.fetch_add(1, Ordering::SeqCst) == 0 {
            CALLS.store(0, Ordering::SeqCst);
        }
        TurnGuard { _priv: () }
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        DEPTH.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Réserve un appel LLM. Retourne `false` si le budget du tour est épuisé.
pub fn try_consume() -> bool {
    let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
    n <= max_calls()
}

pub fn used() -> usize {
    CALLS.load(Ordering::SeqCst)
}

pub fn remaining() -> usize {
    max_calls().saturating_sub(used())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les compteurs sont globaux : on sérialise les tests de ce module pour
    /// qu'ils ne se marchent pas dessus (sinon test flaky par construction).
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn env_parsing() {
        assert_eq!(max_calls_from(Some("42")), 42);
        assert_eq!(max_calls_from(Some(" 7 ")), 7);
        assert_eq!(max_calls_from(Some("0")), DEFAULT_MAX_LLM_CALLS);
        assert_eq!(max_calls_from(Some("nope")), DEFAULT_MAX_LLM_CALLS);
        assert_eq!(max_calls_from(None), DEFAULT_MAX_LLM_CALLS);
    }

    #[test]
    fn nested_turns_share_the_budget() {
        let _serial = serial();
        let outer = TurnGuard::enter();
        assert_eq!(used(), 0, "le premier niveau remet à zéro");
        assert!(try_consume());
        assert!(try_consume());
        assert_eq!(used(), 2);

        {
            // Sous-agent : même budget, pas de remise à zéro.
            let _inner = TurnGuard::enter();
            assert_eq!(used(), 2);
            assert!(try_consume());
            assert_eq!(used(), 3);
        }
        assert_eq!(used(), 3);
        drop(outer);

        // Nouveau tour utilisateur : compteur neuf.
        let _next = TurnGuard::enter();
        assert_eq!(used(), 0);
    }

    #[test]
    fn budget_is_enforced() {
        let _serial = serial();
        let _g = TurnGuard::enter();
        // On consomme jusqu'au plafond : toutes les réservations passent…
        for _ in 0..max_calls() {
            assert!(try_consume());
        }
        // … et la suivante est refusée.
        assert!(!try_consume());
        assert_eq!(remaining(), 0);
    }
}
