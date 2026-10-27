//! AI runtime: HyprTrace delegates every AI feature to DeepSeek Harness.
//!
//! - [`service::AiService`] owns the `dsh` child process, the conversation
//!   sessions, streaming, cancellation and the model/tool inventory.
//! - [`bridge::Bridge`] is the Unix-socket side channel to the in-harness
//!   plugin that supplies what the SDK protocol does not (token deltas,
//!   reasoning, cancel, inventory).
//!
//! There is no in-process LLM provider and no in-process tool registry any
//! more: the harness owns the agent loop and its tools.

pub mod bridge;
pub mod service;

pub use service::AiService;
