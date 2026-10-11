//! A review's seven layers.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{NewLayerFact, ReviewLayerName, ReviewLayerState, StoreError};

#[test]
fn a_layer_is_written_and_read_back_with_the_name_its_number_fixes() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    let written = store
        .set_review_layer(review.id, 5, ReviewLayerState::Running, Some("6 findings"))
        .expect("set layer");

    assert_eq!(written.number, 5);
    assert_eq!(
        written.name,
        ReviewLayerName::CodeReview,
        "the number fixes the name, so a row cannot claim layer 5 is the lint"
    );
    assert_eq!(written.state, ReviewLayerState::Running);
    assert_eq!(written.detail.as_deref(), Some("6 findings"));
    assert!(written.updated_at.is_some());
    assert_eq!(written, store.get_review_layer(review.id, 5).unwrap());
}

#[test]
fn writing_a_layer_again_replaces_what_it_said() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    store
        .set_review_layer(review.id, 6, ReviewLayerState::Running, Some("writing"))
        .unwrap();
    let then = store
        .set_review_layer(
            review.id,
            6,
            ReviewLayerState::WaitingOnYou,
            Some("waiting on you"),
        )
        .unwrap();

    assert_eq!(then.state, ReviewLayerState::WaitingOnYou);
    assert_eq!(then.detail.as_deref(), Some("waiting on you"));
    assert_eq!(
        store.list_review_layers(review.id).unwrap()[5],
        then,
        "a layer reports progress without the caller checking first"
    );
}

#[test]
fn every_review_has_seven_layers_and_an_unwritten_one_is_pending() {
    // The five screens show seven, so a reader always gets seven. An absent row
    // meaning "not started" is knowledge no caller should need.
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    store
        .set_review_layer(review.id, 1, ReviewLayerState::Failed, Some("2 errors"))
        .unwrap();
    store
        .set_review_layer(review.id, 4, ReviewLayerState::Skipped, Some("no diagram"))
        .unwrap();

    let layers = store.list_review_layers(review.id).unwrap();

    assert_eq!(layers.len(), ReviewLayerName::COUNT as usize);
    assert_eq!(
        layers.iter().map(|layer| layer.number).collect::<Vec<_>>(),
        (1..=7).collect::<Vec<_>>(),
        "in order"
    );
    assert_eq!(
        layers.iter().map(|layer| layer.name).collect::<Vec<_>>(),
        ReviewLayerName::ALL.to_vec()
    );

    assert_eq!(layers[0].state, ReviewLayerState::Failed);
    assert_eq!(
        layers[3].state,
        ReviewLayerState::Skipped,
        "a skipped layer is not a layer that has not run"
    );
    for pending in [1, 2, 4, 5, 6] {
        assert_eq!(layers[pending].state, ReviewLayerState::Pending);
        assert_eq!(layers[pending].detail, None);
        assert_eq!(
            layers[pending].updated_at, None,
            "nothing has written it, so there is no time to report"
        );
    }
}

#[test]
fn a_layer_outside_the_seven_is_refused_with_the_range_named() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    for number in [0, 8, -1, i64::MAX] {
        match store.set_review_layer(review.id, number, ReviewLayerState::Done, None) {
            Err(StoreError::InvalidValue {
                field, expected, ..
            }) => {
                assert_eq!(field, "layer");
                assert!(expected.contains("1 to 7"), "{expected}");
            }
            other => panic!("layer {number} must be refused, got {other:?}"),
        }
    }
}

