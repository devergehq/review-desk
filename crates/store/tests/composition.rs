//! The draft body is composed, and the developer's decisions are what compose it.
//!
//! The file is in two halves. The first is the composition itself — what blocks compose
//! to, what the store refuses, and the one claim the whole of DEV-877 rests on: **the
//! assembled draft of a review with no decisions is what the agent wrote**, held against
//! `contract/examples/getDraft.response.json`, which is the only record of what an agent
//! wrote that this repository has. The second is the eight decisions, one test each, and
//! what each leaves behind.

mod common;

use std::path::{Path, PathBuf};

use common::{new_review, temp_store};
use review_desk_store::{
    finding_heading, Actor, DecidedBy, Decision, Disposition, DraftLint, DraftSectionKind,
    DraftStatus, FindingCarry, FindingDecision, NewDraft, NewDraftSection, NewFinding, OutcomeKind,
    ReviewEvent, RoundStatus, Severity, Store, WorkFilter, WorkKind,
};
use serde_json::Value;

/// A review with three findings in its draft, as an agent would leave it.
///
/// F1 is a blocker, F2 and F3 are shoulds, and the draft is a summary sentence, a block
/// per finding and the not-checked line — the shape board A2 draws.
fn seeded(store: &Store) -> i64 {
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("review");

    for (severity, claim) in [
        (
            Severity::Blocker,
            "A retried delivery is sent without an idempotency key",
        ),
        (Severity::Should, "The backoff schedule has no ceiling"),
        (Severity::Nit, "Backoff schedule is repeated in two files"),
    ] {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity,
                    claim: claim.to_string(),
                    why_it_matters: Some("the consequence, as the agent recorded it".to_string()),
                    ..Default::default()
                },
            )
            .expect("finding");
    }

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: Some(DraftLint {
                    passes: Some(true),
                    visible_words: Some(96),
                    ..Default::default()
                }),
                sections: Some(vec![
                    NewDraftSection::prose("The queue-with-backoff approach fits the ticket."),
                    NewDraftSection::finding(
                        1,
                        "A retried delivery is sent without an idempotency key",
                        "A second delivery is indistinguishable from a new one.",
                    ),
                    NewDraftSection::finding(
                        2,
                        "The backoff schedule has no ceiling",
                        "A dead endpoint is retried forever.",
                    ),
                    NewDraftSection::finding(
                        3,
                        "Backoff schedule is repeated in two files",
                        "Two copies will drift.",
                    ),
                    NewDraftSection::prose("**Not checked:** the admin screens."),
                ]),
            },
        )
        .expect("the draft");

    review.id
}

// -- the composition ----------------------------------------------------------------

#[test]
fn a_finding_blocks_heading_is_composed_from_the_finding() {
    assert_eq!(
        finding_heading(
            Severity::Blocker,
            1,
            "A retried delivery is sent without a key"
        ),
        "### Blocker · F1 · A retried delivery is sent without a key",
        "the format the approved designs and the contract's own example already use"
    );
}

#[test]
fn the_body_is_what_the_blocks_compose_to() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let body = store.get_draft(review).expect("the draft").body;
    assert_eq!(
        body,
        "The queue-with-backoff approach fits the ticket.\n\
         \n\
         ### Blocker · F1 · A retried delivery is sent without an idempotency key\n\
         \n\
         A second delivery is indistinguishable from a new one.\n\
         \n\
         ### Should · F2 · The backoff schedule has no ceiling\n\
         \n\
         A dead endpoint is retried forever.\n\
         \n\
         ### Nit · F3 · Backoff schedule is repeated in two files\n\
         \n\
         Two copies will drift.\n\
         \n\
         **Not checked:** the admin screens.\n"
    );
}

#[test]
fn the_assembled_draft_of_a_review_with_no_decisions_is_what_the_agent_wrote() {
    // **The acceptance criterion of DEV-877, held against a real agent's body.**
    //
    // `contract/examples/getDraft.response.json` is the one record this repository has of
    // what an agent writes: a summary sentence, three findings with the heading format the
    // designs use, a mermaid fence and a folded proof inside one of them, and the
    // not-checked line. The test decomposes it the way a session would — prose for
    // everything the composition does not model, a finding block per heading — writes
    // that, and asserts the stored body is the same bytes it started as.
    let body = example_draft_body();
    let blocks = decompose(&body);

    assert_eq!(
        blocks.len(),
        4,
        "a summary sentence and three findings: {blocks:#?}"
    );
    assert_eq!(
        blocks
            .iter()
            .filter(|block| block.kind == DraftSectionKind::Finding)
            .count(),
        3
    );

    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("review");
    for severity in [Severity::Blocker, Severity::Should, Severity::Should] {
        store
            .add_finding(
                review.id,
                NewFinding {
                    lens: "correctness".to_string(),
                    severity,
                    claim: "as the example has it".to_string(),
                    ..Default::default()
                },
            )
            .expect("finding");
    }

    store
        .put_draft(
            review.id,
            NewDraft {
                // The body as well as the blocks, which is the stronger call: the store
                // composes the blocks and refuses a body that differs from the result, so
                // this assertion is also made at the moment of writing.
                body: body.clone(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(blocks),
            },
        )
        .expect("the example's own body, written in parts");

    assert_eq!(
        store.get_draft(review.id).expect("the draft").body,
        body,
        "the composition has to reproduce what the agent wrote, character for character"
    );
}

/// **A finding block written with no title takes the finding's, and the finding owns it.**
///
/// Two copies of a title are two titles that can disagree, and the one a reader sees first
/// is the finding's — it is the whole of a row on the code review page. So a block that
/// states none is composed from `Finding::title`, and from the claim only for a finding
/// nobody has titled, which is every finding written before migration 11.
#[test]
fn a_finding_block_with_no_title_takes_the_findings_own() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 878, "a1b2c3d", 1))
        .expect("review");

    let titled = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Should,
                claim: "A retried delivery is sent with no idempotency key, so an endpoint \
                        that accepted the first attempt slowly receives the same event a \
                        second time and acts on it twice"
                    .to_string(),
                title: Some("A retried delivery carries no idempotency key".to_string()),
                ..Default::default()
            },
        )
        .expect("a titled finding");

    let untitled = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "naming".to_string(),
                severity: Severity::Nit,
                claim: "The accessor is still public".to_string(),
                ..Default::default()
            },
        )
        .expect("a finding nobody titled");

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![
                    NewDraftSection {
                        kind: DraftSectionKind::Finding,
                        finding_seq: Some(titled.seq),
                        title: None,
                        text: "Please send the delivery id as the key.".to_string(),
                    },
                    NewDraftSection {
                        kind: DraftSectionKind::Finding,
                        finding_seq: Some(untitled.seq),
                        title: None,
                        text: "A `@deprecated` line would stop the next caller.".to_string(),
                    },
                ]),
            },
        )
        .expect("a draft whose blocks state no titles");

    let draft = store.get_draft(review.id).expect("the draft");
    let blocks = store.draft_sections(review.id).expect("the blocks");

    assert_eq!(
        blocks[0].title.as_deref(),
        Some("A retried delivery carries no idempotency key"),
        "the titled finding's own title, not its 56-word claim"
    );
    assert_eq!(
        blocks[1].title.as_deref(),
        Some("The accessor is still public"),
        "and the claim for a finding nobody titled, which is the honest fallback"
    );
    assert!(draft
        .body
        .contains("### Should · F1 · A retried delivery carries no idempotency key"));

    /*
     * **And `agent_title` is the resolved words**, which is what keeps DEV-877's guarantee
     * standing: a rewrite still keeps what it replaced, because what it replaced is what is
     * recorded here.
     */
    assert_eq!(blocks[0].agent_title, blocks[0].title);
    assert!(!blocks[0].rewritten());
}

