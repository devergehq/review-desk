//! **The only module in this crate that calls `review-desk-store`.**
//!
//! Two reasons it is one file. The project's first rule of interaction is that the store
//! crate is the only code that opens the database, and a server with store calls spread
//! across a dozen handlers makes that rule hard to check. The second was immediate: this
//! crate was built against the store before DEV-860 landed, and when it landed the whole
//! rebase was two edits in this file and two in a test seed. The rule paid for itself
//! once already; it is kept for the next time.
//!
//! The wrappers are thin. They add exactly what the handlers need and the store does not
//! owe them: a lock around the one connection, and the contract's refusals for states the
//! store expresses as an absent row.

use std::sync::{Arc, Mutex};

use review_desk_store::{
    Actor, Brief, BriefOption, Decision, DecisionMade, Disposition, Draft, DraftSection, Finding,
    FindingInRound, FullReview, NewDraft, NewOutcome, NitTicket, OpenedReview, ProblemStatement,
    RepoSummary, Review, ReviewAtLayer, ReviewEvent, ReviewFacts, ReviewLayer, ReviewProvenance,
    RoundCommit, RoundSummary, Rule, RuleCounts, RuleStage, RulesSummary, Store, StoreError,
    TicketedAndApproved, Timestamp,
};

use crate::error::{ApiError, Code};

/// Who the server records as having decided.
///
/// **Always the developer, and there is no code path that chooses otherwise** — the same
/// argument as [`SIGN_OFF_ACTOR`], for the same reason. Keeping, cutting, regrading and
/// rewriting a finding are the developer's decisions, made in the dashboard, and the store
/// refuses any other actor. An endpoint that could be told whose decision to record would
/// be one that could be told the wrong one.
pub const DECIDING_ACTOR: Actor = Actor::Developer;

/// Who the server records as having signed off.
///
/// **Always the developer, and there is no code path that chooses otherwise.** Only the
/// developer may confirm a brief or approve a draft — Patrick, 9 October 2026 — and the
/// dashboard's API is where that happens; `review-desk brief confirm` no longer exists.
/// The store refuses any other actor, so this is a constant rather than an argument: a
/// sign-off endpoint that could be told whose signature to record would be one that could
/// be told the wrong one.
pub const SIGN_OFF_ACTOR: Actor = Actor::Developer;

/// The open database, shared by every worker thread.
///
/// One connection behind a mutex, rather than a connection per thread. SQLite serialises
/// writes anyway, this server has one user, and a pool would mean the store crate handing
/// out connections — which is a change to the store, and DEV-861 is not allowed one.
#[derive(Debug, Clone)]
pub struct Records {
    store: Arc<Mutex<Store>>,
}

impl Records {
    /// Wrap an open store.
    pub fn new(store: Store) -> Self {
        Records {
            store: Arc::new(Mutex::new(store)),
        }
    }

    /// Run one piece of work against the store.
    ///
    /// A poisoned lock means a worker thread panicked while holding it, so what the
    /// database holds is unknown. That is Review Desk being unable to run rather than a
    /// refusal, which is what `store_unavailable` means.
    fn with<T>(&self, work: impl FnOnce(&Store) -> Result<T, StoreError>) -> Result<T, ApiError> {
        let store = self.store.lock().map_err(|_| ApiError {
            code: Code::StoreUnavailable,
            message: "the database lock was left poisoned by a failed request".to_string(),
            field: None,
            expected: None,
        })?;
        work(&store).map_err(ApiError::from)
    }

    /// The schema version, for the health endpoint. The cheapest honest proof that the
    /// database is open and migrated, rather than that the process is merely running.
    pub fn schema_version(&self) -> Result<i64, ApiError> {
        self.with(|store| store.schema_version())
    }

    // -- reviews ------------------------------------------------------------------

    /// Open the review for this head commit: the one already on the record, or a new
    /// round, carrying whatever facts the caller knew. `review open` in the command line
    /// is the same call, so the two surfaces cannot disagree about what "the same review"
    /// is.
    pub fn open_review(
        &self,
        repo: &str,
        pr: i64,
        head_sha: &str,
        facts: &ReviewFacts,
    ) -> Result<OpenedReview, ApiError> {
        self.with(|store| store.open_review_with(repo, pr, head_sha, facts))
    }

