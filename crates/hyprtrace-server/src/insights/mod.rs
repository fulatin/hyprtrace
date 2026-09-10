//! Pure analytics helpers and serde models for the read-only `/api/insights/*`
//! endpoints.
//!
//! Everything in this module is a plain function over already-fetched rows so it
//! can be unit-tested without a database (see `mod tests` at the bottom). The
//! SQL lives in `db.rs`; the HTTP plumbing lives in `routes/insights.rs`.
//!
//! Contract notes that explain the non-obvious formulas:
//!   * `probability` in the transition graph is row-normalised over *all*
//!     outgoing transitions of the source app, including the self transition, so
//!     every matrix row sums to ~1.0. Self edges are reported per node
//!     (`self_probability`) but never as edges.
//!   * `context_switch_cost_ms` charges half of the previous dwell time (capped
//!     at 60s) per switch: the re-orientation cost of rebuilding context after
//!     leaving an app is bounded by roughly half a minute of lost focus.
//!   * robust z-scores use median + 1.4826*MAD so a single huge day cannot drag
//!     the baseline, with a std-dev fallback when MAD collapses to 0.

mod anomalies;
mod cooccurrence;
mod disruption;
mod forecast;
mod fragmentation;
mod rhythm;
mod session;
mod stats;
#[cfg(test)]
mod test_support;
mod time;
mod transitions;

// Re-exported so every name that used to sit directly in `insights` is still
// reachable as `crate::insights::X`. A few of them (the inner serde models) are
// only ever named by their own submodule, and an unused `pub use` is a lint in a
// binary crate, hence the scoped allow.
#[allow(unused_imports)]
pub use self::{
    anomalies::{anomalies, AnomaliesResponse, AnomalyDay, AnomalyInput, efficiency_subscore},
    cooccurrence::{cooccurrence, CooccurrenceNode, CooccurrencePair, CooccurrenceResponse},
    disruption::{
        disruption_impact, DisruptionBucket, DisruptionCorrelations, DisruptionDay,
        DisruptionImpactResponse,
    },
    forecast::{forecast, ForecastFuturePoint, ForecastPoint, ForecastResponse},
    fragmentation::{
        fragmentation, switch_cost_ms, FragmentationDay, FragmentationResponse, WorstDay,
    },
    rhythm::{rhythm, RhythmCell, RhythmResponse},
    session::InsightSession,
    time::{default_from, today_local},
    transitions::{
        transitions, TransitionEdge, TransitionFlow, TransitionMatrix, TransitionNode,
        TransitionsResponse,
    },
};