/// A stated title is used as stated, **and that is deliberate rather than a leftover.**
///
/// A heading the posted review carries is not required to be the one line a row of the code
/// review page carries, and `decideFinding`'s `rewrite` exists precisely to make the two
/// differ. Refusing a stated title that disagreed with the finding's would remove a
/// rewrite's own effect, so the ownership is "the finding is the source", not "they can
/// never differ".
#[test]
fn a_stated_title_is_used_as_stated_even_when_the_finding_has_one() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 878, "d4e5f6a", 1))
        .expect("review");

    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "the claim".to_string(),
                title: Some("The finding's own title".to_string()),
                ..Default::default()
            },
        )
        .expect("finding");

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection::finding(
                    finding.seq,
                    "The heading this review posts",
                    "the paragraph",
                )]),
            },
        )
        .expect("the draft");

    let blocks = store.draft_sections(review.id).expect("the blocks");
    assert_eq!(
        blocks[0].title.as_deref(),
        Some("The heading this review posts")
    );
    assert_eq!(
        store
            .get_finding(review.id, finding.seq)
            .expect("read it")
            .title
            .as_deref(),
        Some("The finding's own title"),
        "and the finding is not rewritten by a block being written"
    );
}

/// Keeping a held-back finding writes it a block **titled from the finding**.
///
/// The designs' "Post it" on a held-back nit. Before the title existed the block was headed
/// with the finding's claim, which on the review board A8 was drawn from ran to 56 words —
/// a heading nobody would post.
#[test]
fn posting_a_held_back_finding_heads_its_block_with_the_findings_title() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 878, "b7c8d9e", 1))
        .expect("review");

    let finding = store
        .add_finding(
            review.id,
            NewFinding {
                lens: "repetition".to_string(),
                severity: Severity::Nit,
                claim: "The superseded accessor is still public with nothing at all marking \
                        it as superseded, so the next caller to need a tenant reaches for it"
                    .to_string(),
                title: Some("The superseded accessor is still public".to_string()),
                why_it_matters: Some("The next caller writes a second path.".to_string()),
                disposition: Disposition::HeldBack,
                disposition_reason: Some("a nit; it goes in the ticket".to_string()),
                ..Default::default()
            },
        )
        .expect("finding");

    store
        .put_draft(
            review.id,
            NewDraft {
                body: String::new(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection::prose("Nothing much to say.")]),
            },
        )
        .expect("the draft");

    store
        .decide_finding(
            review.id,
            finding.seq,
            &Decision {
                decision: FindingDecision::Keep,
                severity: None,
                title: None,
                text: None,
                note: None,
            },
            Actor::Developer,
        )
        .expect("the developer posts it");

    let draft = store.get_draft(review.id).expect("the draft");
    assert!(
        draft
            .body
            .contains("### Nit · F1 · The superseded accessor is still public"),
        "headed from the title, not the claim: {}",
        draft.body
    );
}

#[test]
fn a_body_written_as_one_block_is_stored_as_one_block() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("review");

    store
        .put_draft(
            review.id,
            NewDraft::ready("one block, as a session wrote it"),
        )
        .expect("the draft");

    let sections = store.draft_sections(review.id).expect("the blocks");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].kind, DraftSectionKind::Prose);
    assert_eq!(sections[0].text, "one block, as a session wrote it");
    assert_eq!(
        store.get_draft(review.id).expect("the draft").body,
        "one block, as a session wrote it\n",
        "composed, which ends in one newline"
    );
}

#[test]
fn a_body_the_blocks_do_not_compose_to_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .put_draft(
            review,
            NewDraft {
                body: "something else entirely".to_string(),
                status: DraftStatus::Ready,
                lint: None,
                sections: Some(vec![NewDraftSection::prose("the blocks say this")]),
            },
        )
        .expect_err("a body and blocks that disagree");

    assert_eq!(refused.code(), "invalid_value");
    assert!(
        refused.to_string().contains("compose"),
        "the refusal has to say which of the two is the composition: {refused}"
    );
    assert!(
        store
            .get_draft(review)
            .expect("the draft")
            .body
            .contains("### Blocker · F1"),
        "and nothing may have been written"
    );
}