    /// Record what is known about the pull request, leaving a fact left `None` as it was.
    ///
    /// Used by `POST /reviews` to fill in only what the record has no answer for, after
    /// the review has been opened. Separate from `open_review` because the two cases are
    /// different: what the caller stated is written whatever is there, and what was
    /// resolved from GitHub fills a gap and never replaces an answer.
    pub fn set_review_facts(&self, id: i64, facts: &ReviewFacts) -> Result<Review, ApiError> {
        self.with(|store| store.set_review_facts(id, facts))
    }

    /// One review by id, as a refusal when it is not there.
    pub fn review(&self, id: i64) -> Result<Review, ApiError> {
        self.with(|store| store.find_review_by_id(id))?
            .ok_or_else(|| ApiError::not_found(format!("no review with id {id}")))
    }

    /// A review, its layers, its brief, its findings, its outcomes and its draft, in one
    /// read. The contract's "each screen is one GET" rests on this being one read.
    pub fn full_review(&self, id: i64) -> Result<FullReview, ApiError> {
        self.review(id)?;
        self.with(|store| store.full_review(id))
    }

    /// Where the review's understanding came from: its sources, the ticket's criteria, the
    /// description lint's results and the change map, in one read.
    ///
    /// Written only by the command line. The dashboard reads it and decides nothing about
    /// it, which is why no operation in the contract writes it.
    pub fn review_provenance(&self, id: i64) -> Result<ReviewProvenance, ApiError> {
        self.with(|store| store.review_provenance(id))
    }

    /// The sequence numbers of a review's findings on one path.
    ///
    /// The change map's `finding_seqs`, read from the findings rather than stored against
    /// the entry — so a finding listed on a file is one that exists and names that file.
    pub fn findings_on_path(&self, id: i64, path: &str) -> Result<Vec<i64>, ApiError> {
        self.with(|store| store.findings_on_path(id, path))
    }

    /// The state of the one ticket covering this review's held-back nits.
    pub fn nit_ticket(&self, id: i64) -> Result<NitTicket, ApiError> {
        self.with(|store| store.nit_ticket(id))
    }

    /// Ask for that ticket, or record the one that was filed.
    pub fn request_nit_ticket(
        &self,
        id: i64,
        ticket_id: Option<&str>,
    ) -> Result<NitTicket, ApiError> {
        self.with(|store| store.request_nit_ticket(id, ticket_id))
    }

    // -- the inbox ----------------------------------------------------------------

    /// Reviews whose brief is written and not yet confirmed. Layer 2, waiting on the
    /// developer.
    pub fn reviews_with_unconfirmed_brief(&self) -> Result<Vec<Review>, ApiError> {
        self.with(|store| store.reviews_with_unconfirmed_brief(None))
    }

    /// Reviews whose draft is at this status.
    pub fn reviews_with_draft_status(
        &self,
        status: review_desk_store::DraftStatus,
    ) -> Result<Vec<Review>, ApiError> {
        self.with(|store| store.reviews_with_draft_status(status, None))
    }

    /// Reviews an author or the harvester has acted on since a posted review.
    pub fn reviews_with_author_activity(&self) -> Result<Vec<Review>, ApiError> {
        self.with(|store| store.reviews_with_author_activity(None, None))
    }

    /// Reviews a session is still working on, with the layer each is at.
    pub fn reviews_in_progress(&self) -> Result<Vec<ReviewAtLayer>, ApiError> {
        self.with(|store| store.reviews_in_progress(None))
    }

    /// Nit tickets filed since a time, for the Inbox's "handled without you".
    pub fn filed_nit_tickets(&self, since: Timestamp) -> Result<Vec<NitTicket>, ApiError> {
        self.with(|store| store.filed_nit_tickets(None, Some(since)))
    }

    /// Every repository Review Desk holds a record for.
    pub fn list_repos(&self) -> Result<Vec<RepoSummary>, ApiError> {
        self.with(|store| store.list_repos())
    }

    // -- briefs -------------------------------------------------------------------

    /// The review's brief, or a `conflict`.
    ///
    /// The contract gives `confirmBrief` a 409 for "the review has no brief to confirm",
    /// which the store reports as an absent row. A caller that asked to confirm a brief
    /// that was never written has made a request about a review that does exist, so 404
    /// would be the wrong answer and the contract says as much.
    pub fn brief_to_confirm(&self, review_id: i64) -> Result<Brief, ApiError> {
        self.with(|store| store.find_brief(review_id))?
            .ok_or_else(|| {
                ApiError::conflict(format!("review {review_id} has no brief to confirm"))
            })
    }

