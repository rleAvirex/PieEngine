//! Milestone 8: networking protocol types.
//!
//! Transport-agnostic types for the server-authoritative + client-side-prediction
//! model documented in `pie-engine-project-brief.md` → "Networking model". The
//! actual transport (decided: `renet` + `renetcode`) lives behind a feature flag
//! and an adapter; these types are the protocol contract both client and server
//! share, so the simulation stays deterministic and in sync.
//!
//! ## Why transport-agnostic types?
//!
//! The brief mandates that client and server run the *same* `SimulationCore` and
//! produce identical state from identical input. The protocol types here encode
//! that contract:
//!
//! - [`InputCommand`]: a single client input frame, tagged with a monotonically
//!   increasing sequence number. Sent client→server (reliable-unordered) and
//!   also applied locally for prediction.
//! - [`Snapshot`]: the server's authoritative state at a given tick, plus the
//!   highest input sequence it has acknowledged. Sent server→client (unreliable;
//!   stale snapshots are dropped).
//! - [`ClientInputBuffer`]: the rolling ring of unacknowledged predicted inputs
//!   the client keeps so it can replay them after a server correction.
//!
//! These types deliberately avoid any renet-specific imports so that a future
//! transport swap touches only the adapter, not the protocol or the simulation.

use std::collections::VecDeque;

/// A monotonically-increasing input sequence number. Wraps at u32::MAX; the
/// comparison helpers below handle wraparound so a client that has been running
/// longer than ~2 years at 60 Hz still orders inputs correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sequence(pub u32);

impl Sequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// True if `self` is *newer than* `other`, accounting for u32 wraparound.
    /// Uses the standard "serial number arithmetic" trick: a difference in the
    /// range `(0, 2^31)` means newer.
    pub fn is_newer_than(self, other: Self) -> bool {
        let diff = self.0.wrapping_sub(other.0);
        diff > 0 && diff < (u32::MAX / 2)
    }
}

impl std::fmt::Display for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "seq({})", self.0)
    }
}

/// A single client input frame, tagged with its sequence number.
///
/// The `payload` is an opaque byte slice in v1 — the simulation's input layout
/// is gameplay-specific (which keys, analog stick values, etc.) and not the
/// engine's concern. The engine only guarantees: same `payload` + same starting
/// state → same resulting state (the determinism contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputCommand {
    pub sequence: Sequence,
    pub payload: Vec<u8>,
}

impl InputCommand {
    pub fn new(sequence: Sequence, payload: Vec<u8>) -> Self {
        Self { sequence, payload }
    }
}

/// The server's authoritative snapshot of simulation state at a given tick,
/// broadcast to clients. Carries the highest input sequence the server has
/// applied (`last_acknowledged`) so the client knows which predicted inputs it
/// can discard.
///
/// `state` is opaque bytes in v1 — the serialization of the relevant
/// `SimulationCore` subset (entities + resources the gameplay needs to
/// replicate). Keeping it opaque here means the engine doesn't dictate the
/// serialization format; that's a gameplay/sim-layer decision (and a candidate
/// for the future serde-based save/load system, see future-systems.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The simulation tick this snapshot was taken at.
    pub tick: u64,
    /// The highest client input sequence the server has applied to produce this
    /// state. The client uses this to discard acknowledged predictions.
    pub last_acknowledged: Sequence,
    /// Serialized authoritative state. Opaque to the transport layer.
    pub state: Vec<u8>,
}

impl Snapshot {
    pub fn new(tick: u64, last_acknowledged: Sequence, state: Vec<u8>) -> Self {
        Self {
            tick,
            last_acknowledged,
            state,
        }
    }
}

/// A rolling buffer of unacknowledged predicted inputs, kept client-side so the
/// client can replay them after a server correction.
///
/// The prediction/reconciliation flow (from the brief):
/// 1. Client captures input, assigns the next sequence, pushes it here, and
///    applies it locally (prediction).
/// 2. Client sends the input to the server.
/// 3. Server runs the same sim authoritatively and broadcasts a snapshot with
///    `last_acknowledged`.
/// 4. Client receives the snapshot, calls [`discard_acknowledged`] to drop inputs
///    the server has seen, hard-sets local state to the snapshot, then
///    [`replay_pending`] to re-apply any still-unacknowledged inputs on top.
///
/// If predicted and replayed state match, the player sees nothing change. If
/// they don't, the correction is absorbed smoothly over a frame or two (the
/// smoothing is a gameplay-layer concern, not encoded here).
#[derive(Debug, Default)]
pub struct ClientInputBuffer {
    pending: VecDeque<InputCommand>,
}