#[test]
fn two_blocks_about_one_finding_are_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .put_draft_sections(
            review,
            &[
                NewDraftSection::finding(1, "once", "the first paragraph"),
                NewDraftSection::finding(1, "twice", "the second paragraph"),
            ],
        )
        .expect_err("two blocks about F1");

    assert_eq!(refused.code(), "invalid_value");
    assert!(refused.to_string().contains("F1"), "{refused}");
}

#[test]
fn a_block_that_brings_its_own_blank_line_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    for text in ["\nleading", "trailing\n"] {
        let refused = store
            .put_draft_sections(review, &[NewDraftSection::prose(text)])
            .expect_err("a block whose text would compose a body nobody wrote");
        assert_eq!(refused.code(), "invalid_value");
        assert!(refused.to_string().contains("newline"), "{refused}");
    }
}

#[test]
fn a_block_about_a_finding_the_draft_cannot_raise_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    // The agent marks F3 fixed, as `finding became` does, and then tries to write a block
    // about it. **This is the rule DEV-873 left to this issue**: a carried finding whose
    // round status is fixed, accepted, withdrawn or ticketed is not posted as an open
    // finding, and the write is where that is enforced rather than the body quietly
    // dropping a paragraph the agent wrote.
    store
        .set_finding_round_status(
            review,
            3,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: Some("their new test fails with the fix reverted".to_string()),
                commit: None,
            },
        )
        .expect("the agent records what became of it");

    let refused = store
        .put_draft_sections(
            review,
            &[NewDraftSection::finding(
                3,
                "still here",
                "or so this block says",
            )],
        )
        .expect_err("a block about a fixed finding");

    assert_eq!(refused.code(), "invalid_value");
    assert!(refused.to_string().contains("fixed"), "{refused}");
}

#[test]
fn a_finding_marked_fixed_after_the_draft_was_written_stops_posting() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .set_finding_round_status(
            review,
            2,
            &FindingCarry {
                status: RoundStatus::Fixed,
                by: DecidedBy::Agent,
                checked: Some("fixed in a41c9e2".to_string()),
                commit: None,
            },
        )
        .expect("the status changes");

    // The body is stored, so it does not change until something recomposes it — and the
    // blocks are what say whether it would. That is the point of storing it: an approved
    // draft cannot be recomposed underneath the session about to post it.
    let sections = store.draft_sections(review).expect("the blocks");
    let f2 = sections
        .iter()
        .find(|section| section.finding_seq == Some(2))
        .expect("F2's block");
    assert!(
        !f2.posted,
        "a fixed finding is not posted as an open finding"
    );

    store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("any decision recomposes the body");
    let body = store.get_draft(review).expect("the draft").body;
    assert!(
        !body.contains("F2"),
        "and it leaves the body when it is next composed: {body}"
    );
}

#[test]
fn changing_a_block_of_prose_keeps_what_a_rewrite_replaced() {
    // **How the developer changes a block of prose**: there is no decision for one — a
    // prose block is about no finding, so it can carry no outcome — and the dashboard
    // writes the whole composition back with that one block's text changed. Every other
    // block goes back unchanged, and a finding block the developer had rewritten must not
    // be relabelled as the agent's words by an edit somewhere else in the body.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .decide_finding(
            review,
            1,
            &Decision {
                text: Some("The customer receives the event twice.".to_string()),
                ..Decision::of(FindingDecision::Rewrite)
            },
            Actor::Developer,
        )
        .expect("the developer rewrites F1");

    // The whole composition, as the page holds it, with the summary sentence changed.
    let held = store.draft_sections(review).expect("the blocks");
    let sections: Vec<NewDraftSection> = held
        .iter()
        .map(|section| NewDraftSection {
            kind: section.kind,
            finding_seq: section.finding_seq,
            title: section.title.clone(),
            text: if section.position == 1 {
                "The queue-with-backoff approach fits, and one thing blocks the merge.".to_string()
            } else {
                section.text.clone()
            },
        })
        .collect();
    store
        .put_draft_sections(review, &sections)
        .expect("the developer changes the summary sentence");

    let f1 = store
        .draft_sections(review)
        .expect("the blocks")
        .into_iter()
        .find(|section| section.finding_seq == Some(1))
        .expect("F1's block");
    assert_eq!(f1.text, "The customer receives the event twice.");
    assert_eq!(
        f1.agent_text, "A second delivery is indistinguishable from a new one.",
        "what the rewrite replaced survives an edit to another block"
    );
}

#[test]
fn a_session_that_writes_new_words_is_the_author_of_them() {
    // The other side of the rule above: a session reworking a draft after a send-back is
    // the writer of what it sends, and the comparison is against *its* words.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .decide_finding(
            review,
            1,
            &Decision {
                text: Some("the developer's paragraph".to_string()),
                ..Decision::of(FindingDecision::Rewrite)
            },
            Actor::Developer,
        )
        .expect("the developer rewrites F1");
    store
        .put_draft_sections(
            review,
            &[NewDraftSection::finding(
                1,
                "A retried delivery is sent without an idempotency key",
                "the session's own new paragraph",
            )],
        )
        .expect("the session reworks the draft");

    let f1 = store
        .draft_sections(review)
        .expect("the blocks")
        .into_iter()
        .find(|section| section.finding_seq == Some(1))
        .expect("F1's block");
    assert_eq!(f1.text, "the session's own new paragraph");
    assert_eq!(f1.agent_text, "the session's own new paragraph");
    assert!(
        !f1.rewritten(),
        "nothing has been rewritten since the session wrote this"
    );
}

