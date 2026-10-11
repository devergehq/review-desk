//! What a harvest leaves on step 7.
//!
//! **Only `post_draft` wrote step 7 until DEV-933**, and it writes `pending` with
//! "posted, waiting for the harvest" — so a review whose outcomes had been read back
//! every fifteen minutes since went on saying it was waiting for a harvest. Nothing in
//! `crates/harvest` wrote a layer at all.
//!
//! [`Store::record_harvest_step`] is what a harvest calls when it has finished reading
//! one review:
//!
//! | What the record now holds | Step 7 |
//! | -- | -- |
//! | The pull request is open and nothing has come back | `running`, "nothing back yet" |
//! | The pull request is open and something has | `running`, and the counts as facts |
//! | The pull request merged or closed | `done`, how it ended, and the same counts |
//!
//! **Every one of those records `Pull request` as a fact**, `open` until it is `merged` or
//! `closed`. It is a fact about the review rather than a device: an open pull request is
//! why a step is still `running`, and it is the pair the settled rows answer with
//! `merged` or `closed`.
//!
//! It was also, for one commit, what gave a harvested step a card at all — the trail drew
//! one only for a step with facts, and the card is where the page says when the harvest
//! last read the review, which the step's own line has no room for. **A running step no
//! longer depends on it**: the trail opens a card for a step with facts *or* a read time.
//! A settled one still does, because the read time is written only while the step is
//! running — see [`facts_for`].
//!
//! **The counts are read from the outcomes the record holds, not from what the run just
//! wrote.** Running a harvest twice records nothing twice — that is the whole of
//! `add_harvested_outcome` — so a step whose facts were the run's own writes would say
//! "2 replies" on the first harvest and "0 replies" on every one after it. Read from the
//! record, the facts agree with the outcomes the review page lists beside them, and a
//! second harvest that found nothing new computes the same words and so appends no second
//! state to the layer's record.
//!
//! **The time it read is the layer row's `updated_at`**, which [`Store::set_review_layer`]
//! moves on every call. It is not a fact holding a date: every other time this record
//! holds is an instant the screen words, and a date frozen into a fact's value would be
//! the one place a reader could be told "read 3 min ago" an hour later.

use rusqlite::params;

use crate::error::Result;
use crate::model::{
    NewLayerFact, OutcomeKind, PrState, ReviewLayer, ReviewLayerName, ReviewLayerState,
};
use crate::Store;

/// What step 7 says while the pull request is open and nothing has come back.
///
/// Not [`crate::entities::draft::AWAITING_HARVEST_DETAIL`], and the difference is the
/// point of DEV-933: that one means no harvest has read this review, and this one means
/// one has and GitHub had nothing to say yet.
pub(crate) const NOTHING_BACK_DETAIL: &str = "nothing back yet";

/// Which of step 7's three counts an outcome feeds, if any.
///
/// **One exhaustive match, and both ends of the query are driven off it**: the kinds the
/// statement asks for are the kinds this answers `Some` to, and the rows that come back
/// are added up by asking it again. A twelfth outcome kind stops this compiling, and
/// whatever arm it is given is the whole of the decision.
///
/// The first attempt at this kept a list of three kinds beside a match over eleven, and
/// the review of it said why that was not the same thing: exhaustiveness forces an *arm*,
/// not membership in the list. A kind added to the match and not to the list would be a
/// kind the query never asked for, and step 7 would say "nothing back yet" about outcomes
/// the harvester had just written — the one failure neither the types nor the SQL catch.
fn counts_toward(kind: OutcomeKind) -> Option<Count> {
    match kind {
        OutcomeKind::Replied => Some(Count::Replies),
        OutcomeKind::ThreadResolved => Some(Count::ThreadsResolved),
        OutcomeKind::CodeChangedAfter => Some(Count::FindingsOvertaken),
        // The developer's own outcomes. Named rather than swept up by a `_` so that a new
        // kind is a decision somebody makes instead of a count that stays at nought.
        OutcomeKind::Cut
        | OutcomeKind::Regraded
        | OutcomeKind::Rewritten
        | OutcomeKind::Kept
        | OutcomeKind::Rejected
        | OutcomeKind::Ticketed
        | OutcomeKind::Withdrawn
        | OutcomeKind::RaisedAgain => None,
    }
}

/// One of the three figures step 7 reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Count {
    /// Counted by row: one row is one reply.
    Replies,
    /// Counted by row: one row is one thread.
    ThreadsResolved,
    /// Counted by finding: a finding whose file two later commits touched is two rows
    /// about one finding.
    FindingsOvertaken,
}

