//! A review's seven layers.

mod common;

use common::{new_review, temp_store};
use review_desk_store::{ReviewLayerName, ReviewLayerState, StoreError};

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
    // The four screens show seven, so a reader always gets seven. An absent row
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