#[test]
fn a_block_handed_back_unchanged_does_not_stop_an_edit_somewhere_else() {
    // **Found by the review of this change, after it merged.** `draft_sections` serves
    // every block including the ones that do not post — a cut finding's block stays on the
    // record — and the way the developer changes a block of prose is to write the whole
    // composition back with that one block's text replaced. So the block the store just
    // handed the page is the block the next write has to accept.
    //
    // It refused: any block naming a finding that is not posted was refused, whether the
    // caller was adding it or handing it straight back. The first decision that took a
    // finding out of the post therefore made every later prose edit impossible, and the
    // recoveries were all worse — dropping the block loses what a rewrite replaced, and
    // raising it again to edit it writes outcomes the developer never decided.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some("the ceiling is ABC-240 work".to_string()),
                ..Decision::of(FindingDecision::Cut)
            },
            Actor::Developer,
        )
        .expect("the developer cuts F2");

    let held = store.draft_sections(review).expect("the blocks");
    assert!(
        held.iter()
            .any(|section| section.finding_seq == Some(2) && !section.posted),
        "F2's block is still served, and does not post"
    );

    let sections: Vec<NewDraftSection> = held
        .iter()
        .map(|section| NewDraftSection {
            kind: section.kind,
            finding_seq: section.finding_seq,
            title: section.title.clone(),
            text: if section.position == 1 {
                "One thing blocks the merge.".to_string()
            } else {
                section.text.clone()
            },
        })
        .collect();

    store
        .put_draft_sections(review, &sections)
        .expect("the developer changes the summary sentence after cutting a finding");

    let after = store.draft_sections(review).expect("the blocks");
    assert_eq!(after[0].text, "One thing blocks the merge.");
    let f2 = after
        .iter()
        .find(|section| section.finding_seq == Some(2))
        .expect("F2's block survives the write");
    assert!(!f2.posted);
    assert_eq!(
        f2.agent_text, "A dead endpoint is retried forever.",
        "and what the agent wrote about it is still there"
    );
    assert!(
        !store
            .get_draft(review)
            .expect("the draft")
            .body
            .contains("F2"),
        "a cut finding is still not posted"
    );
}

#[test]
fn a_block_added_for_a_finding_that_cannot_be_posted_is_still_refused() {
    // The other half of the rule above: what the refusal is *for* is a session writing a
    // draft that raises a finding the composition will drop. Adding such a block, and
    // changing the words of one, are both that claim; handing one back is not.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some("withdrawn".to_string()),
                ..Decision::of(FindingDecision::Withdraw)
            },
            Actor::Developer,
        )
        .expect("the developer withdraws F2");

    let held = store.draft_sections(review).expect("the blocks");
    let changed: Vec<NewDraftSection> = held
        .iter()
        .map(|section| NewDraftSection {
            kind: section.kind,
            finding_seq: section.finding_seq,
            title: section.title.clone(),
            text: if section.finding_seq == Some(2) {
                "the session says it is still open after all".to_string()
            } else {
                section.text.clone()
            },
        })
        .collect();

    let refused = store
        .put_draft_sections(review, &changed)
        .expect_err("new words for a withdrawn finding");
    assert_eq!(refused.code(), "invalid_value");
    assert!(refused.to_string().contains("withdrawn"), "{refused}");
}

#[test]
fn a_rewrite_cannot_compose_a_body_nobody_wrote() {
    // **Found by the review, after the merge.** `rewrite` was the one write path that did
    // not run `validate`, so the two rules the composition rests on — no newline at either
    // end of a block's text, and a finding block carries a title — were enforced
    // everywhere except on the decision whose whole purpose is to change those words. A
    // body with three blank lines in it, or a heading reading `### Blocker · F1 ·`, is
    // posted to GitHub verbatim under the developer's name.
    let (_dir, store) = temp_store();
    let review = seeded(&store);
    let before = store.get_draft(review).expect("the draft").body;

    for (title, text) in [
        (None, Some("\nleading and trailing\n".to_string())),
        (None, Some("trailing only\n".to_string())),
        (Some("   ".to_string()), None),
        (Some(String::new()), None),
    ] {
        let refused = store
            .decide_finding(
                review,
                1,
                &Decision {
                    title: title.clone(),
                    text: text.clone(),
                    ..Decision::of(FindingDecision::Rewrite)
                },
                Actor::Developer,
            )
            .expect_err(&format!(
                "{title:?} / {text:?} composes a body nobody wrote"
            ));
        assert_eq!(refused.code(), "invalid_value", "{title:?} / {text:?}");
    }

    assert_eq!(
        store.get_draft(review).expect("the draft").body,
        before,
        "and none of them was written"
    );
}

#[test]
fn posting_a_nit_in_the_developers_own_words_says_whose_they_are() {
    // **Found by the review, after the merge.** A `keep` may carry the developer's own
    // words, because that is the designs' "Post it" on a held-back nit. Those words were
    // written into `agent_title` and `agent_text` as well, so the record said the agent
    // wrote them, `rewritten()` read false, and the outcome was a bare `kept` — a decision
    // that changed what gets posted without the record saying it had, which is the one
    // thing this issue exists to stop.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .put_draft_sections(
            review,
            &[NewDraftSection::prose("The approach fits the ticket.")],
        )
        .expect("a draft with no block for F3");
    store
        .set_finding_disposition(review, 3, Disposition::HeldBack, Some("a nit".to_string()))
        .expect("held back");

    let made = store
        .decide_finding(
            review,
            3,
            &Decision {
                title: Some("The backoff schedule lives in two files".to_string()),
                text: Some("Say it once. The second copy will drift.".to_string()),
                ..Decision::of(FindingDecision::Keep)
            },
            Actor::Developer,
        )
        .expect("the developer posts the nit in their own words");

    let block = store
        .draft_sections(review)
        .expect("the blocks")
        .into_iter()
        .find(|section| section.finding_seq == Some(3))
        .expect("F3's block");

    assert_eq!(block.text, "Say it once. The second copy will drift.");
    assert_eq!(
        block.agent_text, "the consequence, as the agent recorded it",
        "the agent's words are the finding's own consequence, which is what the store \
         would have composed the block from"
    );
    assert_eq!(
        block.agent_title.as_deref(),
        Some("Backoff schedule is repeated in two files"),
        "and its claim"
    );
    assert!(
        block.rewritten(),
        "the posted words are the developer's, and the record has to say so"
    );
    assert_eq!(made.outcome.kind, OutcomeKind::Kept);
    assert!(
        made.outcome
            .detail
            .as_deref()
            .expect("a detail saying what the decision did")
            .contains("own words"),
        "{:?}",
        made.outcome.detail
    );
}

