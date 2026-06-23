//! Per-peer circuit breaker for P2P inference proxies.
//!
//! Trips on connection/timeout failures (not Ollama-level HTTP errors).
//! One `CircuitBreaker` is created per inference session and shared across
//! all proxy accept loops via `Arc`.
//!
//! States:
//!   Closed  → normal; all requests pass through
//!   Open    → tripped; requests fail fast with HTTP 503 (no P2P round-trip)
//!   HalfOpen→ cooldown elapsed; one probe request allowed through
//!
//! Thresholds: 3 failures → Open; 30 s cooldown before HalfOpen.

use libp2p::PeerId;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing::{info, warn};

const FAILURE_THRESHOLD: u32 = 3;
const OPEN_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Closed,
    Open,
    HalfOpen,
}

struct PeerEntry {
    state: State,
    failures: u32,
    opened_at: Option<Instant>,
}

impl Default for PeerEntry {
    fn default() -> Self {
        Self {
            state: State::Closed,
            failures: 0,
            opened_at: None,
        }
    }
}

pub struct CircuitBreaker {
    peers: Mutex<HashMap<PeerId, PeerEntry>>,
}

impl CircuitBreaker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            peers: Mutex::new(HashMap::new()),
        })
    }

    /// Returns `true` when a request should be forwarded to the peer.
    /// Returns `false` when the circuit is open and the cooldown has not elapsed.
    /// Transitions Open → HalfOpen automatically when the cooldown expires.
    pub fn allow(&self, peer: &PeerId) -> bool {
        let mut map = self.peers.lock().unwrap();
        let entry = map.entry(*peer).or_default();
        match entry.state {
            State::Closed => true,
            State::Open => {
                let elapsed = entry
                    .opened_at
                    .map(|t| t.elapsed() >= OPEN_DURATION)
                    .unwrap_or(false);
                if elapsed {
                    info!(
                        "circuit breaker: {} → HalfOpen (cooldown elapsed)",
                        peer.to_base58()
                    );
                    entry.state = State::HalfOpen;
                    true
                } else {
                    false
                }
            }
            // HalfOpen: let exactly one probe through (caller decides outcome).
            State::HalfOpen => true,
        }
    }

    /// Record a successful P2P round-trip. Closes the circuit and resets counters.
    pub fn record_success(&self, peer: &PeerId) {
        let mut map = self.peers.lock().unwrap();
        let entry = map.entry(*peer).or_default();
        if entry.state != State::Closed {
            info!(
                "circuit breaker: {} → Closed (peer recovered)",
                peer.to_base58()
            );
        }
        *entry = PeerEntry::default();
    }

    /// Record a connection/timeout failure. Opens the circuit after
    /// `FAILURE_THRESHOLD` failures, or immediately if currently HalfOpen.
    pub fn record_failure(&self, peer: &PeerId) {
        let mut map = self.peers.lock().unwrap();
        let entry = map.entry(*peer).or_default();
        entry.failures += 1;
        let should_open = entry.failures >= FAILURE_THRESHOLD || entry.state == State::HalfOpen;
        if should_open && entry.state != State::Open {
            warn!(
                "circuit breaker: {} → Open ({} consecutive connection failure(s)); \
                 will retry in {}s",
                peer.to_base58(),
                entry.failures,
                OPEN_DURATION.as_secs(),
            );
            entry.state = State::Open;
            entry.opened_at = Some(Instant::now());
        }
    }
}