impl Store {
    /// Record on step 7 what the harvest has just read, and when it read it.
    ///
    /// `Ok(None)` for a review nothing has posted: step 7 is the harvest of a posted
    /// review, and a sweep covers every review whose pull request is open — including one
    /// whose draft is still being written. Writing `running` on that would put two steps
    /// of the trail in progress at once and claim a feedback loop nobody had started.
    /// `github_review_id` is the record's own mark that the review is on GitHub, written
    /// by `post_draft` in the same transaction as step 7's `pending`.
    ///
    /// **Two statements rather than one transaction**, because
    /// [`Store::put_review_layer_facts`] opens its own — which is what
    /// `review-desk layer set --fact` has always done, in that order. **So this must not
    /// be called inside a transaction of the caller's**: SQLite refuses a transaction
    /// within a transaction, and it would refuse it at run time. The house pattern next
    /// door is the other way round — `post_draft` writes its layer row *in* the
    /// transaction it opened — so the difference is worth stating rather than discovering.
    /// What it costs: a facts write that fails after the layer write leaves step 7's line
    /// saying "merged, 3 replies back" with the previous harvest's pairs on its card,
    /// until the next sweep writes both again.
    ///
    /// **The facts are replaced whole**, so a fact something else wrote on step 7 does not
    /// survive a harvest. That is the bargain `put_review_layer_facts` documents: the list
    /// is the statement, and step 7's statement is the harvest's.
    pub fn record_harvest_step(&self, review_id: i64) -> Result<Option<ReviewLayer>> {
        let review = self.get_review(review_id)?;
        if review.github_review_id.is_none() {
            return Ok(None);
        }

        let found = self.harvested_so_far(review_id)?;
        let number = ReviewLayerName::Harvest.number();
        let ended = settled_as(review.pr_state);

        let layer = self.set_review_layer(
            review_id,
            number,
            if ended.is_some() {
                ReviewLayerState::Done
            } else {
                ReviewLayerState::Running
            },
            Some(&detail_for(ended, &found)),
        )?;
        let facts = self.put_review_layer_facts(review_id, number, facts_for(ended, &found))?;

        Ok(Some(ReviewLayer { facts, ..layer }))
    }