// -- the decisions ------------------------------------------------------------------

#[test]
fn only_the_developer_decides() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    for actor in [Actor::Author, Actor::Harvester] {
        let refused = store
            .decide_finding(review, 1, &Decision::of(FindingDecision::Cut), actor)
            .expect_err("an actor who is not the developer");
        assert_eq!(refused.code(), "invalid_value");
        assert!(refused.to_string().contains("developer"), "{refused}");
    }
}

#[test]
fn cutting_a_finding_takes_it_and_its_comment_out_of_the_draft() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some("the ceiling is in ABC-240".to_string()),
                ..Decision::of(FindingDecision::Cut)
            },
            Actor::Developer,
        )
        .expect("the developer cuts F2");

    assert!(!made.draft.body.contains("F2"), "{}", made.draft.body);
    assert!(
        !made.draft.body.contains("retried forever"),
        "its paragraph goes with it"
    );
    assert!(made.draft.body.contains("F1"), "and nothing else moves");
    assert_eq!(made.outcome.kind, OutcomeKind::Cut);
    assert_eq!(made.outcome.actor, Actor::Developer);
    assert_eq!(
        made.outcome.detail.as_deref(),
        Some("the ceiling is in ABC-240")
    );
    assert_eq!(made.finding.disposition, Disposition::Suppressed);

    let block = store
        .draft_sections(review)
        .expect("the blocks")
        .into_iter()
        .find(|section| section.finding_seq == Some(2))
        .expect("F2's block is still on the record");
    assert!(!block.posted, "cut is a decision, not a deletion");
}

#[test]
fn regrading_changes_the_heading_and_records_both_severities() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            1,
            &Decision {
                severity: Some(Severity::Should),
                ..Decision::of(FindingDecision::Regrade)
            },
            Actor::Developer,
        )
        .expect("the developer regrades F1");

    assert_eq!(made.finding.severity, Severity::Should);
    assert!(
        made.draft
            .body
            .contains("### Should · F1 · A retried delivery is sent without an idempotency key"),
        "{}",
        made.draft.body
    );
    assert!(!made.draft.body.contains("### Blocker · F1"));
    assert_eq!(made.outcome.kind, OutcomeKind::Regraded);
    assert_eq!(
        made.outcome.detail.as_deref(),
        Some("Blocker to Should"),
        "the old severity and the new one, which is what the draft screen lists"
    );
}

#[test]
fn a_severity_cannot_change_without_the_record_saying_it_was_regraded() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .decide_finding(
            review,
            1,
            &Decision {
                severity: Some(Severity::Nit),
                ..Decision::of(FindingDecision::Keep)
            },
            Actor::Developer,
        )
        .expect_err("a severity on a decision that is not a regrade");

    assert_eq!(refused.code(), "invalid_value");
    assert_eq!(
        store.get_finding(review, 1).expect("F1").severity,
        Severity::Blocker
    );
}

#[test]
fn rewriting_stores_the_developers_words_and_what_they_replaced() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            1,
            &Decision {
                title: Some("A retry carries no idempotency key".to_string()),
                text: Some("The customer receives the event twice.".to_string()),
                ..Decision::of(FindingDecision::Rewrite)
            },
            Actor::Developer,
        )
        .expect("the developer rewrites F1");

    assert!(
        made.draft
            .body
            .contains("### Blocker · F1 · A retry carries no idempotency key"),
        "{}",
        made.draft.body
    );
    assert!(made
        .draft
        .body
        .contains("The customer receives the event twice."));
    assert_eq!(made.outcome.kind, OutcomeKind::Rewritten);

    let block = store
        .draft_sections(review)
        .expect("the blocks")
        .into_iter()
        .find(|section| section.finding_seq == Some(1))
        .expect("F1's block");
    assert_eq!(block.text, "The customer receives the event twice.");
    assert_eq!(
        block.agent_text, "A second delivery is indistinguishable from a new one.",
        "what the rewrite replaced is kept, and it is the agent's own paragraph"
    );
    assert_eq!(
        block.agent_title.as_deref(),
        Some("A retried delivery is sent without an idempotency key")
    );
}

#[test]
fn a_rewrite_with_nothing_to_say_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Rewrite),
            Actor::Developer,
        )
        .expect_err("a rewrite with neither a title nor a paragraph");
    assert_eq!(refused.code(), "invalid_value");
}

#[test]
fn words_on_a_decision_that_has_nowhere_to_put_them_are_refused() {
    // An API that accepts a field and does nothing with it is one a caller cannot tell
    // they misused. The two that *can* hold words are a rewrite, and a keep for a finding
    // the draft holds no block for — the designs' "Post it" on a held-back nit.
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    for decision in [
        FindingDecision::Cut,
        FindingDecision::ToTicket,
        FindingDecision::Withdraw,
    ] {
        let refused = store
            .decide_finding(
                review,
                1,
                &Decision {
                    text: Some("the developer's words".to_string()),
                    note: Some("a reason".to_string()),
                    ..Decision::of(decision)
                },
                Actor::Developer,
            )
            .unwrap_err();
        assert_eq!(refused.code(), "invalid_value", "{decision:?}");
    }

    // And on a keep for a finding that already has a block, because taking them would
    // change what gets posted while the record said only "kept".
    let refused = store
        .decide_finding(
            review,
            1,
            &Decision {
                text: Some("the developer's words".to_string()),
                ..Decision::of(FindingDecision::Keep)
            },
            Actor::Developer,
        )
        .unwrap_err();
    assert_eq!(refused.code(), "invalid_value");
    assert!(refused.to_string().contains("rewriting"), "{refused}");
}