    /// Confirm the brief. The developer's act at layer 2, and a sign-off: see
    /// [`SIGN_OFF_ACTOR`].
    pub fn confirm_brief(&self, review_id: i64) -> Result<Brief, ApiError> {
        self.with(|store| store.confirm_brief(review_id, SIGN_OFF_ACTOR))
    }

    /// The alternatives the brief considered, in key order.
    pub fn brief_options(&self, review_id: i64) -> Result<Vec<BriefOption>, ApiError> {
        self.with(|store| store.list_brief_options(review_id))
    }

    /// The problem half of a review's brief.
    ///
    /// A review that exists and has no brief is a **conflict**, not a 404, for the reason
    /// [`Records::brief_to_confirm`] gives: the caller asked about a review that is there.
    /// The store reports an absent brief as `NotFound`, which this crate serves as 404, so
    /// the absence is read here rather than mapped from the store's error.
    pub fn problem_statement(&self, review_id: i64) -> Result<ProblemStatement, ApiError> {
        if self.with(|store| store.find_brief(review_id))?.is_none() {
            return Err(ApiError::conflict(format!(
                "review {review_id} has no brief, so it has no problem statement"
            )));
        }
        self.with(|store| store.problem_statement(review_id))
    }

    // -- findings and outcomes ----------------------------------------------------

    /// One finding by review and sequence number.
    pub fn finding(&self, review_id: i64, seq: i64) -> Result<Finding, ApiError> {
        self.with(|store| store.find_finding(review_id, seq))?
            .ok_or_else(|| ApiError::not_found(format!("review {review_id} has no finding {seq}")))
    }

    /// Every finding on a review, in sequence order.
    pub fn findings(&self, review_id: i64) -> Result<Vec<Finding>, ApiError> {
        self.with(|store| store.list_findings(review_id))
    }

    /// Move a finding between the draft, held back and suppressed.
    pub fn set_finding_disposition(
        &self,
        review_id: i64,
        seq: i64,
        disposition: Disposition,
        reason: Option<String>,
    ) -> Result<Finding, ApiError> {
        self.with(|store| store.set_finding_disposition(review_id, seq, disposition, reason))
    }

    /// Record what became of a finding.
    pub fn add_outcome(&self, review_id: i64, seq: i64, new: NewOutcome) -> Result<(), ApiError> {
        self.with(|store| {
            store
                .add_outcome(review_desk_store::FindingRef { review_id, seq }, new)
                .map(|_| ())
        })
    }

    /// Every finding of a round, with the round it was carried from, how many consecutive
    /// rounds it has been open, and whether it is late.
    ///
    /// **One call, not one query.** It walks each carried finding's chain to count the
    /// rounds it has been open, so it is a handful of primary-key reads per finding rather
    /// than a single statement. That is deliberate: the count is derived so it cannot
    /// drift, and one developer's SQLite file serves a page in well under the time a
    /// stored copy would cost in correctness. What it does keep is the contract's rule
    /// that each screen is one *GET* — the handler makes this call once.
    pub fn findings_in_round(&self, review_id: i64) -> Result<Vec<FindingInRound>, ApiError> {
        self.with(|store| store.findings_in_round(review_id))
    }

    // -- rounds -------------------------------------------------------------------

    /// Every round of a pull request, oldest first, with what each posted and what came
    /// back.
    pub fn rounds(&self, repo: &str, pr: i64) -> Result<Vec<RoundSummary>, ApiError> {
        self.with(|store| store.rounds(repo, pr))
    }

    /// The commits recorded between the previous round's head commit and this one's.
    ///
    /// **Read, never fetched.** `review-desk round commits` reads GitHub and records them;
    /// this server has one GitHub read and it is `POST /reviews`.
    pub fn round_commits(&self, review_id: i64) -> Result<Vec<RoundCommit>, ApiError> {
        self.with(|store| store.list_round_commits(review_id))
    }

    /// The findings held back from this review's draft.
    pub fn held_back_findings(&self, review_id: i64) -> Result<Vec<i64>, ApiError> {
        self.with(|store| store.held_back_findings(review_id))
    }

    // -- the review's layers ------------------------------------------------------

