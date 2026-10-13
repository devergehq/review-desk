//! The shapes on the wire, and the two conversions the contract asks the server to make.
//!
//! **Times.** The store holds `Timestamp = i64`, seconds since the Unix epoch; the
//! contract is RFC 3339 in UTC with a `Z` offset. `contract/README.md` names this as the
//! one mismatch that will look like a bug the first time somebody serialises a store
//! record straight out, so the conversion lives here and no handler formats a time
//! itself.
//!
//! **Display keys.** `F1` and `R-09` are derived, not stored — `F` plus the finding's
//! `seq`, and `R-` plus the rule's id zero-padded to two digits. The contract has the
//! server derive them so the dashboard never computes an identifier.
//!
//! Bodies are built as `serde_json::Value` with every field written out in one place.
//! The contract sets `additionalProperties: false` on every object, so a mistyped field
//! name is a validation failure in the conformance test rather than a field the
//! dashboard silently never sees.

use review_desk_store::{
    finding_heading, AcceptanceCriterion, BlindPassState, Brief, BriefBlindPass, BriefDisagreement,
    BriefFlowBox, BriefFlowRow, BriefOpenChoice, BriefOption, BriefOptionVerdict, BriefPart,
    BriefPartFile, BriefProblem, BriefSupportTable, DescriptionLintResult, Disposition, Draft,
    DraftLint, DraftSection, DraftThread, FileChange, Finding, FindingHunk, FindingInRound,
    HeadUpdate, HeadUpdates, LayerFact, LeftTheDesk, NitTicket, NotForSigning, Outcome,
    PostedReview, ProblemStatement, RepoSummary, Review, ReviewAtStep, ReviewLayer,
    ReviewLayerName, ReviewLayerRun, ReviewProvenance, ReviewSource, ReviewTimings, RoundCommit,
    RoundStatusCounts, RoundSummary, Rule, RuleCounts, RuleScope, RuleStage, RulesSummary,
    Severity, Timestamp,
};
use serde_json::{json, Value};

use crate::error::ApiError;

