//! Code shared by `hyprtrace-daemon` and `hyprtrace-server`.
//!
//! The two binaries deliberately do not share most of their code (they have
//! different runtimes, lifetimes and responsibilities), but a few leaf helpers
//! have to behave identically on both sides. Those live here so there is
//! exactly one definition instead of a "keep in sync" comment.

pub mod notify;
pub mod paths;
