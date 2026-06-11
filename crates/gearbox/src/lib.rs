//! Native Rust reference for the cog-store protocol — JCS canonicalization + Ed25519
//! catalog verification (protocol §7). The native counterpart to the Python `tools/`
//! generator and the device-side verifier in `cognitum-one/seed` (B4); all three are
//! pinned by `docs/protocol/testvectors/`.
//!
//! Phase-1 scope: catalog signature **verification** (the B4-critical path). Catalog
//! generation + signing parity with the Python tools is the next slice.

pub mod catalog;
pub mod jcs;
pub mod server;
pub mod signing;
pub mod store;