#[test]
fn keeping_a_finding_records_it_and_changes_no_words() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);
    let before = store.get_draft(review).expect("the draft").body;

    let made = store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("the developer keeps F1");

    assert_eq!(made.draft.body, before);
    assert_eq!(made.outcome.kind, OutcomeKind::Kept);
    assert_eq!(made.finding.disposition, Disposition::InDraft);
}

#[test]
fn keeping_a_finding_cut_a_moment_ago_puts_it_back() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);
    let before = store.get_draft(review).expect("the draft").body;

    store
        .decide_finding(
            review,
            2,
            &Decision::of(FindingDecision::Cut),
            Actor::Developer,
        )
        .expect("cut");
    let made = store
        .decide_finding(
            review,
            2,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("and keep it after all");

    assert_eq!(
        made.draft.body, before,
        "the decision track is four buttons the developer moves between"
    );
    let kinds: Vec<OutcomeKind> = store
        .list_outcomes_for_finding(review_desk_store::FindingRef {
            review_id: review,
            seq: 2,
        })
        .expect("the outcomes")
        .iter()
        .map(|outcome| outcome.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![OutcomeKind::Cut, OutcomeKind::Kept],
        "and both decisions stay on the record"
    );
}

#[test]
fn posting_a_held_back_nit_writes_its_block_above_the_not_checked_line() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    // The agent held F3 back, as the draft screen's "Held back · nits" list shows it, and
    // the draft has no block for it.
    store
        .put_draft_sections(
            review,
            &[
                NewDraftSection::prose("The queue-with-backoff approach fits the ticket."),
                NewDraftSection::finding(
                    1,
                    "A retried delivery is sent without an idempotency key",
                    "A second delivery is indistinguishable from a new one.",
                ),
                NewDraftSection::prose("**Not checked:** the admin screens."),
            ],
        )
        .expect("a draft of two findings and the not-checked line");
    store
        .set_finding_disposition(review, 3, Disposition::HeldBack, Some("a nit".to_string()))
        .expect("held back");
    store
        .set_finding_disposition(review, 2, Disposition::HeldBack, Some("a nit".to_string()))
        .expect("held back");

    let made = store
        .decide_finding(
            review,
            3,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect("the developer posts the nit after all");

    assert_eq!(made.finding.disposition, Disposition::InDraft);
    assert_eq!(made.outcome.kind, OutcomeKind::Kept);
    let body = made.draft.body;
    let nit = body.find("### Nit · F3").expect("the nit's heading");
    let not_checked = body.find("**Not checked:**").expect("the not-checked line");
    assert!(
        nit < not_checked,
        "the nit goes with the findings, above the line: {body}"
    );
    assert!(
        body.contains("the consequence, as the agent recorded it"),
        "and its paragraph is the finding's own consequence, not an invention: {body}"
    );
}

#[test]
fn a_decision_that_needs_a_block_says_so_when_the_draft_has_none() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .expect("review");
    store
        .add_finding(
            review.id,
            NewFinding {
                lens: "correctness".to_string(),
                severity: Severity::Blocker,
                claim: "a finding the one-block draft mentions somewhere".to_string(),
                ..Default::default()
            },
        )
        .expect("finding");
    store
        .put_draft(
            review.id,
            NewDraft::ready("one block of markdown, as every draft was"),
        )
        .expect("the draft");

    let refused = store
        .decide_finding(
            review.id,
            1,
            &Decision::of(FindingDecision::Cut),
            Actor::Developer,
        )
        .expect_err("cutting a finding out of a draft that has no block for it");

    assert_eq!(refused.code(), "no_draft_section");
    assert!(
        refused.to_string().contains("one block"),
        "the refusal has to say why, because every draft written before this is like it: \
         {refused}"
    );
}

#[test]
fn raising_a_carried_finding_again_says_it_is_still_open() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some("the author did not answer it".to_string()),
                ..Decision::of(FindingDecision::RaiseAgain)
            },
            Actor::Developer,
        )
        .expect("the developer raises F2 again");

    assert_eq!(made.finding.round_status, RoundStatus::StillOpen);
    assert_eq!(made.finding.round_status_by, Some(DecidedBy::Developer));
    assert_eq!(
        made.finding.round_status_checked.as_deref(),
        Some("the author did not answer it")
    );
    assert_eq!(made.outcome.kind, OutcomeKind::RaisedAgain);
    assert!(made.draft.body.contains("F2"), "and it is in the body");
}

#[test]
fn accepting_the_authors_reason_keeps_the_reason() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            2,
            &Decision {
                note: Some(
                    "Agreed, but I'd like to do it with the dashboard work in ABC-240.".to_string(),
                ),
                ..Decision::of(FindingDecision::AcceptReason)
            },
            Actor::Developer,
        )
        .expect("the developer accepts the author's reason on F2");

    assert_eq!(made.finding.round_status, RoundStatus::Accepted);
    assert_eq!(made.finding.round_status_by, Some(DecidedBy::Developer));
    assert_eq!(
        made.outcome.kind,
        OutcomeKind::Rejected,
        "which is what 'rejected with a reason you accepted' counts"
    );
    assert!(made
        .outcome
        .detail
        .as_deref()
        .expect("the reason")
        .contains("ABC-240"));
    assert!(
        !made.draft.body.contains("F2"),
        "and it leaves the post: {}",
        made.draft.body
    );
}

#[test]
fn accepting_a_reason_with_no_reason_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .decide_finding(
            review,
            2,
            &Decision::of(FindingDecision::AcceptReason),
            Actor::Developer,
        )
        .expect_err("an acceptance with nothing accepted");
    assert_eq!(refused.code(), "invalid_value");
    assert_eq!(
        store.get_finding(review, 2).expect("F2").round_status,
        RoundStatus::New,
        "and the record is untouched"
    );
}

