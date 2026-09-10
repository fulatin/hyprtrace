//! The canonical database schema, embedded at compile time.
//!
//! `schema/hyprtrace.sql` is shared verbatim with `hyprtrace-server` through
//! `include_str!`, so the two migration paths can no longer drift apart. Before
//! this module existed each crate kept its own hand-written copy of the DDL
//! with a "KEEP IN SYNC" comment, and the copies had already diverged once.
//!
//! This file deliberately holds nothing but the embedded string: it sits
//! outside `db/` so the module split of `db.rs` cannot change the relative path.

/// Canonical DDL for the tables both components own. Every statement is
/// idempotent, so `Database::migrate()` can run it on every start.
pub const SCHEMA_SQL: &str = include_str!("../../../schema/hyprtrace.sql");

/// Committed snapshot of the shape `SCHEMA_SQL` (plus the legacy column
/// upgrades) produces, asserted by `db::tests::migrate_*`.
pub const EXPECTED_SCHEMA: &str = include_str!("../../../schema/expected-columns.txt");