/// A time on the wire: RFC 3339, UTC, `Z` offset.
pub fn rfc3339(seconds: Timestamp) -> String {
    // Floor division, so a time before 1970 does not round towards zero and land on the
    // wrong day. Review Desk will not hold one, but a clock set wrong can produce one
    // and a date that is silently off by a day is a bad way to find out.
    let days = seconds.div_euclid(86_400);
    let within = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (within / 3600, (within % 3600) / 60, within % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// An optional time on the wire: the string, or JSON null.
pub fn rfc3339_or_null(seconds: Option<Timestamp>) -> Value {
    seconds.map_or(Value::Null, |seconds| json!(rfc3339(seconds)))
}

/// The civil date for a count of days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`, which is exact for the whole range of `i64` days
/// and needs no table. Written out rather than taken from `chrono` or `time`: one date
/// conversion does not justify a dependency in a crate whose whole argument for its HTTP
/// stack is dependency weight.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// A finding's display key: `F` plus its sequence number.
pub fn finding_key(seq: i64) -> String {
    format!("F{seq}")
}

/// The contract's `Review`.
///
/// **`pr_state_at` is here since DEV-957, and it was deliberately absent before.** The
/// argument for leaving it out was that nothing drew it; the draft page now does — "Not
/// posted — the pull request merged on 11 October 2026" is that column and nothing else
/// can date it. It is still only a date when GitHub gave one, and the sentence says the
/// pull request merged without naming the day when it did not.
pub fn review(review: &Review) -> Value {
    json!({
        "id": review.id,
        "repo": review.repo,
        "pr": review.pr,
        "head_sha": review.head_sha,
        "round": review.round,
        "title": review.title,
        "ticket_id": review.ticket_id,
        "ticket_url": review.ticket_url,
        "changed_lines": review.changed_lines,
        "session_id": review.session_id,
        "ledger_id": review.ledger_id,
        "github_review_id": review.github_review_id,
        "created_at": rfc3339(review.created_at),
        "pr_state": review.pr_state.map(|state| state.as_str()),
        "pr_state_at": rfc3339_or_null(review.pr_state_at),
        "harvested_at": rfc3339_or_null(review.harvested_at),
        // When and why the developer dismissed it, both null while it is in play. On the
        // review itself rather than on the list item, because every screen that can draw
        // a review has to be able to say it is dismissed — the page it is dismissed from
        // reads `GET /reviews/{id}`, and the list reads `GET /reviews`.
        "dismissed_at": rfc3339_or_null(review.dismissed_at),
        "dismissed_reason": review.dismissed_reason,
        // Why its pull request is out of play — `draft` or `withdrawn` — and when, both
        // null while it is in play. Beside the dismissal for the same reason: every screen
        // that can draw a review has to be able to say the review is paused, and the
        // sentence it draws names which of the two it is. It is reversible, so nothing
        // here refuses an act on the strength of it.
        "out_of_play": review.out_of_play.map(|why| why.as_str()),
        "out_of_play_at": rfc3339_or_null(review.out_of_play_at),
    })
}

/// The contract's `InboxPostedItem`: a posted review whose pull request is still open.
///
/// **Where the three counts are decided**, and the one thing this function is really
/// for. Each is null when it would be zero *and* nothing could have found one, because
/// zero is a measurement and the absence of one is not:
///
/// | The record | What is served |
/// |---|---|
/// | nothing has harvested the review, and it holds none of these outcomes | null, and the screen says "not harvested yet" |
/// | harvested, but no finding carries the comment it was posted as | null for the two counts read off a thread — there was nothing for a reply to attach to |
/// | harvested, something to read, nothing found | `0`, which is a measurement |
/// | an outcome the record holds, with nothing having harvested it | the count — the harvest writes its outcomes and its own timestamp in separate statements, and `outcome add` writes outcomes with no harvest at all |
///
/// The last row is the converse failure and it mattered: `reviews_with_author_activity`
/// has no `harvested_at` condition, so the same screen was about to draw those replies
/// under "Needs you" while this group said nobody had measured whether the author had
/// replied.
pub fn inbox_posted(posted: &PostedReview) -> Value {
    let review = &posted.review;
    let harvested = review.harvested_at.is_some();

    // Zero with a reason to doubt it is served as "not measured". A count the record
    // holds is served whatever else is missing: something came back, however it arrived.
    let counted = |count: i64, could_look: bool| {
        if count > 0 || could_look {
            json!(count)
        } else {
            Value::Null
        }
    };
    // A thread is reached through a finding's GitHub comment id. With none, the
    // harvester reads no thread at all and marks the review harvested regardless.
    let threads_readable = harvested && posted.findings_linked > 0;

    json!({
        "review_id": review.id,
        "pull_request": inbox_pr(review),
        // The ticket and its address, for the reason the in-progress item carries them: a
        // posted review whose author has not answered *may* be in no other array, and it
        // then draws its own row, which needs to reach its ticket. It may equally be joined
        // onto a needs-you row — review 12 in `getInbox.response.json` is in
        // `author_replies` and here at once — so "is in no other array" would say what the
        // `Inbox` schema calls false. The same correction `api.rs` took, which left this
        // copy behind.
        "ticket_id": review.ticket_id,
        "ticket_url": review.ticket_url,
        "round": review.round,
        // **The same number as `round`, and read here rather than passed in.** The store
        // returns only the newest round of a pull request, so the round this review is on
        // *is* the highest that pull request has reached. Served because the contract's
        // field exists so no screen has to know that; taken as a parameter it would have
        // been a value only one caller could get right, with the reason it is equal
        // living at that caller rather than here.
        "rounds_on_pr": review.round,
        "posted_at": rfc3339_or_null(posted.posted_at),
        "posted_as": posted.posted_as.map(|event| event.as_str()),
        "pr_state": review.pr_state.map(|state| state.as_str()),
        "harvested_at": rfc3339_or_null(review.harvested_at),
        "findings": posted.findings,
        "findings_linked": posted.findings_linked,
        "replies": counted(posted.response.replies, threads_readable),
        "threads_resolved": counted(posted.response.threads_resolved, threads_readable),
        "code_changed_after": counted(posted.response.code_changed_after, harvested),
    })
}

/// The contract's `ReviewListItem`: one row of `GET /reviews`.
///
/// The step's three fields are absent together, for a review nothing has reported on at
/// all. Absent rather than a number: "nothing has started" and "the lint is pending" are
/// different sentences, and the page draws the first in words.
pub fn review_list_item(at: &ReviewAtStep) -> Value {
    let mut item = json!({
        "review": review(&at.review),
        "rounds_on_pr": at.rounds_on_pr,
        "steps_total": ReviewLayerName::COUNT,
        "last_activity_at": rfc3339_or_null(at.review.last_activity_at),
        // **How it left the desk, or null while it is in play**, and the row says which.
        // DEV-957 asks that dismissed and finished be told apart everywhere a review is
        // listed, and the row cannot work this out for itself: a dismissal is a column on
        // the review, but a finish is a fact about the pull request read off whichever
        // round of it a harvest last reached — which a page holding one row has not got.
        "left_the_desk": at.left_the_desk.map(|left| match left {
            LeftTheDesk::Dismissed => "dismissed",
            LeftTheDesk::Finished => "finished",
            // **The queue, not this list.** A pull request out of play is coming back, so
            // its row stays under In play and says this instead of disappearing — the one
            // row that explains where the review went must not be the row nobody can find.
            // `Review.out_of_play` beside it carries which of the two reasons it is.
            LeftTheDesk::OutOfPlay => "out_of_play",
            LeftTheDesk::Superseded => "superseded",
        }),
        // Which way it ended and when, on a finished row only. Read off the pull request
        // rather than off this round's own column, which holds nothing for three of the
        // four rounds of a merged pull request. Null when GitHub gave no time, and the row
        // then says it merged without naming the day.
        "finished_how": at.finished_how.map(|state| state.as_str()),
        "finished_at": rfc3339_or_null(at.finished_at),
    });

    // `if let`, not `as_object_mut().expect(...)`: `finding_in_round` below grows a
    // document the same way and without a panic path, and two idioms for one operation in
    // one file is how the one that can panic ends up copied. The match always holds — the
    // `json!` above is an object literal — which is exactly why nothing should panic if it
    // somehow did not.
    if let (Some(step), Value::Object(object)) = (&at.step, &mut item) {
        object.insert("step_number".to_string(), json!(step.number));
        object.insert("step_name".to_string(), json!(step.name.as_str()));
        object.insert("step_state".to_string(), json!(step.state.as_str()));
        object.insert("step_detail".to_string(), json!(step.detail));
    }

    item
}

/// The contract's `NotForSigning`: why a draft may not be signed off, or null.
///
/// **The page's sentence and the API's refusal come off one value**, which is what stops
/// a screen offering a sign-off the API refuses. The three reasons carry what the sentence
/// needs and nothing more: the merge needs the word GitHub used and the day, and a
/// supersession needs the round that can still post.
pub fn not_for_signing(why: Option<NotForSigning>) -> Value {
    match why {
        None => Value::Null,
        Some(NotForSigning::Dismissed) => json!({ "reason": "dismissed" }),
        Some(NotForSigning::Finished { how, at }) => json!({
            "reason": "finished",
            "how": how.as_str(),
            "at": rfc3339_or_null(at),
        }),
        Some(NotForSigning::Superseded { by_round }) => json!({
            "reason": "superseded",
            "by_round": by_round,
        }),
    }
}

/// The contract's `BriefOption`.
pub fn brief_option(option: &BriefOption) -> Value {
    json!({
        "key": option.key,
        "title": option.title,
        "argument_for": option.argument_for,
        "argument_against": option.argument_against,
        "chosen": option.chosen,
        "proposed_by": option.proposed_by.as_str(),
        "verdicts": option
            .verdicts
            .iter()
            .map(brief_option_verdict)
            .collect::<Vec<_>>(),
    })
}

/// The contract's `BriefOptionVerdict`: what one option does about one numbered problem.
pub fn brief_option_verdict(verdict: &BriefOptionVerdict) -> Value {
    json!({
        "problem": verdict.problem,
        "verdict": verdict.verdict.as_str(),
        "why": verdict.why,
    })
}

/// One box of a flow.
pub fn brief_flow_box(flow_box: &BriefFlowBox) -> Value {
    json!({
        "title": flow_box.title,
        "note": flow_box.note,
        "problem": flow_box.problem,
    })
}

/// One flow, as rows of one to three boxes.
pub fn brief_flow(flow: &[BriefFlowRow]) -> Value {
    json!(flow
        .iter()
        .map(|row| row.iter().map(brief_flow_box).collect::<Vec<_>>())
        .collect::<Vec<_>>())
}

/// One numbered problem, with its detail.
pub fn brief_problem(problem: &BriefProblem) -> Value {
    json!({
        "number": problem.number,
        "label": problem.label,
        "was_wrong": problem.was_wrong,
        "now_fixed": problem.now_fixed,
        "detail_title": problem.detail_title,
        "detail_before": problem.detail_before,
        "detail_after": problem.detail_after,
    })
}

/// The one small supporting table.
pub fn brief_support_table(table: &BriefSupportTable) -> Value {
    json!({
        "title": table.title,
        "columns": table.columns,
        "rows": table.rows,
    })
}

/// The blind pass a session reported running.
///
/// `state` is derived by the store from this and the options together, and is served
/// beside it so the page never has to work out which of the three states it is drawing.
pub fn brief_blind_pass(pass: &BriefBlindPass) -> Value {
    json!({
        "given": pass.given,
        "pick_key": pass.pick_key,
        "pick_why": pass.pick_why,
        "looked_up": pass.looked_up,
        "questions": pass.questions,
    })
}

/// One part of the change.
pub fn brief_part(part: &BriefPart) -> Value {
    json!({
        "title": part.title,
        "summary": part.summary,
        "one_line": part.one_line,
        "fixes": part.fixes,
        "files": part.files.iter().map(brief_part_file).collect::<Vec<_>>(),
        "lines_added": part.files.iter().map(|file| file.lines_added).sum::<i64>(),
        "lines_removed": part
            .files
            .iter()
            .map(|file| file.lines_removed)
            .sum::<i64>(),
    })
}

/// One file of a part.
pub fn brief_part_file(file: &BriefPartFile) -> Value {
    json!({
        "path": file.path,
        "lines_added": file.lines_added,
        "lines_removed": file.lines_removed,
        "in_diff": file.in_diff,
    })
}

/// One choice the ticket left open.
pub fn brief_open_choice(choice: &BriefOpenChoice) -> Value {
    json!({
        "left_open": choice.left_open,
        "chosen": choice.chosen,
        "why": choice.why,
        "departs_from_ticket": choice.departs_from_ticket,
        "finding_seq": choice.finding_seq,
        // Derived here, like every other display key, so the dashboard never builds an
        // identifier of its own.
        "finding_key": choice
            .finding_seq
            .map_or(Value::Null, |seq| json!(finding_key(seq))),
    })
}

/// One place the description and the code disagree.
pub fn brief_disagreement(clash: &BriefDisagreement) -> Value {
    json!({
        "description_says": clash.description_says,
        "code_does": clash.code_does,
        "finding_seq": clash.finding_seq,
        "finding_key": finding_key(clash.finding_seq),
    })
}

/// The contract's `ProblemStatement`: the problem half of a brief and nothing else.
///
/// **Five fields, listed here as they are listed in the store.** The store builds the
/// statement by naming what belongs in it rather than by removing what does not, and this
/// does the same, so a field added to the brief cannot reach the wire through here by
/// accident.
pub fn problem_statement(statement: &ProblemStatement) -> Value {
    json!({
        "review_id": statement.review_id,
        "problem": statement.problem,
        "how_it_works_today": statement.how_it_works_today,
        "flow_before_caption": statement.flow_before_caption,
        "flow_before": brief_flow(&statement.flow_before),
        /*
         * **Two fields, and `label` is deliberately not the third.** The problem statement
         * is what a session may hand to a pass that has not seen the answer, and every
         * absence from it is argued in `openapi.yaml`. A label names a problem for a column
         * heading; a blind pass reads sentences, not headings, so adding it would widen
         * what travels without anything asking for it. DEV-892 added the field for the
         * Alternatives page and left this projection where it was.
         */
        "problems": statement
            .problems
            .iter()
            .map(|problem| {
                json!({ "number": problem.number, "was_wrong": problem.was_wrong })
            })
            .collect::<Vec<_>>(),
    })
}

/// The contract's `Brief`.
///
/// `options` comes from the `brief_option` table, not from `Brief::option_map`. The store
/// still holds that opaque string so migration 4 did not destroy what a review had
/// already written, but it is four structured fields on the wire and nothing can turn one
/// into the other — so a brief written before the migration serves an empty list, which
/// is true, rather than a guess.
///
/// **The options come off the brief rather than as a second argument.** The store
/// populates `Brief::options` on every read, and since DEV-882 each option carries its
/// verdicts — so a second argument was a second read of the same rows, and a chance to
/// serve one brief's options under another brief's fields.
pub fn brief(brief: &Brief) -> Value {
    json!({
        "review_id": brief.review_id,
        "problem": brief.problem,
        "diagram": brief.diagram,
        "options": brief.options.iter().map(brief_option).collect::<Vec<_>>(),
        "approach_verdict": brief.approach_verdict,
        "provenance": brief.provenance,
        "confirmed": brief.is_confirmed(),
        "confirmed_at": rfc3339_or_null(brief.confirmed_at),
        // Two fields rather than one, because "the developer has not confirmed it" and
        // "nobody has to" are different states and the screen renders them differently.
        // `confirmed` false with `confirmation_required` false is a brief that is
        // finished with and that nobody signed.
        "confirmation_required": brief.confirmation_required,
        "confirmation_not_required_because": brief.confirmation_not_required_because,
        "headline": brief.headline,
        "how_it_works_today": brief.how_it_works_today,
        "flow_before_caption": brief.flow_before_caption,
        "flow_after_caption": brief.flow_after_caption,
        "flow_before": brief_flow(&brief.flow_before),
        "flow_after": brief_flow(&brief.flow_after),
        "problems": brief.problems.iter().map(brief_problem).collect::<Vec<_>>(),
        "support_table": brief
            .support_table
            .as_ref()
            .map_or(Value::Null, brief_support_table),
        "blind_pass": brief
            .blind_pass
            .as_ref()
            .map_or(Value::Null, brief_blind_pass),
        // **Derived by the store from the record, never stored.** A page drawing the
        // blind column asks one question — did it run, did it find anything — and
        // answering it here is what stops `blind_pass` being null beside a state that
        // says it ran.
        "blind_pass_state": match brief.blind_pass_state() {
            BlindPassState::NotRun => "not_run",
            BlindPassState::RanAndFoundNothing => "ran_and_found_nothing",
            BlindPassState::Ran => "ran",
        },
        "parts": brief.parts.iter().map(brief_part).collect::<Vec<_>>(),
        "open_choices": brief
            .open_choices
            .iter()
            .map(brief_open_choice)
            .collect::<Vec<_>>(),
        "disagreements": brief
            .disagreements
            .iter()
            .map(brief_disagreement)
            .collect::<Vec<_>>(),
        "updated_at": rfc3339(brief.updated_at),
    })
}

/// The contract's `FindingHunk`, with the number the dashboard draws beside each line.
///
/// **The numbers are derived here and stored nowhere.** `FindingHunk::numbered_lines` is
/// the derivation and the store owns it: counting from `start_line`, an added or unchanged
/// line takes the next number and a removed line takes none, because it is not in the file
/// any more. Derived at this boundary for the reason `key` and `rule_key` are — the
/// dashboard renders what it is given and computes no numbers of its own.
///
/// **Nothing here touches the text.** Review Desk does not fetch code, so a hunk line is
/// the only copy of that line there is; it is serialised exactly as it was stored, leading
/// whitespace, trailing whitespace and all.
pub fn finding_hunk(hunk: &FindingHunk) -> Value {
    json!({
        "file": hunk.file,
        "start_line": hunk.start_line,
        "lines": hunk
            .numbered_lines()
            .into_iter()
            .map(|(number, line)| json!({
                "marker": line.marker.as_str(),
                "line": number,
                "text": line.text,
            }))
            .collect::<Vec<_>>(),
    })
}

/// The contract's `Finding`, without the three fields derived from the carry chain.
///
/// `continues_round`, `rounds_open` and `late` are left out rather than guessed at: each
/// needs a read this function has no store to make. The contract declares all three
/// nullable and not required, and says this operation serves them — `getReview` does,
/// through [`finding_in_round`].
pub fn finding(finding: &Finding) -> Value {
    json!({
        "review_id": finding.review_id,
        "seq": finding.seq,
        "key": finding_key(finding.seq),
        "lens": finding.lens,
        "severity": finding.severity.as_str(),
        "file": finding.file,
        "line": finding.line,
        "claim": finding.claim,
        // Null when nothing has titled this finding, which is every finding written before
        // migration 11. The page clamps the claim to two lines instead; a title defaulted
        // from the claim here would be a 56-word "title" the page could not tell apart
        // from a real one.
        "title": finding.title,
        "evidence": finding.evidence,
        "disposition": finding.disposition.as_str(),
        "disposition_reason": finding.disposition_reason,
        // The comment the finding was posted as, which is the only route anything can come
        // back on it by. Null for a finding raised in the body alone, which is what
        // `InboxPostedItem.findings_linked` counts and why it has to.
        "github_comment_id": finding.github_comment_id,
        "rule_id": finding.rule_id,
        "rule_key": finding.rule_id.map(rule_key),
        // Null, not an empty hunk. A finding written with only a file and a line has no
        // hunk, and `FindingHunk` requires a file, a start line and at least one line —
        // so there is no shape of the object that means "none", and inventing one would
        // be the contract's own rule against a made-up value, broken here.
        "hunk": finding
            .hunk
            .as_ref()
            .map_or(Value::Null, finding_hunk),
        "flagged_line": finding.flagged_line,
        "inline_comment": finding.inline_comment,
        "why_it_matters": finding.why_it_matters,
        "proof": finding.proof,
        "proof_note": finding.proof_note,
        // Always written, and `unknown` is a value rather than an absence: a finding that
        // said nothing about its method has said nothing, not that it was read and not run.
        "verification": finding.verification.as_str(),
        "suggested_fix": finding.suggested_fix,
        // Empty, not null. A finding with no sibling references has none, which is a
        // measurement; the contract's "null is not zero" rule cuts this way round too.
        "fix_references": finding.fix_references,
        "created_at": rfc3339(finding.created_at),
        "round_status": finding.round_status.as_str(),
        "round_status_by": finding.round_status_by.map(|by| by.as_str()),
        "round_status_checked": finding.round_status_checked,
        "round_status_at": rfc3339_or_null(finding.round_status_at),
        "round_status_commit": finding.round_status_commit,
        "continues_review_id": finding.continues_review_id,
        "introduced_by_commit": finding.introduced_by_commit,
        "introduced_by_seq": finding.introduced_by_seq,
        "introduced_by_key": finding.introduced_by_seq.map(finding_key),
    })
}

/// The contract's `Finding` whole: the row, plus the three facts derived from the carry
/// chain and the round's commits.
///
/// What `GET /reviews/{reviewId}` serves. The three are derived rather than stored so they
/// cannot drift from the chain they come from, which is also why they are computed in the
/// store and merely copied here.
pub fn finding_in_round(found: &FindingInRound) -> Value {
    let mut document = finding(&found.finding);

    // `finding` above builds an object literal, so this always matches. Adding to it
    // rather than writing the whole object out twice keeps the two shapes from drifting:
    // a field added to `Finding` reaches both.
    if let Value::Object(object) = &mut document {
        object.insert("continues_round".to_string(), json!(found.continues_round));
        object.insert("rounds_open".to_string(), json!(found.rounds_open));
        object.insert("late".to_string(), json!(found.late));
    }

    document
}

/// The contract's `RoundStatusCounts`.
pub fn round_status_counts(counts: &RoundStatusCounts) -> Value {
    json!({
        "new": counts.new,
        "fixed": counts.fixed,
        "still_open": counts.still_open,
        "accepted": counts.accepted,
        "withdrawn": counts.withdrawn,
        "ticketed": counts.ticketed,
    })
}

/// The contract's `RoundSummary`.
pub fn round_summary(round: &RoundSummary) -> Value {
    json!({
        "review_id": round.review_id,
        "round": round.round,
        "head_sha": round.head_sha,
        "created_at": rfc3339(round.created_at),
        // **The one fact on a round that is not a count.** Every other field here looks
        // the same for a round the developer took out of play as for a live one, which is
        // why the timeline was drawing a dismissed round as live (DEV-954).
        "dismissed": round.dismissed,
        "posted_at": rfc3339_or_null(round.posted_at),
        "event": round.event.map(|event| event.as_str()),
        "findings": round.findings,
        "commits": round.commits,
        "replies": round.replies,
        "threads_resolved": round.threads_resolved,
        "statuses": round_status_counts(&round.statuses),
    })
}

/// The contract's `HeadUpdates`: what has happened to this round's head commit.
///
/// **The sentence and the evidence under it.** `since_sha` with `commits` is "Updated for 3
/// commits since `4d9464f`"; `base_only` is "2 commits brought the base branch in and
/// changed nothing"; and `updates` is each one with its time, which is what lets the code
/// review show what changed since the draft was written. `commits` is null rather than zero
/// when the round's commits were never recorded — see the store, which will not count what
/// nobody read.
pub fn head_updates(updates: &HeadUpdates) -> Value {
    json!({
        "since_sha": updates.since_sha,
        "moved": updates.moved,
        "base_only": updates.base_only,
        "folded": updates.folded,
        "commits": updates.commits,
        "last_at": rfc3339_or_null(updates.last_at),
        "updates": updates
            .updates
            .iter()
            .map(head_update)
            .collect::<Vec<_>>(),
    })
}

/// The contract's `HeadUpdate`: one thing that happened to a round's head.
fn head_update(update: &HeadUpdate) -> Value {
    json!({
        "kind": update.kind.as_str(),
        "from_sha": update.from_sha,
        "to_sha": update.to_sha,
        "at": rfc3339(update.at),
    })
}

/// The contract's `RoundCommit`.
pub fn round_commit(commit: &RoundCommit) -> Value {
    json!({
        "sha": commit.sha,
        "position": commit.position,
        "summary": commit.summary,
        "committed_at": rfc3339_or_null(commit.committed_at),
        "files": commit.files,
    })
}

/// The contract's `Outcome`.
pub fn outcome(outcome: &Outcome) -> Value {
    json!({
        "id": outcome.id,
        "review_id": outcome.review_id,
        "finding_seq": outcome.finding_seq,
        "finding_key": finding_key(outcome.finding_seq),
        "kind": outcome.kind.as_str(),
        "actor": outcome.actor.as_str(),
        "occurred_at": rfc3339(outcome.occurred_at),
        "detail": outcome.detail,
    })
}

/// The contract's `ReviewLayer`.
pub fn review_layer(layer: &ReviewLayer) -> Value {
    json!({
        "number": layer.number,
        "name": layer.name.as_str(),
        "state": layer.state.as_str(),
        "detail": layer.detail,
        "updated_at": rfc3339_or_null(layer.updated_at),
        // **The passes, on the layer they belong to.** A second list of the same figures
        // beside `layers` is a second answer a page could render, and the contract's rule
        // about two coverage records applies here for the same reason.
        "runs": layer.runs.iter().map(review_layer_run).collect::<Vec<_>>(),
        "seconds": layer.seconds(),
        // **What the step came to, as pairs rather than as a sentence.** `detail` is one
        // line and the trail draws it on one line; these are what the card beside the
        // step holds. Empty for a step that recorded none, which draws no card at all —
        // unless it is step 7 and `running`, whose card holds `updated_at` worded as when
        // the harvest last read the review.
        "facts": layer.facts.iter().map(layer_fact).collect::<Vec<_>>(),
    })
}

/// The contract's `LayerFact`: one label, one value.
pub fn layer_fact(fact: &LayerFact) -> Value {
    json!({
        "label": fact.label,
        "value": fact.value,
    })
}

/// The contract's `ReviewLayerRun`: one pass a step made.
///
/// Every duration is whole seconds and the store derives it. Formatting one as "17 min" is
/// the dashboard's decision, and a server that had made it would have decided it.
pub fn review_layer_run(run: &ReviewLayerRun) -> Value {
    json!({
        "state": run.state.map(|state| state.as_str()),
        "started_at": rfc3339_or_null(run.started_at),
        "ended_as": run.ended_as.map(|state| state.as_str()),
        "finished_at": rfc3339_or_null(run.finished_at),
        "seconds": run.seconds(),
        "detail": run.detail,
    })
}

/// The contract's `ReviewTimings`: how long the review took, as totals.
///
/// **The per-step figures are not here.** They are on `layers[]`, with the passes they
/// were computed from.
pub fn review_timings(timings: &ReviewTimings) -> Value {
    json!({
        "requested_at": rfc3339(timings.requested_at),
        "draft_ready_at": rfc3339_or_null(timings.draft_ready_at),
        "signed_off_at": rfc3339_or_null(timings.signed_off_at),
        "posted_at": rfc3339_or_null(timings.posted_at),
        "with_the_agent_seconds": timings.with_the_agent_seconds,
        "waiting_on_you_seconds": timings.waiting_on_you_seconds,
        "last_activity_at": rfc3339_or_null(timings.last_activity_at),
    })
}

/// The contract's `SeverityCounts`.
pub fn severity_counts(findings: &[Finding]) -> Value {
    let count = |wanted: Severity| {
        findings
            .iter()
            .filter(|finding| finding.severity == wanted)
            .count()
    };
    json!({
        "blocker": count(Severity::Blocker),
        "should": count(Severity::Should),
        "nit": count(Severity::Nit),
        "question": count(Severity::Question),
    })
}

/// The contract's `DispositionCounts`.
pub fn disposition_counts(findings: &[Finding]) -> Value {
    let count = |wanted: Disposition| {
        findings
            .iter()
            .filter(|finding| finding.disposition == wanted)
            .count()
    };
    json!({
        "in_draft": count(Disposition::InDraft),
        "held_back": count(Disposition::HeldBack),
        "suppressed": count(Disposition::Suppressed),
    })
}

/// The contract's `NitTicket`.
pub fn nit_ticket(ticket: &NitTicket) -> Value {
    json!({
        "review_id": ticket.review_id,
        "status": ticket.status.as_str(),
        "ticket_id": ticket.ticket_id,
        "finding_seqs": ticket.finding_seqs,
        "requested_at": rfc3339_or_null(ticket.requested_at),
        "filed_at": rfc3339_or_null(ticket.filed_at),
    })
}

/// The contract's `DraftLint`, or null when the lint has not run.
///
/// Null rather than a `DraftLint` with every field absent. `passes` and `checked_at` are
/// required by the schema, so there is no shape of the object that means "not measured" —
/// serialising one would mean inventing `passes: false` and a time, and a failing lint is
/// not an absent one. The contract was changed in this pull request to allow the null.
pub fn draft_lint(lint: &DraftLint) -> Value {
    if lint.is_empty() || lint.passes.is_none() || lint.checked_at.is_none() {
        return Value::Null;
    }

    json!({
        "passes": lint.passes,
        "visible_words": lint.visible_words,
        "word_budget": lint.word_budget,
        "findings_in_body": lint.findings_in_body,
        "findings_expected": lint.findings_expected,
        "raised_before_on_pr": lint.raised_before_on_pr,
        // The store holds what the lint printed, as one string; the contract wants the
        // error lines. Splitting is a display decision the store declined to make, so it
        // is made here, once.
        "errors": lint
            .errors
            .as_deref()
            .map(|errors| {
                errors
                    .lines()
                    .map(str::trim_end)
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        "checked_at": rfc3339_or_null(lint.checked_at),
    })
}

/// The contract's `Draft`.
pub fn draft(draft: &Draft) -> Value {
    json!({
        "review_id": draft.review_id,
        "body": draft.body,
        "status": draft.status.as_str(),
        "event": draft.event.map(|event| event.as_str()),
        "approved_at": rfc3339_or_null(draft.approved_at),
        "sent_back_note": draft.sent_back_note,
        "sent_back_at": rfc3339_or_null(draft.sent_back_at),
        "posted_at": rfc3339_or_null(draft.posted_at),
        // **Why it is no longer signed off**, or null. A draft whose head moved under it
        // went back to `drafting` and lost its approval, and the page has to say why or
        // the sign-off looks as though it were never given: the developer approved the
        // commit named here, and the pull request has moved past it. Null on every draft
        // nobody has signed off and on one signed off since.
        "signoff_withdrawn_at": rfc3339_or_null(draft.signoff_withdrawn_at),
        "signoff_withdrawn_head": draft.signoff_withdrawn_head,
        "updated_at": rfc3339(draft.updated_at),
    })
}

/// The contract's `DraftSection`: one block of the draft body.
///
/// `heading` is served beside the parts it is made of, because the page draws the heading
/// the body will carry and deriving it a second time in the dashboard is a second place for
/// the format to live. `posted` is what says whether this block is in `draft.body` — a cut
/// finding's block is served and does not post.
pub fn draft_section(section: &DraftSection) -> Value {
    json!({
        "position": section.position,
        "kind": section.kind.as_str(),
        "finding_seq": section.finding_seq,
        "finding_key": section.finding_seq.map(finding_key),
        "severity": section.severity.map(|severity| severity.as_str()),
        "heading": heading_of(section),
        "title": section.title,
        "text": section.text,
        "agent_title": section.agent_title,
        "agent_text": section.agent_text,
        "rewritten": section.rewritten(),
        "posted": section.posted,
    })
}

/// The heading a finding block composes, or null on a block of prose.
fn heading_of(section: &DraftSection) -> Value {
    match (section.severity, section.finding_seq, &section.title) {
        (Some(severity), Some(seq), Some(title)) => json!(finding_heading(severity, seq, title)),
        _ => Value::Null,
    }
}

/// The contract's `DraftThread`: one inline thread the draft hands over.
///
/// **What the page draws and what the session posts, from one place.** The draft page lists
/// these with the line each sits on, so the developer reads the comment on the code before
/// signing off; the session puts them in the `comments` array of its one `pulls/<n>/reviews`
/// call. `side` is always `RIGHT` — a removed line carries no number in this record, so a
/// flagged line is always a line still in the file, and the numbering is the new file's.
pub fn draft_thread(thread: &DraftThread) -> Value {
    json!({
        "finding_seq": thread.finding_seq,
        "finding_key": thread.finding_key,
        "severity": thread.severity.as_str(),
        "title": thread.title,
        "path": thread.path,
        "line": thread.line,
        "side": thread.side,
        "body": thread.body,
    })
}

/// The contract's `DraftEdit`: one edit the developer made, as an outcome by them.
pub fn draft_edit(outcome: &Outcome) -> Value {
    json!({
        "finding_seq": outcome.finding_seq,
        "finding_key": finding_key(outcome.finding_seq),
        "kind": outcome.kind.as_str(),
        // `summary` is required and `detail` is not, so an edit recorded without one
        // falls back to the kind rather than the field being omitted.
        "summary": outcome
            .detail
            .clone()
            .unwrap_or_else(|| outcome.kind.as_str().to_string()),
        "occurred_at": rfc3339(outcome.occurred_at),
    })
}

/// The contract's `HeldBackFinding`.
pub fn held_back_finding(finding: &Finding) -> Value {
    json!({
        "finding_seq": finding.seq,
        "finding_key": finding_key(finding.seq),
        "claim": finding.claim,
        "severity": finding.severity.as_str(),
    })
}

/// The contract's `RepoSummary`.
pub fn repo_summary(repo: &RepoSummary) -> Value {
    json!({
        "repo": repo.repo,
        "open_reviews": repo.open_reviews,
        "active_rules": repo.active_rules,
    })
}

/// The contract's `RuleScope`.
pub fn rule_scope(scope: &RuleScope) -> Value {
    json!({ "kind": scope.as_str(), "repo": scope.repo() })
}

/// The contract's `RuleCounts`.
///
/// Every field is written, and a field with no source is written as null. The contract's
/// rule is that a count Review Desk cannot answer is absent and never zero; the store
/// says in its own documentation which three are always absent, and they are the three
/// that need a CI or harvest source nothing supplies.
pub fn rule_counts(counts: &RuleCounts) -> Value {
    json!({
        "raised": counts.raised,
        "kept": counts.kept,
        "rejected_with_reason": counts.rejected_with_reason,
        "cited_by_findings": counts.cited_by_findings,
        "exceptions": counts.exceptions,
        "caught_by_check": counts.caught_by_check,
        "raised_in_a_review_since": counts.raised_in_a_review_since,
        "last_human_mention_at": rfc3339_or_null(counts.last_human_mention_at),
    })
}

/// A rule's display key: `R-` plus its id, zero-padded to two digits.
pub fn rule_key(id: i64) -> String {
    format!("R-{id:02}")
}

/// How the Rules screen words a stage.
pub fn stage_label(stage: RuleStage, exceptions: Option<i64>) -> String {
    let stage = match stage {
        RuleStage::Proposed => "Proposed",
        RuleStage::Active => "Repository rule",
        RuleStage::Lint => "Lint check",
        RuleStage::Ci => "CI check",
        RuleStage::Retired => "Retired",
    };

    match exceptions {
        Some(1) => format!("{stage} · 1 exception"),
        Some(many) if many > 1 => format!("{stage} · {many} exceptions"),
        _ => stage.to_string(),
    }
}

/// Whether nothing has cited a rule for long enough to question it.
///
/// **Stale is a question, not a stage.** A stale rule is still in force until the
/// developer retires it, which is why it is derived here rather than stored as a stage.
///
/// **The threshold is the contract's, not this server's to choose.** It used to be: the
/// store holds `last_cited_at` and `review_again_at` and declines to say how long is too
/// long, and sixty days was taken from the Rules mockup's "No finding has cited it in 60
/// days" because nothing else said. DEV-862 wrote the rule into `Rule.stale` in
/// `contract/openapi.yaml` — no finding has cited it for sixty days and it has not been
/// deferred past today — and `crates/server/tests/staleness.rs` fails when this number and
/// that sentence disagree. Change one and you change both.
pub const STALE_AFTER_DAYS: i64 = 60;

/// Seconds in a day.
pub const DAY: i64 = 86_400;

/// Whether this rule should be shown as going stale.
pub fn is_stale(rule: &Rule, now: Timestamp) -> bool {
    // Deferring answers the question for a while, whatever the citations say. That is the
    // whole point of "Keep 60 more days": it takes the rule out of the stale group
    // without pretending a finding cited it.
    if rule.review_again_at.is_some_and(|until| until > now) {
        return false;
    }

    // A proposal is not in force, so it cannot have gone stale; a retired rule is not in
    // force either. Staleness is a question about rules that are being applied.
    if matches!(rule.stage, RuleStage::Proposed | RuleStage::Retired) {
        return false;
    }

    // Never cited: measured from when the rule was made, so a rule created yesterday is
    // not stale on its first day merely for having no citations yet.
    let last = rule.last_cited_at.unwrap_or(rule.created_at);
    now - last > STALE_AFTER_DAYS * DAY
}

/// The contract's `Rule`.
pub fn rule(rule: &Rule, counts: &RuleCounts, now: Timestamp) -> Value {
    json!({
        "id": rule.id,
        "key": rule_key(rule.id),
        "statement": rule.statement,
        "prevents": rule.prevents,
        "scope": rule_scope(&rule.scope),
        "stage": rule.stage.as_str(),
        "stage_label": stage_label(rule.stage, counts.exceptions),
        "why_proposed": rule.why_proposed,
        "note": rule.note,
        "origin_review_id": rule.origin_review_id,
        // The pull request a rule was decided on is the one its origin review was of.
        // Null when the rule names no origin review; filled in by the caller, which is
        // the only thing that can read that review.
        "origin_pr": Value::Null,
        "origin_findings": rule
            .origin_findings
            .iter()
            .map(|finding| json!({ "review_id": finding.review_id, "seq": finding.seq }))
            .collect::<Vec<_>>(),
        "stale": is_stale(rule, now),
        "last_cited_at": rfc3339_or_null(rule.last_cited_at),
        "review_again_at": rfc3339_or_null(rule.review_again_at),
        "counts": rule_counts(counts),
        "created_at": rfc3339(rule.created_at),
    })
}

/// The contract's `RulesSummary`.
pub fn rules_summary(summary: &RulesSummary) -> Value {
    json!({
        "active_rules": summary.active_rules,
        "active_rules_cap": summary.active_rules_cap,
        "replay_matched": summary.replay_matched,
        "replay_total": summary.replay_total,
        "findings_kept": summary.findings_kept,
        "findings_total": summary.findings_total,
        "findings_window_days": summary.findings_window_days,
        "raised_twice_on_one_pr": summary.raised_twice_on_one_pr,
    })
}

/// The contract's `InboxPr`.
pub fn inbox_pr(review: &Review) -> Value {
    json!({ "repo": review.repo, "pr": review.pr, "title": review.title })
}

/// A repository and pull request number read out of a pasted link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrLink {
    pub repo: String,
    pub pr: i64,
}

/// What a pull request link has to look like, for an error message that says so.
const PR_LINK_SHAPE: &str = "a pull request link such as https://github.com/acme/portal/pull/412";

/// The host a pull request link has to name.
///
/// **Checked, because Review Desk has exactly one GitHub.** The store keys a review on
/// `owner/repo` with no host, and the only way this server reads a pull request is by
/// running `gh`, which talks to github.com. So a link to `gitlab.com/acme/portal` is not
/// a link this server can act on — and accepting it would silently open a review against
/// *a GitHub repository of the same name*, which is a different project entirely.
///
/// A GitHub Enterprise host would need a host on the record and a `gh` told which host to
/// use. That is a change to the store, so it is refused here and named rather than half
/// supported.
const PR_LINK_HOSTS: [&str; 2] = ["github.com", "www.github.com"];

/// Read `owner/repo` and the number out of a pull request link.
///
/// Checked here, in full, **before anything reaches for GitHub**: DEV-861 requires that a
/// malformed link is refused with no GitHub read, so that a machine with no `gh` reports
/// the real problem rather than reporting `gh` missing.
///
/// The shape is matched from the host forwards — `HOST/owner/repo/pull/NUMBER` — rather
/// than by searching for a `pull` segment. Searching looks more forgiving and is wrong:
/// `https://github.com/portal/pull/412` has a `pull` segment with two segments in front
/// of it, and reading those two as the repository makes the host part of the repository
/// name. The repository is then `github.com/portal`, which matches the contract's
/// `RepoName` pattern, so nothing downstream would have objected.
pub fn parse_pr_link(url: &str) -> Result<PrLink, ApiError> {
    let refused = |detail: &str| {
        ApiError::invalid_value("pr_url", format!("{detail}: {url:?}"), PR_LINK_SHAPE)
    };

    // The fragment and query say nothing about which pull request this is, and GitHub
    // adds both — `#discussion_r1`, `?diff=split`.
    let trimmed = url
        .split(['#', '?'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');

    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| refused("the link has no http:// or https:// scheme"))?;
    if !matches!(scheme, "http" | "https") {
        return Err(refused("the link's scheme is not http or https"));
    }

    let segments: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();

    // A path with a `.` or `..` segment means something different once it is normalised:
    // `.../acme/portal/pull/412/../../evil/pull/9` is `acme/evil/pull/9` to a browser and
    // `acme/portal#412` to anything reading the front of the path. Rather than pick one,
    // the link is refused — nobody pastes one of these by accident, and guessing at what
    // a link meant is how the owner and repository end up wrong.
    if segments.iter().any(|part| *part == ".." || *part == ".") {
        return Err(refused(
            "the link's path has a relative segment and does not say which pull request it means",
        ));
    }

    // Host, owner, repository, the literal `pull`, the number. Anything after that —
    // `/files`, `/commits` — is still the same pull request and is ignored.
    let [host, owner, name, "pull", number, ..] = segments.as_slice() else {
        return Err(refused(
            "the link's path is not HOST/owner/repo/pull/NUMBER",
        ));
    };

    // A host and a port, if one is given. The port says nothing about which GitHub this
    // is, and github.com is never on a non-default one.
    let bare_host = host.split(':').next().unwrap_or(host);
    if !PR_LINK_HOSTS
        .iter()
        .any(|allowed| bare_host.eq_ignore_ascii_case(allowed))
    {
        return Err(ApiError::invalid_value(
            "pr_url",
            format!(
                "{bare_host:?} is not a host Review Desk reads: a review is recorded as \
                 owner/repo with no host, and GitHub is the only one it can read"
            ),
            "a link on github.com",
        ));
    }

    let repo = format!("{owner}/{name}");
    if !is_repo_name(&repo) {
        return Err(refused(
            "the owner and repository in the link are not usable",
        ));
    }

    let pr: i64 = number
        .parse()
        .map_err(|_| refused("the pull request number is not a number"))?;
    if pr < 1 {
        return Err(refused("the pull request number is below 1"));
    }

    Ok(PrLink { repo, pr })
}

/// The contract's `RepoName` pattern, and not a relative path.
///
/// **Delegated, because this rule existed twice.** The store had a second, looser copy —
/// missing the `.`/`..` guard this function has always had — and the store is the gate the
/// command line passes through. `review_desk_store::is_repo_name` is the one copy now, and
/// it carries the reason, and the table of the **four** readers that put this value into a
/// path — a dot segment is inert in SQL and is not inert in any of them.
pub fn is_repo_name(repo: &str) -> bool {
    review_desk_store::is_repo_name(repo)
}

/// The contract's `head_sha` pattern: 7 to 40 lower-case hexadecimal characters.
pub fn is_head_sha(sha: &str) -> bool {
    (7..=40).contains(&sha.len())
        && sha
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}

/* -------------------------------------------------------------------------------------
 * Where a review's understanding came from. DEV-875, board A2 sections 1 to 4.
 *
 * None of these is written through this API: the command line writes all of it, and the
 * dashboard reads it. What the server adds on the way out is the three derived facts the
 * contract says it owns — the findings on each file, the description lint's verdict, and
 * which record counts as the review's coverage.
 * ----------------------------------------------------------------------------------- */

/// One row of "Read from".
pub fn review_source(source: &ReviewSource) -> Value {
    json!({
        "position": source.position,
        "kind": source.kind.as_str(),
        "label": source.label,
        "link": source.link,
        // What the session read off the ticket, and what it says the source tells us.
        // Both are the session's own words and neither was fetched.
        "status": source.status,
        "note": source.note,
    })
}

/// One row of "What the ticket asks for".
pub fn acceptance_criterion(criterion: &AcceptanceCriterion) -> Value {
    json!({
        "position": criterion.position,
        "text": criterion.text,
        "status": criterion.status.as_str(),
        "finding_seq": criterion.finding_seq,
        // Derived here, like every other display key, so the dashboard never builds an
        // identifier of its own.
        "finding_key": criterion
            .finding_seq
            .map_or(Value::Null, |seq| json!(finding_key(seq))),
        "file": criterion.file,
        "line": criterion.line,
        "note": criterion.note,
    })
}

/// One line of the Lint section.
pub fn description_lint_result(result: &DescriptionLintResult) -> Value {
    json!({
        "position": result.position,
        "label": result.label,
        "status": result.status.as_str(),
        "detail": result.detail,
    })
}

/// Layer 1's result on the pull request description, or null when it has not run.
///
/// `passes` is "no result failed", computed from the list served beside it. Nothing stores
/// it, so the verdict and the list it summarises cannot disagree.
pub fn description_lint(provenance: &ReviewProvenance) -> Value {
    match provenance.description_lint_passes() {
        None => Value::Null,
        Some(passes) => json!({
            "passes": passes,
            "results": provenance
                .description_lint
                .iter()
                .map(description_lint_result)
                .collect::<Vec<_>>(),
        }),
    }
}

/// One row of "What they built", with the findings on it.
///
/// `seqs` is read from the review's findings by path, by the caller — this module does not
/// reach the store. That is the whole of why the change map holds no finding column: there
/// is no second copy of the join to go stale.
pub fn file_change(file: &FileChange, seqs: &[i64]) -> Value {
    json!({
        "position": file.position,
        "path": file.path,
        "summary": file.summary,
        "lines_added": file.lines_added,
        "lines_removed": file.lines_removed,
        "checked": file.checked.as_str(),
        "how_checked": file.how_checked,
        "finding_seqs": seqs,
        "finding_keys": seqs.iter().map(|seq| finding_key(*seq)).collect::<Vec<_>>(),
    })
}

/// Which record counts as this review's coverage.
///
/// **The one place the question is answered**, which is what stops a screen rendering two
/// coverage answers. `files` when the review has a change map, `sentence` when it has none
/// and the single "not checked" sentence is set, `none` when it has neither.
pub fn coverage_source(files: &[FileChange], not_checked: Option<&str>) -> &'static str {
    if !files.is_empty() {
        "files"
    } else if not_checked.is_some_and(|sentence| !sentence.trim().is_empty()) {
        "sentence"
    } else {
        "none"
    }
}

/// What the review looked at and what it did not, as one answer.
pub fn coverage(source: &'static str, rows: Vec<Value>) -> Value {
    json!({ "source": source, "files": rows })
}

/// The sentence the posted review carries, served **only when it is the coverage record**.
///
/// Null when the review has a change map. The sentence is still on the record and the
/// session reads it back through `review-desk review show`; what it is not, once the table
/// exists, is this review's coverage — and a field served in both cases is a field a page
/// could render beside a table that says something else.
pub fn not_checked(source: &str, sentence: Option<&str>) -> Value {
    match source {
        "sentence" => json!(sentence),
        _ => Value::Null,
    }
}