#[test]
fn withdrawing_a_finding_says_it_no_longer_stands() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            3,
            &Decision {
                note: Some("the two copies are deliberate".to_string()),
                ..Decision::of(FindingDecision::Withdraw)
            },
            Actor::Developer,
        )
        .expect("the developer withdraws F3");

    assert_eq!(made.finding.round_status, RoundStatus::Withdrawn);
    assert_eq!(made.outcome.kind, OutcomeKind::Withdrawn);
    assert!(!made.draft.body.contains("F3"));
}

#[test]
fn moving_a_finding_to_a_ticket_holds_it_back_and_records_it_ticketed() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let made = store
        .decide_finding(
            review,
            3,
            &Decision {
                note: Some("one ticket for the repetition".to_string()),
                ..Decision::of(FindingDecision::ToTicket)
            },
            Actor::Developer,
        )
        .expect("the developer moves F3 to a ticket");

    assert_eq!(made.finding.disposition, Disposition::HeldBack);
    assert_eq!(made.finding.round_status, RoundStatus::Ticketed);
    assert_eq!(made.outcome.kind, OutcomeKind::Ticketed);
    assert!(!made.draft.body.contains("F3"));
    assert_eq!(
        store.nit_ticket(review).expect("the ticket").finding_seqs,
        vec![3],
        "and it is in the ticket the review will file"
    );
}

#[test]
fn keeping_a_finding_this_round_has_settled_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .decide_finding(
            review,
            3,
            &Decision {
                note: Some("withdrawn".to_string()),
                ..Decision::of(FindingDecision::Withdraw)
            },
            Actor::Developer,
        )
        .expect("withdrawn");

    let refused = store
        .decide_finding(
            review,
            3,
            &Decision::of(FindingDecision::Keep),
            Actor::Developer,
        )
        .expect_err("keeping a withdrawn finding");
    assert_eq!(refused.code(), "invalid_value");
    assert!(refused.to_string().contains("again"), "{refused}");
}

#[test]
fn every_decision_leaves_the_lint_reading_as_not_checked() {
    // Every one of the eight, each on its own review, because the draft's lint is written
    // once by the agent and a decision is what makes it stale.
    let decisions = [
        Decision::of(FindingDecision::Keep),
        Decision::of(FindingDecision::Cut),
        Decision {
            severity: Some(Severity::Nit),
            ..Decision::of(FindingDecision::Regrade)
        },
        Decision {
            text: Some("the developer's words".to_string()),
            ..Decision::of(FindingDecision::Rewrite)
        },
        Decision {
            note: Some("one ticket".to_string()),
            ..Decision::of(FindingDecision::ToTicket)
        },
        Decision::of(FindingDecision::RaiseAgain),
        Decision {
            note: Some("their reason".to_string()),
            ..Decision::of(FindingDecision::AcceptReason)
        },
        Decision::of(FindingDecision::Withdraw),
    ];

    for decision in decisions {
        // A store each, because each decision is about the same review of the same pull
        // request and the record will not hold two of those.
        let (_dir, store) = temp_store();
        let review = seeded(&store);
        let before = store.get_draft(review).expect("the draft");
        assert_eq!(
            before.lint.passes,
            Some(true),
            "the agent's own lint result"
        );

        let made = store
            .decide_finding(review, 1, &decision, Actor::Developer)
            .unwrap_or_else(|error| panic!("{:?}: {error}", decision.decision));

        assert_eq!(
            made.draft.lint.passes, None,
            "{:?} must leave the lint reading as not checked, never as passing",
            decision.decision
        );
        assert!(made.draft.lint.is_empty(), "{:?}", decision.decision);
    }
}

#[test]
fn a_decision_about_an_approved_draft_is_refused() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);
    store
        .approve_draft(review, ReviewEvent::RequestChanges, Actor::Developer)
        .expect("the developer signs off");

    let refused = store
        .decide_finding(
            review,
            1,
            &Decision::of(FindingDecision::Cut),
            Actor::Developer,
        )
        .expect_err("a decision after the sign-off");

    assert_eq!(refused.code(), "draft_status");
    assert!(
        store
            .get_draft(review)
            .expect("the draft")
            .body
            .contains("F1"),
        "an approved draft is what the session posts, and it does not move under it"
    );
}

// -- ticket what is left, and approve ------------------------------------------------

#[test]
fn ticketing_what_is_left_holds_the_open_findings_back_and_approves_in_one_step() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let settled = store
        .ticket_rest_and_approve(review, Some("filed as one ticket"), Actor::Developer)
        .expect("ticket what is left, and approve");

    assert_eq!(settled.ticketed, vec![1, 2, 3]);
    assert_eq!(settled.draft.status, DraftStatus::Approved);
    assert_eq!(
        settled.draft.event,
        Some(ReviewEvent::Approve),
        "the button says approve, and a request for changes that ticketed everything it \
         asked for would contradict itself"
    );
    assert_eq!(settled.draft.approved_by, Some(Actor::Developer));
    assert_eq!(
        settled.ticket.status,
        review_desk_store::NitTicketStatus::Requested
    );
    assert_eq!(settled.ticket.finding_seqs, vec![1, 2, 3]);
    assert!(settled.ticket.requested_at.is_some());

    for seq in [1, 2, 3] {
        let finding = store.get_finding(review, seq).expect("the finding");
        assert_eq!(finding.disposition, Disposition::HeldBack);
        assert_eq!(finding.round_status, RoundStatus::Ticketed);
        assert_eq!(finding.round_status_by, Some(DecidedBy::Developer));

        let kinds: Vec<OutcomeKind> = store
            .list_outcomes_for_finding(review_desk_store::FindingRef {
                review_id: review,
                seq,
            })
            .expect("the outcomes")
            .iter()
            .map(|outcome| outcome.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![OutcomeKind::Ticketed],
            "each one leaves an outcome"
        );
    }

    let body = settled.draft.body;
    assert!(
        !body.contains("### "),
        "no finding is posted as open: {body}"
    );
    assert!(
        body.contains("**Not checked:**"),
        "and the prose stands: {body}"
    );
    assert!(
        settled.draft.lint.is_empty(),
        "the body changed, so the lint is not checked"
    );
}

