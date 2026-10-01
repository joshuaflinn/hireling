//! The in-process party fan-out registry — E7 (plan Task 5).
//!
//! `party_id → connections`, each connection a bounded mpsc channel
//! (capacity 64, binding number). Broadcast is `try_send`-only and fully
//! synchronous: it never awaits, so one stalled reader cannot stall the
//! party (the mechanism is structural, design.md). A stalled reader's
//! channel fills, the next broadcast evicts it, and dropping the sender
//! closes its rx — the session's signal to close 1013 and let the client
//! recover via reconnect + snapshot. Eviction is the registry's whole say;
//! closing the connection is the session's job.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::sync::protocol::ServerFrame;

/// The outbound buffer per connection (contract §1/design: binding 64).
pub const CHANNEL_CAPACITY: usize = 64;

struct Conn {
    id: u64,
    tx: mpsc::Sender<ServerFrame>,
}

/// The party → connections map. Registry owns the senders; each session
/// owns its [`ConnHandle`]'s receiver.
#[derive(Default)]
pub struct PartyRegistry {
    conns: Mutex<HashMap<i64, Vec<Conn>>>,
    next_id: AtomicU64,
}

/// The session's half of one subscribed connection: the receive side of its
/// outbound channel plus the id that [`PartyRegistry::unsubscribe`] takes.
pub struct ConnHandle {
    pub rx: mpsc::Receiver<ServerFrame>,
    pub id: u64,
}

/// What one broadcast did: how many connections got the frame and how many
/// were evicted for a full buffer. A connection whose receiver was dropped
/// (session gone without unsubscribing) is silently collected — it is
/// neither delivered nor overflowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanOutReport {
    pub delivered: usize,
    pub overflowed: usize,
}

impl PartyRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a connection on `party_id`: buffer 64, unique handle id.
    #[must_use]
    pub fn subscribe(&self, party_id: i64) -> ConnHandle {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.lock()
            .entry(party_id)
            .or_default()
            .push(Conn { id, tx });
        ConnHandle { rx, id }
    }

    /// Remove one connection — the clean teardown path. Dropping the sender
    /// closes the handle's rx once drained.
    pub fn unsubscribe(&self, party_id: i64, handle_id: u64) {
        let mut conns = self.lock();
        let Some(list) = conns.get_mut(&party_id) else {
            return;
        };
        list.retain(|conn| conn.id != handle_id);
        if list.is_empty() {
            conns.remove(&party_id);
        }
    }

    /// Fan one frame out to every connection on `party_id`. Synchronous by
    /// contract: `try_send` only, never a send that awaits.
    pub fn broadcast(&self, party_id: i64, frame: &ServerFrame) -> FanOutReport {
        let mut report = FanOutReport {
            delivered: 0,
            overflowed: 0,
        };
        let mut conns = self.lock();
        let Some(list) = conns.get_mut(&party_id) else {
            return report;
        };
        let taken = std::mem::take(list);
        let mut survivors = Vec::with_capacity(taken.len());
        for conn in taken {
            match conn.tx.try_send(frame.clone()) {
                Ok(()) => {
                    report.delivered += 1;
                    survivors.push(conn);
                }
                // Stalled reader: evict. Its rx drains the buffer, then
                // closes — the session sees that as the 1013 path.
                Err(mpsc::error::TrySendError::Full(_)) => {
                    report.overflowed += 1;
                }
                // Session gone without unsubscribing: collect the handle.
                Err(mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
        if survivors.is_empty() {
            conns.remove(&party_id);
        } else {
            *list = survivors;
        }
        report
    }

    /// The map lock. Poison is not a state this registry can produce a
    /// coherent recovery for, but a panicked holder left the data
    /// structurally intact (plain inserts/removes) — continue on the
    /// interior state rather than poison-locking the whole party.
    fn lock(&self) -> MutexGuard<'_, HashMap<i64, Vec<Conn>>> {
        self.conns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
