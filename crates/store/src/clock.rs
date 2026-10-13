//! Where the store's times come from.
//!
//! **Every time the record holds is stamped here, by the store, and by nothing else.**
//! That is DEV-890's first rule: a command or a request that could supply a time could
//! backdate a step and invent a duration, so neither the command line's arguments nor
//! the HTTP API's bodies carry one, and no write function on [`crate::Store`] takes a
//! timestamp for a step.
//!
//! Which leaves the problem that put this module here. A duration cannot be tested
//! against a clock that only moves forward in real time: a test for "a step that ran
//! twice reports both runs and their total" would have to sleep for the total, and a
//! test for "four minutes with the agent" could not be written at all. So the clock is a
//! value the store is opened with, and a test opens a store with one it can move.
//!
//! **The injection point is always there; a clock that can lie is not.**
//! [`Clock::fixed`] exists only under the `fixed-clock` feature, which the store's own
//! dev-dependency on itself turns on for its tests and which nothing turns on for a
//! release binary. [`crate::Store::open_with_clock`] is therefore safe to leave
//! ungated — without the feature the only clock that can be built is the system one, so
//! the function can do nothing a caller could not do with [`crate::Store::open`].

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::model::Timestamp;

/// The clock a [`crate::Store`] stamps its writes from.
#[derive(Debug, Clone)]
pub struct Clock(Source);

/// What a clock reads from. Private, so the set cannot be extended from outside.
#[derive(Debug, Clone)]
enum Source {
    /// The machine's clock.
    System,
    /// A clock that moves one second on every reading. Shared, for `Fixed`'s reason.
    ///
    /// **For the tests that are about two writes having one time.** A fixed clock cannot
    /// fail them and neither can the system clock: both readings land in the same second,
    /// so an invariant like "the draft is stamped from the move rather than from a second
    /// reading" holds whether the code threads the time through or reads the clock twice.
    /// Measured by a review of this branch — the test passed 200 times out of 200 with the
    /// code it was written to guard reverted. This clock puts the second boundary between
    /// every pair of readings, so the test fails by construction without the fix.
    ///
    /// Gated with `Fixed` and present for the same reason: nothing a release binary can
    /// build.
    #[cfg_attr(not(feature = "fixed-clock"), allow(dead_code))]
    Ticking(Arc<AtomicI64>),
    /// A time a test holds and moves. Shared, so a clock handed to a store and the copy
    /// the test kept are the same clock.
    ///
    /// Only [`Clock::fixed`] builds one, and the `fixed-clock` feature gates it — so
    /// without that feature this variant is present and unreachable, which is the point
    /// rather than an oversight.
    #[cfg_attr(not(feature = "fixed-clock"), allow(dead_code))]
    Fixed(Arc<AtomicI64>),
}

impl Clock {
    /// The machine's clock. What every binary runs on.
    pub fn system() -> Self {
        Clock(Source::System)
    }

    /// What the time is now.
    pub fn now(&self) -> Timestamp {
        match &self.0 {
            Source::System => crate::model::now(),
            Source::Fixed(held) => held.load(Ordering::SeqCst),
            // Every reading is a second later than the one before it, which is what makes a
            // test about two writes sharing one time able to fail.
            Source::Ticking(held) => held.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// A clock stopped at `at`, which [`Clock::advance`] moves.
    ///
    /// **For tests, and only reachable from them.** See the module's note on the
    /// `fixed-clock` feature.
    #[cfg(feature = "fixed-clock")]
    pub fn fixed(at: Timestamp) -> Self {
        Clock(Source::Fixed(Arc::new(AtomicI64::new(at))))
    }

    /// A clock that reads one second later every time it is asked, starting at `at`.
    ///
    /// **For the tests that are about two writes having one time**, and the only clock that
    /// can fail one — see [`Source::Ticking`]. A test that wants time to stand still wants
    /// [`Clock::fixed`]; this one is for proving that a function threads a time through
    /// rather than reading the clock again.
    #[cfg(feature = "fixed-clock")]
    pub fn ticking(at: Timestamp) -> Self {
        Clock(Source::Ticking(Arc::new(AtomicI64::new(at))))
    }

    /// Move a fixed clock on by `seconds`, and say what it now reads.
    ///
    /// Does nothing to the system clock, which is the honest answer rather than a panic:
    /// a helper that both kinds of store accept keeps a test from having to know which
    /// one it was handed.
    #[cfg(feature = "fixed-clock")]
    pub fn advance(&self, seconds: i64) -> Timestamp {
        match &self.0 {
            // Neither the system clock nor a ticking one is a clock a test moves: the first
            // moves itself and the second moves on every reading, which is the whole of what
            // it is for. The honest answer is what it reads now, which is `fixed`'s own rule
            // for the system clock.
            Source::System | Source::Ticking(_) => self.now(),
            Source::Fixed(held) => held.fetch_add(seconds, Ordering::SeqCst) + seconds,
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock::system()
    }
}
