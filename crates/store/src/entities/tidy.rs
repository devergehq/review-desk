//! The one-off tidy for the duplicate unposted rounds already on the record. DEV-962, item
//! 4 of "What changes".
//!
//! **It shows, and the developer decides.** The issue is explicit: the tidy "shows each pull
//! request's extra unposted rounds and folds them into the newest (or dismisses them);
//! nothing is deleted without confirmation". So this module answers two questions and
//! performs one act, and the act takes a review id the developer's own screen passed —
//! there is no command line path to any of it, which is the rule every act of the
//! developer's follows.
//!
//! **Two acts, because two kinds of row are there.** An extra round holding nothing a
//! delete would lose is *folded*: its head is recorded on the newest round as a
//! [`HeadUpdateKind::Folded`] update — which is what `open_review` would have done had this
//! rule existed when the commit arrived — and the empty row goes. An extra round holding a
//! brief, a finding or a draft is *dismissed* instead, through the dismissal that already
//! exists: it keeps every row under it, which is the whole argument for choosing a
//! dismissal over a delete.
//!
//! **Nothing here can arise from new data.** Since DEV-962 a round opens only when the
//! newest one has posted, so a pull request cannot grow a second unposted round. What this
//! is for is the record as it stands on 13 October 2026, where a queue showed one pull
//! request twice and another reached round 4 with no post at all.

use rusqlite::{params, Transaction, TransactionBehavior};

use crate::entities::inbox::row_to_review;
use crate::entities::review::COLUMNS;
use crate::error::{Result, StoreError};
use crate::model::{
    DuplicateRounds, ExtraRound, FoldedRound, HeadUpdateKind, Review, RoundRef, TidyAct,
};
use crate::{normalise, Store};