    /// All seven layers of a review, in order, written or not.
    pub fn review_layers(&self, review_id: i64) -> Result<Vec<ReviewLayer>, ApiError> {
        self.with(|store| store.list_review_layers(review_id))
    }

    // -- drafts -------------------------------------------------------------------

    /// The review's draft, as a refusal when there is none.
    pub fn draft(&self, review_id: i64) -> Result<Draft, ApiError> {
        self.with(|store| store.find_draft(review_id))?
            .ok_or_else(|| ApiError::not_found(format!("review {review_id} has no draft")))
    }

    /// Replace the draft's body.
    ///
    /// `NewDraft::status` only accepts `drafting` and `ready`, which is where the
    /// sign-off boundary is enforced: whatever writes a draft cannot approve it by
    /// writing a status.
    pub fn put_draft(&self, review_id: i64, new: NewDraft) -> Result<Draft, ApiError> {
        self.with(|store| store.put_draft(review_id, new))
    }

    /// Approve the draft and record the event it is to be posted as. A sign-off: see
    /// [`SIGN_OFF_ACTOR`]. **This does not post.**
    pub fn approve_draft(&self, review_id: i64, event: ReviewEvent) -> Result<Draft, ApiError> {
        self.with(|store| store.approve_draft(review_id, event, SIGN_OFF_ACTOR))
    }

    /// Send the draft back to the session with a note.
    pub fn send_back_draft(&self, review_id: i64, note: &str) -> Result<Draft, ApiError> {
        self.with(|store| store.send_back_draft(review_id, note))
    }

    /// The blocks the draft body is composed of, in order.
    pub fn draft_sections(&self, review_id: i64) -> Result<Vec<DraftSection>, ApiError> {
        self.with(|store| store.draft_sections(review_id))
    }

    /// Record the developer's decision about one finding, and recompose the draft.
    ///
    /// One transaction in the store: the finding, the block, the body and the outcome that
    /// says who decided are one step. Recorded as [`DECIDING_ACTOR`].
    pub fn decide_finding(
        &self,
        review_id: i64,
        seq: i64,
        decision: &Decision,
    ) -> Result<DecisionMade, ApiError> {
        self.with(|store| store.decide_finding(review_id, seq, decision, DECIDING_ACTOR))
    }

    /// File what is left of the review as one ticket and approve the draft, in one step.
    /// A sign-off: see [`SIGN_OFF_ACTOR`].
    pub fn ticket_rest_and_approve(
        &self,
        review_id: i64,
        note: Option<&str>,
    ) -> Result<TicketedAndApproved, ApiError> {
        self.with(|store| store.ticket_rest_and_approve(review_id, note, SIGN_OFF_ACTOR))
    }

    // -- rules --------------------------------------------------------------------

    /// One rule by id.
    pub fn rule(&self, id: i64) -> Result<Rule, ApiError> {
        self.with(|store| store.find_rule(id))?
            .ok_or_else(|| ApiError::not_found(format!("no rule with id {id}")))
    }

    /// The rules at any of these stages, for one repository or all of them.
    pub fn rules_at_stages(
        &self,
        repo: Option<&str>,
        stages: &[RuleStage],
    ) -> Result<Vec<Rule>, ApiError> {
        self.with(|store| store.list_rules_at_stages(repo, stages))
    }

    /// One rule's counts.
    pub fn rule_counts(&self, rule_id: i64) -> Result<RuleCounts, ApiError> {
        self.with(|store| store.rule_counts(rule_id))
    }

    /// The figures across the top of the Rules screen.
    pub fn rules_summary(
        &self,
        repo: Option<&str>,
        window_days: i64,
    ) -> Result<RulesSummary, ApiError> {
        self.with(|store| store.rules_summary(repo, window_days))
    }

    /// Move a rule to another stage, with the reason it moved.
    pub fn set_rule_stage(
        &self,
        id: i64,
        stage: RuleStage,
        reason: Option<&str>,
    ) -> Result<Rule, ApiError> {
        self.with(|store| store.set_rule_stage(id, stage, reason))
    }

    /// Keep a rule for longer before asking about it again.
    pub fn defer_rule(&self, id: i64, until: Timestamp) -> Result<Rule, ApiError> {
        self.with(|store| store.defer_rule(id, until, None))
    }
}

/// The current time, as the store counts it. Handlers do not read the clock themselves.
pub fn now() -> Timestamp {
    review_desk_store::now()
}