impl ClientInputBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a newly-captured predicted input. The client should push *before*
    /// applying it locally, so a crash between push and apply doesn't leave the
    /// buffer missing an input the server will eventually acknowledge.
    pub fn push(&mut self, command: InputCommand) {
        self.pending.push_back(command);
    }

    /// Number of unacknowledged inputs still pending replay.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drop every input with a sequence the server has already acknowledged
    /// (i.e. not newer than `last_acknowledged`). Called after receiving a
    /// snapshot, before replaying the remainder.
    pub fn discard_acknowledged(&mut self, last_acknowledged: Sequence) {
        self.pending
            .retain(|cmd| cmd.sequence.is_newer_than(last_acknowledged));
    }

    /// Iterate over the still-pending inputs in sequence order, so the client
    /// can replay them on top of the server-corrected state.
    pub fn replay_pending(&self) -> impl Iterator<Item = &InputCommand> {
        self.pending.iter()
    }

    /// The highest sequence currently pending, or `None` if empty. Useful for
    /// diagnostics and for the "did prediction get far ahead of the server?"
    /// check.
    pub fn latest_pending(&self) -> Option<Sequence> {
        self.pending.back().map(|c| c.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientInputBuffer, InputCommand, Sequence, Snapshot};

    // ----- Sequence wraparound -----

    #[test]
    fn sequence_next_increments() {
        let s = Sequence(5);
        assert_eq!(s.next(), Sequence(6));
    }

    #[test]
    fn sequence_next_wraps_at_max() {
        let s = Sequence(u32::MAX);
        assert_eq!(s.next(), Sequence(0));
    }

    #[test]
    fn sequence_is_newer_than_handles_normal_ordering() {
        assert!(Sequence(10).is_newer_than(Sequence(5)));
        assert!(!Sequence(5).is_newer_than(Sequence(10)));
        assert!(!Sequence(5).is_newer_than(Sequence(5))); // equal is not newer
    }

    #[test]
    fn sequence_is_newer_than_handles_wraparound() {
        // Server is at u32::MAX (about to wrap); client sends seq=2 (wrapped).
        // seq 2 should be newer than u32::MAX.
        assert!(Sequence(2).is_newer_than(Sequence(u32::MAX)));
        // And the reverse isn't.
        assert!(!Sequence(u32::MAX).is_newer_than(Sequence(2)));
    }

    // ----- InputCommand / Snapshot -----

    #[test]
    fn input_command_carries_sequence_and_payload() {
        let cmd = InputCommand::new(Sequence(42), vec![1, 2, 3]);
        assert_eq!(cmd.sequence, Sequence(42));
        assert_eq!(cmd.payload, vec![1, 2, 3]);
    }

    #[test]
    fn snapshot_carries_tick_ack_and_state() {
        let snap = Snapshot::new(100, Sequence(98), vec![0xAA; 16]);
        assert_eq!(snap.tick, 100);
        assert_eq!(snap.last_acknowledged, Sequence(98));
        assert_eq!(snap.state.len(), 16);
    }

    // ----- ClientInputBuffer prediction/reconciliation flow -----

    #[test]
    fn buffer_push_and_len() {
        let mut buf = ClientInputBuffer::new();
        assert!(buf.is_empty());
        buf.push(InputCommand::new(Sequence(1), vec![]));
        buf.push(InputCommand::new(Sequence(2), vec![]));
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.latest_pending(), Some(Sequence(2)));
    }

    #[test]
    fn discard_acknowledged_drops_inputs_at_or_below_the_ack() {
        // Client predicted seqs 1..=5. Server acknowledges seq 3 (it has applied
        // 1, 2, 3). The client should keep only 4 and 5 for replay.
        let mut buf = ClientInputBuffer::new();
        for s in 1..=5u32 {
            buf.push(InputCommand::new(Sequence(s), vec![]));
        }
        buf.discard_acknowledged(Sequence(3));
        let remaining: Vec<u32> = buf.replay_pending().map(|c| c.sequence.0).collect();
        assert_eq!(remaining, vec![4, 5]);
    }

    #[test]
    fn discard_acknowledged_with_wraparound() {
        // Server acks seq u32::MAX; client has pending [u32::MAX, 0, 1] (wrapped).
        // After ack, only [0, 1] should remain (the wrapped-newer ones).
        let mut buf = ClientInputBuffer::new();
        buf.push(InputCommand::new(Sequence(u32::MAX), vec![]));
        buf.push(InputCommand::new(Sequence(0), vec![]));
        buf.push(InputCommand::new(Sequence(1), vec![]));
        buf.discard_acknowledged(Sequence(u32::MAX));
        let remaining: Vec<u32> = buf.replay_pending().map(|c| c.sequence.0).collect();
        assert_eq!(remaining, vec![0, 1]);
    }

    #[test]
    fn replay_pending_preserves_sequence_order() {
        let mut buf = ClientInputBuffer::new();
        for s in 10..=13u32 {
            buf.push(InputCommand::new(Sequence(s), vec![s as u8]));
        }
        let seqs: Vec<u32> = buf.replay_pending().map(|c| c.sequence.0).collect();
        assert_eq!(seqs, vec![10, 11, 12, 13]);
    }

    #[test]
    fn full_prediction_reconciliation_cycle() {
        // End-to-end model of the flow, minus the actual sim:
        // 1. Client predicts 5 inputs.
        // 2. Server acks seq 3 with a snapshot.
        // 3. Client discards 1..=3, replays 4..=5 on top of the snapshot state.
        let mut buf = ClientInputBuffer::new();
        for s in 1..=5u32 {
            buf.push(InputCommand::new(Sequence(s), vec![s as u8]));
        }
        assert_eq!(buf.len(), 5);

        // Server snapshot acks seq 3.
        let snap = Snapshot::new(103, Sequence(3), vec![0xFF; 8]);
        buf.discard_acknowledged(snap.last_acknowledged);

        assert_eq!(buf.len(), 2, "only seqs 4 and 5 should remain");
        let replayed: Vec<u32> = buf.replay_pending().map(|c| c.sequence.0).collect();
        assert_eq!(replayed, vec![4, 5]);
    }
}