impl Store {
    /// Every pull request with more than one unposted round, and what the tidy would do
    /// with each extra one.
    ///
    /// Newest pull request first — by the newest round's id, which is the order the Reviews
    /// list serves and the order they were opened in.
    ///
    /// **An extra round is one that is not the newest and has not posted.** A posted round
    /// is a review that happened and is not a duplicate of anything; the newest round is
    /// the one everything folds into, so it is never in its own list. A *dismissed* extra
    /// round is left out too: the developer has already taken it out of play, and offering
    /// to tidy it would be asking them to decide the same thing twice.
    ///
    /// **It reads whole reviews rather than counting**, because the screen has to say what
    /// each extra round holds before the developer agrees to anything — "round 2 holds a
    /// brief and 4 findings" is the sentence that makes a dismissal the obvious choice over
    /// a fold.
    pub fn duplicate_rounds(&self, repo: Option<&str>) -> Result<Vec<DuplicateRounds>> {
        let repo = repo.map(normalise::repo);

        // One read of every round of every pull request that has an extra unposted round.
        // The alternative — a query per pull request — is a page that gets slower the
        // longer the record is, and this is a screen the developer opens once.
        let mut statement = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reviews v
              WHERE (?1 IS NULL OR v.repo = ?1)
                AND EXISTS (
                      SELECT 1 FROM reviews extra
                       WHERE extra.repo = v.repo AND extra.pr = v.pr
                         AND extra.dismissed_at IS NULL
                         AND extra.round < (SELECT MAX(round) FROM reviews newest
                                             WHERE newest.repo = v.repo
                                               AND newest.pr = v.pr)
                         AND NOT EXISTS (
                               SELECT 1 FROM drafts
                                WHERE drafts.review_id = extra.id
                                  AND drafts.posted_at IS NOT NULL
                             )
                         AND extra.github_review_id IS NULL
                    )
              ORDER BY v.repo, v.pr, v.round"
        ))?;
        let rounds = statement
            .query_map(params![repo], row_to_review)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut answer: Vec<DuplicateRounds> = Vec::new();
        for round in rounds {
            let last = answer
                .last_mut()
                .filter(|held| held.repo == round.repo && held.pr == round.pr);
            match last {
                // The rows arrive oldest round first, so the one before this is always an
                // earlier round of the same pull request: the newest held so far becomes an
                // extra, and this round becomes the newest.
                Some(held) => {
                    let was_newest = std::mem::replace(&mut held.newest, reference(&round));
                    let title = round.title.clone().or_else(|| held.title.clone());
                    held.title = title;
                    if let Some(extra) = self.extra(was_newest)? {
                        held.extras.push(extra);
                    }
                }
                None => answer.push(DuplicateRounds {
                    repo: round.repo.clone(),
                    pr: round.pr,
                    title: round.title.clone(),
                    newest: reference(&round),
                    extras: Vec::new(),
                }),
            }
        }

        // The newest pull request first, and its extras oldest round first — the order they
        // were opened in, which is the order a reader follows them.
        answer.retain(|found| !found.extras.is_empty());
        answer.sort_by_key(|found| std::cmp::Reverse(found.newest.review_id));
        Ok(answer)
    }

    /// One extra round, with what it holds and what the tidy would do with it.
    ///
    /// `None` when it is a round the tidy has nothing to say about: posted, or dismissed.
    /// The query above excludes both, and this asks again rather than trusting it — the
    /// answer decides whether a row is offered for folding, and a round that posted must
    /// never be.
    fn extra(&self, round: RoundRef) -> Result<Option<ExtraRound>> {
        if self.review_is_posted(round.review_id)? {
            return Ok(None);
        }
        let review = self.get_review(round.review_id)?;
        if review.is_dismissed() {
            return Ok(None);
        }

        let holds = self.review_work(round.review_id)?;
        Ok(Some(ExtraRound {
            // **Fold what holds nothing, dismiss what holds something**, and the question is
            // the one `delete_review` asks: a brief, a finding, a draft or a posted review
            // is work the developer would lose, and a dismissal keeps all of it.
            act: if holds.started() {
                TidyAct::Dismiss
            } else {
                TidyAct::Fold
            },
            // **"nothing" rather than the empty string.** `in_words` is composed for a
            // refusal, which is only ever written when something is held, so it answers
            // nothing at all for the rows this tidy is mostly made of — and the screen has
            // to say what it would remove before the developer agrees to it.
            holds: if holds.started() {
                holds.in_words()
            } else {
                "nothing".to_string()
            },
            review_id: round.review_id,
            round: round.round,
            head_sha: round.head_sha,
            created_at: review.created_at,
        }))
    }

    /// Fold an extra unposted round into its pull request's newest round.
    ///
    /// **The developer's act, and the dashboard is the only way to it.** There is no command
    /// for it, the way there is none for a dismissal or a delete: a session that could reach
    /// this could remove a round of its own review.
    ///
    /// What it does is what `open_review` would have done had DEV-962's rule existed when
    /// the commit arrived: the folded round's head is recorded against the newest round as a
    /// [`HeadUpdateKind::Folded`] update, and the empty row goes. So the history survives
    /// the row — "the request was at `4d9464f` before it was at `a41c9e2`" stays answerable
    /// — which is the difference between folding and deleting.
    ///
    /// **It refuses anything that would lose something.** A round that is the newest, that
    /// has posted, or that holds a brief, a finding or a draft is refused by name, and the
    /// last of those three is `Store::refuse_if_started` — the same function
    /// `delete_review` refuses with, rather than a second opinion here: a screen that
    /// offered a fold the delete would refuse is the shape this avoids.
    ///
    /// **One transaction over the update and the removal**, because they are one fact: a
    /// folded round whose head went unrecorded is the thing this act exists to avoid, and a
    /// recorded fold whose row survived would be a round counted twice. Driven by a test of
    /// this branch, which found the row left behind by a refused fold.
    pub fn fold_round(&self, review_id: i64) -> Result<FoldedRound> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let review = self.get_review(review_id)?;
        let newest = self
            .newest_round(&review.repo, review.pr)?
            .ok_or(StoreError::NotFound {
                entity: "pull request",
                id: format!("{}#{}", review.repo, review.pr),
            })?;

        if newest.id == review.id {
            return Err(StoreError::InvalidValue {
                field: "review_id",
                value: review_id.to_string(),
                expected: format!(
                    "an earlier round than {}, which is the newest round of {}#{} and the \
                     round the others fold into",
                    newest.round, review.repo, review.pr
                ),
            });
        }

        if self.review_is_posted(review.id)? {
            return Err(StoreError::InvalidValue {
                field: "review_id",
                value: review_id.to_string(),
                expected: format!(
                    "a round that was never posted; round {} of {}#{} posted its review, so \
                     it is a review that happened rather than a duplicate",
                    review.round, review.repo, review.pr
                ),
            });
        }

        // **What it holds is checked before anything is written**, through the function
        // `delete_review` refuses with. Checked here rather than left to the removal below
        // so that a refusal leaves nothing behind — a `folded` update recorded against the
        // newest round of a fold that then refused would say a round was folded away while
        // it was still there.
        self.refuse_if_started(&review)?;

        // **Unless the two rounds are of the same commit**, which no `open_review` can
        // produce — it hands back the review of a head already reviewed — but a record
        // written by hand can hold. The table refuses a move from a commit to itself, and
        // the honest reading is that there is nothing to record: the request was already
        // where it is.
        if review.head_sha != newest.head_sha {
            self.record_head_update(
                newest.id,
                HeadUpdateKind::Folded,
                &review.head_sha,
                &newest.head_sha,
            )?;
        }

        self.remove_review(review.id)?;
        let into = self.get_review(newest.id)?;
        tx.commit()?;

        Ok(FoldedRound {
            folded: review,
            into,
        })
    }
}

/// A round, as the tidy's answer names it.
fn reference(review: &Review) -> RoundRef {
    RoundRef {
        review_id: review.id,
        round: review.round,
        head_sha: review.head_sha.clone(),
    }
}
