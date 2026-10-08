//! The typed create and read functions, one module per entity.
//!
//! Each module adds an `impl Store` block, so a caller sees one flat API on `Store`
//! rather than five sub-objects.

mod brief;
mod finding;
mod outcome;
mod review;
mod rule;