#[test]
fn ticketing_what_is_left_finishes_layer_six() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .ticket_rest_and_approve(review, None, Actor::Developer)
        .expect("ticket what is left, and approve");

    let layer = store
        .find_review_layer(review, 6)
        .expect("layer 6")
        .expect("written by the approval");
    assert_eq!(layer.state, review_desk_store::ReviewLayerState::Done);
    assert_eq!(layer.detail.as_deref(), Some("approved by the developer"));
}

#[test]
fn ticketing_what_is_left_is_refused_when_nothing_is_open() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    for seq in [1, 2, 3] {
        store
            .decide_finding(
                review,
                seq,
                &Decision {
                    note: Some("cut".to_string()),
                    ..Decision::of(FindingDecision::Cut)
                },
                Actor::Developer,
            )
            .expect("cut");
    }

    let refused = store
        .ticket_rest_and_approve(review, None, Actor::Developer)
        .expect_err("nothing left to ticket");
    assert_eq!(refused.code(), "invalid_value");
    assert_eq!(
        store.get_draft(review).expect("the draft").status,
        DraftStatus::Ready,
        "and the draft is not approved as a side effect of the refusal"
    );
}

#[test]
fn only_the_developer_tickets_what_is_left() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    let refused = store
        .ticket_rest_and_approve(review, None, Actor::Harvester)
        .expect_err("a sign-off by anybody else");
    assert_eq!(refused.code(), "invalid_value");
    assert_eq!(
        store.get_draft(review).expect("the draft").status,
        DraftStatus::Ready
    );
}

#[test]
fn the_ticket_the_developer_asked_for_is_work_an_agent_can_see() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .ticket_rest_and_approve(review, None, Actor::Developer)
        .expect("ticket what is left, and approve");

    // The draft is the more advanced thing outstanding, so the item is the approval — and
    // it carries the pending ticket, so nothing has to wait to find out about it.
    let items = store
        .work_list(&WorkFilter::default())
        .expect("the work list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, WorkKind::DraftApproved);
    assert_eq!(
        items[0].review.nit_ticket_status,
        review_desk_store::NitTicketStatus::Requested
    );

    store
        .post_draft(review, "gh-review-9001")
        .expect("the session posts it");

    let items = store
        .work_list(&WorkFilter::default())
        .expect("the work list");
    assert_eq!(
        items.len(),
        1,
        "and once it is posted the ticket is the work: {items:#?}"
    );
    assert_eq!(items[0].kind, WorkKind::NitsToTicket);
    assert_eq!(
        items[0].entered_at,
        items[0].review.nit_ticket_requested_at.expect("asked at")
    );

    store
        .request_nit_ticket(review, Some("ABC-246"))
        .expect("the session files it and records the ticket");
    assert!(
        store
            .work_list(&WorkFilter::default())
            .expect("the work list")
            .is_empty(),
        "and then there is nothing left to do"
    );
}

#[test]
fn a_nit_ticket_asked_for_on_its_own_is_work_an_agent_can_see() {
    let (_dir, store) = temp_store();
    let review = seeded(&store);

    store
        .set_finding_disposition(review, 3, Disposition::HeldBack, Some("a nit".to_string()))
        .expect("held back");
    store
        .request_nit_ticket(review, None)
        .expect("the developer asks for the one ticket");

    let items = store
        .work_list(&WorkFilter::default())
        .expect("the work list");
    assert_eq!(
        items.iter().map(|item| item.kind).collect::<Vec<_>>(),
        vec![WorkKind::NitsToTicket],
        "which nothing reported before DEV-877, though the button has existed since \
         DEV-854: {items:#?}"
    );
}

// -- the example this all rests on ---------------------------------------------------

/// The draft body from `contract/examples/getDraft.response.json`.
fn example_draft_body() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|root| {
            root.join("contract")
                .join("examples")
                .join("getDraft.response.json")
        })
        .expect("the repository root");
    let document: Value =
        serde_json::from_str(&read(&path)).expect("the example is a JSON document");

    document["draft"]["body"]
        .as_str()
        .expect("the example's draft body")
        .to_string()
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Split a body an agent wrote into the blocks it is composed of.
///
/// **What a review session does when it writes a draft**, done here so the claim is tested
/// against a body nobody wrote for this test. A line beginning `### ` followed by a blank
/// line starts a finding block and the heading is read back; everything else is prose. The
/// heading format is the store's — `finding_heading` — so reading it back is not a second
/// format living in a test.
fn decompose(body: &str) -> Vec<NewDraftSection> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();

    for line in body.lines() {
        if line.starts_with("### ") || blocks.is_empty() {
            blocks.push(Vec::new());
        }
        blocks.last_mut().expect("a block").push(line);
    }

    blocks
        .iter()
        .map(|lines| {
            let whole = lines.join("\n").trim_matches('\n').to_string();
            match heading_of(lines[0]) {
                Some((severity, seq, title)) => {
                    let text = whole
                        .split_once("\n\n")
                        .map(|(_, rest)| rest.to_string())
                        .unwrap_or_default();
                    assert_eq!(
                        lines[0],
                        finding_heading(severity, seq, &title),
                        "the heading has to read back the way the store writes it"
                    );
                    NewDraftSection::finding(seq, title, text.trim_matches('\n'))
                }
                None => NewDraftSection::prose(whole),
            }
        })
        .collect()
}

/// `### Blocker · F1 · …` read back into a severity, a number and a title.
fn heading_of(line: &str) -> Option<(Severity, i64, String)> {
    let rest = line.strip_prefix("### ")?;
    let mut parts = rest.splitn(3, " · ");
    let severity: Severity = parts.next()?.to_lowercase().parse().ok()?;
    let seq: i64 = parts.next()?.strip_prefix('F')?.parse().ok()?;
    Some((severity, seq, parts.next()?.to_string()))
}