    /// What the record holds for this review of the three facts a harvest reads back.
    ///
    /// One query over the outcomes, asking for the kinds [`counts_toward`] answers for and
    /// adding each row up the way it says. **The kinds come from the enum, going in and
    /// coming out**, the way `rules_for_repo` builds its list of stages: three quoted
    /// words typed in here would be a second spelling of each kind, and the day one of
    /// them was renamed this would go on compiling and count nothing. The words are
    /// formatted in rather than bound for the reason that function gives — an
    /// `OutcomeKind` is this crate's own closed set, and no caller's string reaches the
    /// statement.
    fn harvested_so_far(&self, review_id: i64) -> Result<Found> {
        let kinds = OutcomeKind::ALL
            .iter()
            .filter(|kind| counts_toward(**kind).is_some())
            .map(|kind| format!("'{}'", kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");

        let mut statement = self.conn.prepare(&format!(
            "SELECT kind, COUNT(*) AS rows_held, COUNT(DISTINCT finding_seq) AS findings
             FROM outcomes
             WHERE review_id = ?1 AND kind IN ({kinds})
             GROUP BY kind"
        ))?;
        let counted = statement
            .query_map(params![review_id], |row| {
                Ok((
                    row.get::<_, OutcomeKind>("kind")?,
                    row.get::<_, i64>("rows_held")?,
                    row.get::<_, i64>("findings")?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut found = Found::default();
        for (kind, rows_held, findings) in counted {
            match counts_toward(kind) {
                Some(Count::Replies) => found.replies = rows_held,
                Some(Count::ThreadsResolved) => found.threads_resolved = rows_held,
                Some(Count::FindingsOvertaken) => found.findings_overtaken = findings,
                // Unreachable: the statement above asked for these kinds and no other.
                None => {}
            }
        }
        Ok(found)
    }

    /// Whether anything has moved step 7 past `pending`.
    ///
    /// What `post_draft` asks before writing "posted, waiting for the harvest" over it.
    /// Reporting the same GitHub review twice is a supported call, and step 7 is the one
    /// layer whose true state can be past what posting writes — so a session retrying
    /// after a crash must not throw a harvest's `running` back to `pending`.
    pub(crate) fn harvest_step_has_moved(&self, review_id: i64) -> Result<bool> {
        Ok(self
            .find_review_layer(review_id, ReviewLayerName::Harvest.number())?
            .is_some_and(|layer| layer.state != ReviewLayerState::Pending))
    }
}

/// What the record holds of what a harvest has read back for one review.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Found {
    replies: i64,
    threads_resolved: i64,
    /// Findings whose file a later commit changed, counted once each however many
    /// commits changed it.
    findings_overtaken: i64,
}

impl Found {
    /// The strongest fact, worded, or `None` when nothing has come back.
    ///
    /// **Strongest first, and one of them.** A detail is drawn on one line about 150
    /// pixels wide, and the limit the other steps keep is four words — so three counts
    /// in it would be a line nobody can read and a limit nobody keeps. The rest are the
    /// facts on the card beside the step.
    fn strongest(&self) -> Option<String> {
        if self.replies > 0 {
            return Some(format!(
                "{} {} back",
                self.replies,
                plural(self.replies, "reply", "replies")
            ));
        }
        if self.threads_resolved > 0 {
            return Some(format!(
                "{} {} resolved",
                self.threads_resolved,
                plural(self.threads_resolved, "thread", "threads")
            ));
        }
        if self.findings_overtaken > 0 {
            return Some("code changed since".to_string());
        }
        None
    }
}

/// How the pull request ended, or `None` while it is open.
///
/// A pull request nobody has looked at is treated as open: the harvest writes the state it
/// read before it calls this, so `None` here is a pull request that was open when it was
/// read rather than one nothing knows about.
fn settled_as(state: Option<PrState>) -> Option<&'static str> {
    match state {
        Some(PrState::Merged) => Some("merged"),
        Some(PrState::Closed) => Some("closed"),
        Some(PrState::Open) | None => None,
    }
}

/// The one line step 7 shows after its name.
///
/// Four words at most in every case this can produce, including a three-digit count,
/// which is the limit the other steps keep. The unit test below is what holds it.
fn detail_for(ended: Option<&'static str>, found: &Found) -> String {
    let what = found
        .strongest()
        .unwrap_or_else(|| NOTHING_BACK_DETAIL.to_string());
    match ended {
        Some(ended) => format!("{ended}, {what}"),
        None => what,
    }
}

/// The pairs the card beside step 7 draws.
///
/// Where the pull request stands first, because that is the strongest thing the step has
/// to say, then the counts. **A count of nothing is left out** rather than written as a
/// nought: "Replies 0" fills the card of a review nobody has replied to with three facts
/// that say nothing, and the detail already says nothing came back.
///
/// **Always at least that first pair.** "open" is not filler: on a step that is still
/// running it is the reason it is still running, and it is the pair the settled rows
/// answer with `merged` or `closed`.
///
/// **A running step no longer needs it to have a card; a settled one does.** The trail
/// opens a card for a step with facts *or* a read time, and the read time is there only
/// while the step is `running` — so taking this pair away would cost a merged step 7 with
/// every count at nought its card, and the duration on it, which is the one place the
/// page says how long the harvest ran.
fn facts_for(ended: Option<&'static str>, found: &Found) -> Vec<NewLayerFact> {
    let mut facts = vec![fact("Pull request", ended.unwrap_or("open").to_string())];
    if found.replies > 0 {
        facts.push(fact("Replies", found.replies.to_string()));
    }
    if found.threads_resolved > 0 {
        facts.push(fact("Threads resolved", found.threads_resolved.to_string()));
    }
    if found.findings_overtaken > 0 {
        facts.push(fact(
            "Code changed since",
            format!(
                "{} {}",
                found.findings_overtaken,
                plural(found.findings_overtaken, "finding", "findings")
            ),
        ));
    }

    facts
}

fn fact(label: &str, value: String) -> NewLayerFact {
    NewLayerFact {
        label: label.to_string(),
        value,
    }
}

fn plural(count: i64, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 {
        one
    } else {
        many
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::draft::AWAITING_HARVEST_DETAIL;

    /// Every step 7 this module can word, across the counts that change the wording.
    fn every_wording() -> Vec<(Option<&'static str>, Found)> {
        let counts = [
            Found::default(),
            Found {
                replies: 1,
                threads_resolved: 1,
                findings_overtaken: 1,
            },
            Found {
                replies: 2,
                threads_resolved: 3,
                findings_overtaken: 4,
            },
            Found {
                replies: 0,
                threads_resolved: 7,
                findings_overtaken: 0,
            },
            Found {
                replies: 0,
                threads_resolved: 0,
                findings_overtaken: 2,
            },
            // Three digits of each, which is where a detail that listed them all would
            // have broken the four-word limit.
            Found {
                replies: 128,
                threads_resolved: 256,
                findings_overtaken: 512,
            },
        ];

        let mut all = Vec::new();
        for ended in [None, Some("merged"), Some("closed")] {
            for found in counts {
                all.push((ended, found));
            }
        }
        all
    }

    #[test]
    fn no_step_seven_detail_runs_past_four_words() {
        for (ended, found) in every_wording() {
            let said = detail_for(ended, &found);
            let words = said.split_whitespace().count();
            assert!(
                words <= 4,
                "{words} words in {said:?}, and a step's detail keeps to four"
            );
        }
    }

    #[test]
    fn no_step_seven_records_more_than_five_facts() {
        for (ended, found) in every_wording() {
            let recorded = facts_for(ended, &found);
            assert!(
                recorded.len() <= 5,
                "{} facts for {ended:?} {found:?}, and a step keeps to five",
                recorded.len()
            );
            for pair in &recorded {
                assert!(!pair.label.trim().is_empty(), "a fact with no label");
                assert!(!pair.value.trim().is_empty(), "a fact with no value");
            }
        }
    }

    #[test]
    fn a_harvest_that_found_nothing_says_so_and_still_records_where_it_stands() {
        assert_eq!(detail_for(None, &Found::default()), "nothing back yet");

        let recorded = facts_for(None, &Found::default());
        assert_eq!(
            recorded
                .iter()
                .map(|fact| (fact.label.as_str(), fact.value.as_str()))
                .collect::<Vec<_>>(),
            vec![("Pull request", "open")],
            "where the pull request stands, and nothing invented beside it"
        );
    }

    #[test]
    fn the_strongest_fact_is_the_one_in_the_detail() {
        let replies_and_more = Found {
            replies: 2,
            threads_resolved: 3,
            findings_overtaken: 4,
        };
        assert_eq!(detail_for(None, &replies_and_more), "2 replies back");

        let no_replies = Found {
            replies: 0,
            threads_resolved: 3,
            findings_overtaken: 4,
        };
        assert_eq!(detail_for(None, &no_replies), "3 threads resolved");

        let only_commits = Found {
            replies: 0,
            threads_resolved: 0,
            findings_overtaken: 4,
        };
        assert_eq!(detail_for(None, &only_commits), "code changed since");
    }

    #[test]
    fn one_of_each_is_written_in_the_singular() {
        let one = Found {
            replies: 1,
            threads_resolved: 1,
            findings_overtaken: 1,
        };
        assert_eq!(detail_for(None, &one), "1 reply back");

        let resolved = Found {
            threads_resolved: 1,
            ..Found::default()
        };
        assert_eq!(detail_for(None, &resolved), "1 thread resolved");

        let overtaken = Found {
            findings_overtaken: 1,
            ..Found::default()
        };
        assert_eq!(
            facts_for(None, &overtaken)
                .last()
                .map(|fact| fact.value.clone()),
            Some("1 finding".to_string())
        );
    }

    #[test]
    fn a_settled_pull_request_says_how_it_ended_in_both_places() {
        let found = Found {
            replies: 2,
            threads_resolved: 2,
            findings_overtaken: 0,
        };

        assert_eq!(detail_for(Some("merged"), &found), "merged, 2 replies back");
        assert_eq!(detail_for(Some("closed"), &found), "closed, 2 replies back");

        let recorded = facts_for(Some("merged"), &found);
        assert_eq!(recorded[0].label, "Pull request");
        assert_eq!(recorded[0].value, "merged");
        assert_eq!(
            recorded
                .iter()
                .map(|fact| fact.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Pull request", "Replies", "Threads resolved"],
            "and a count of nothing is left out rather than written as a nought"
        );
    }

    #[test]
    fn an_unread_pull_request_is_worded_as_an_open_one() {
        // The harvest writes the state it read before it words the step, so `None` here
        // is a pull request that was open, not one nothing has looked at.
        assert_eq!(settled_as(None), None);
        assert_eq!(settled_as(Some(PrState::Open)), None);
        assert_eq!(settled_as(Some(PrState::Merged)), Some("merged"));
        assert_eq!(settled_as(Some(PrState::Closed)), Some("closed"));

        // And an open one says so on the card, which is why the step is still running.
        assert_eq!(
            facts_for(None, &Found::default())
                .first()
                .map(|fact| (fact.label.clone(), fact.value.clone())),
            Some(("Pull request".to_string(), "open".to_string()))
        );
    }

    #[test]
    fn the_kinds_a_harvest_counts_are_the_kinds_it_writes() {
        // Named here so that a kind added to `counts_toward` — or taken out of it — is a
        // line in a diff rather than a figure that quietly changes on the page.
        let counted: Vec<&str> = OutcomeKind::ALL
            .iter()
            .filter(|kind| counts_toward(**kind).is_some())
            .map(|kind| kind.as_str())
            .collect();

        assert_eq!(
            counted,
            vec!["replied", "thread_resolved", "code_changed_after"],
            "the three a harvest writes, and nothing the developer writes by hand"
        );
    }

    #[test]
    fn nothing_back_is_not_what_posting_writes() {
        assert_ne!(
            NOTHING_BACK_DETAIL, AWAITING_HARVEST_DETAIL,
            "one means a harvest has read this review and the other means none has; a \
             reader who cannot tell them apart is DEV-933"
        );
    }
}