#[test]
fn a_layer_of_a_review_that_is_not_there_is_an_unknown_parent() {
    let (_dir, store) = temp_store();

    match store.set_review_layer(999_999, 1, ReviewLayerState::Done, None) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!((entity, parent), ("review layer", "review"));
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn an_unwritten_layer_is_not_found_on_its_own() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    assert_eq!(store.find_review_layer(review.id, 3).unwrap(), None);
    match store.get_review_layer(review.id, 3) {
        Err(StoreError::NotFound { entity, .. }) => assert_eq!(entity, "review layer"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn a_layer_number_and_its_name_agree_in_both_directions() {
    for (index, name) in ReviewLayerName::ALL.iter().enumerate() {
        let number = index as i64 + 1;
        assert_eq!(name.number(), number);
        assert_eq!(ReviewLayerName::for_number(number).unwrap(), *name);
    }
    assert_eq!(
        ReviewLayerName::ALL.len() as i64,
        ReviewLayerName::COUNT,
        "the count and the set of names cannot drift apart"
    );
}

/* ---------------------------------------------------------------------------------------
 * The facts a step recorded — DEV-917.
 *
 * **What the one-line `detail` is a summary of.** The trail draws `detail` on one line and
 * opens these on a card beside the step; before this, a step's whole note was printed in a
 * column about 150 pixels wide.
 * ------------------------------------------------------------------------------------- */

#[test]
fn a_steps_facts_are_read_back_in_the_order_they_were_written() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    store
        .set_review_layer(review.id, 3, ReviewLayerState::Done, Some("8 options"))
        .unwrap();

    let written = store
        .put_review_layer_facts(
            review.id,
            3,
            vec![
                fact("On the ticket", "3"),
                fact("From the blind pass", "6"),
                fact("Built", "C"),
            ],
        )
        .expect("put facts");

    assert_eq!(
        written
            .iter()
            .map(|one| (one.position, one.label.as_str(), one.value.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "On the ticket", "3"),
            (2, "From the blind pass", "6"),
            (3, "Built", "C"),
        ],
        "the position is the store's to assign, from the order written"
    );
    assert_eq!(
        store.get_review_layer(review.id, 3).unwrap().facts,
        written,
        "a step carries its own facts when it is read"
    );
    assert_eq!(
        store.list_review_layers(review.id).unwrap()[2].facts,
        written,
        "and when all seven are read"
    );
}

#[test]
fn a_step_that_recorded_no_facts_has_none() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    let written = store
        .set_review_layer(review.id, 1, ReviewLayerState::Done, Some("passed"))
        .unwrap();

    assert!(
        written.facts.is_empty(),
        "which is every step of every review before this change, and draws no card"
    );
    for layer in store.list_review_layers(review.id).unwrap() {
        assert!(layer.facts.is_empty());
    }
}

#[test]
fn writing_facts_replaces_the_whole_list_and_an_empty_list_clears_it() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    store
        .set_review_layer(review.id, 5, ReviewLayerState::Done, Some("14 findings"))
        .unwrap();

    store
        .put_review_layer_facts(
            review.id,
            5,
            vec![fact("To post", "6"), fact("Set aside", "3")],
        )
        .unwrap();
    let again = store
        .put_review_layer_facts(review.id, 5, vec![fact("To post", "5")])
        .unwrap();

    assert_eq!(again.len(), 1, "the list is the statement, not an addition");
    assert_eq!(again[0].value, "5");

    assert!(
        store
            .put_review_layer_facts(review.id, 5, Vec::new())
            .unwrap()
            .is_empty(),
        "an empty list is how a step takes its facts back off the record"
    );
}

#[test]
fn reporting_a_step_again_leaves_its_facts_alone() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    store
        .set_review_layer(review.id, 2, ReviewLayerState::Running, Some("reading"))
        .unwrap();
    store
        .put_review_layer_facts(review.id, 2, vec![fact("Problems", "3")])
        .unwrap();

    /*
     * The call a session makes over and over while it works. Facts cleared by a call that
     * said nothing about them would be facts no session could keep.
     */
    let later = store
        .set_review_layer(
            review.id,
            2,
            ReviewLayerState::WaitingOnYou,
            Some("waiting on you"),
        )
        .unwrap();

    assert_eq!(later.facts.len(), 1);
    assert_eq!(later.facts[0].label, "Problems");
}

#[test]
fn a_fact_with_no_label_or_no_value_is_refused_naming_the_field() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();
    store
        .set_review_layer(review.id, 4, ReviewLayerState::Done, Some("right shape"))
        .unwrap();

    match store.put_review_layer_facts(review.id, 4, vec![fact("   ", "3")]) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "facts.label"),
        other => panic!("expected InvalidValue, got {other:?}"),
    }
    match store.put_review_layer_facts(review.id, 4, vec![fact("Problems", "")]) {
        Err(StoreError::InvalidValue { field, .. }) => assert_eq!(field, "facts.value"),
        other => panic!("expected InvalidValue, got {other:?}"),
    }
    assert!(
        store
            .get_review_layer(review.id, 4)
            .unwrap()
            .facts
            .is_empty(),
        "a refused list leaves the step's facts as they were"
    );
}

#[test]
fn facts_about_a_step_nothing_has_written_are_refused() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    match store.put_review_layer_facts(review.id, 7, vec![fact("Posted", "not yet")]) {
        Err(StoreError::UnknownParent { entity, parent, .. }) => {
            assert_eq!(entity, "layer fact");
            assert_eq!(parent, "review layer");
        }
        other => panic!("expected UnknownParent, got {other:?}"),
    }
}

#[test]
fn an_eighth_steps_facts_are_refused_with_the_range_named() {
    let (_dir, store) = temp_store();
    let review = store
        .create_review(new_review("acme/portal", 412, "3f9c2ab", 1))
        .unwrap();

    match store.put_review_layer_facts(review.id, 8, vec![fact("Problems", "3")]) {
        Err(StoreError::InvalidValue { expected, .. }) => {
            assert!(
                expected.contains('7'),
                "the valid range is named: {expected}"
            )
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }
}

/// One fact to write.
fn fact(label: &str, value: &str) -> NewLayerFact {
    NewLayerFact {
        label: label.to_string(),
        value: value.to_string(),
    }
}
