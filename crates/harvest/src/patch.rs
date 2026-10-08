//! Which of a reviewed commit's lines are gone.
//!
//! A finding points at a line of the commit the review read. To tell "this finding's
//! own lines changed" from "the file changed around it", the harvester needs to know
//! which of those line numbers no longer hold what they held.
//!
//! The basis matters. A single commit's diff is relative to *its parent*, so line 873
//! in it is not line 873 of the reviewed commit once an earlier commit has inserted
//! anything. Only a comparison of the reviewed commit with the current head has the
//! reviewed commit on its old side, which is why the harvester reads one comparison
//! rather than a diff per commit.
//!
//! **A line is "changed" when it was removed or replaced, not when something was
//! inserted near it.** Inserting ten lines above a finding moves it; it does not change
//! it, and recording that as the finding's own lines changing would make the outcome
//! useless on any pull request that grew.

use std::collections::BTreeSet;

/// The line numbers on the old side of a unified diff whose content is gone.
pub fn changed_lines(patch: &str) -> BTreeSet<i64> {
    let mut changed = BTreeSet::new();
    let mut old_line: Option<i64> = None;

    for line in patch.lines() {
        if let Some(start) = hunk_old_start(line) {
            old_line = Some(start);
            continue;
        }

        let Some(line_number) = old_line else {
            // Anything before the first hunk header is a file header, not content.
            continue;
        };

        match line.as_bytes().first() {
            // Gone from the old side.
            Some(b'-') => {
                changed.insert(line_number);
                old_line = Some(line_number + 1);
            }
            // Added on the new side; the old side does not move.
            Some(b'+') => {}
            // "\ No newline at end of file" describes the line before it.
            Some(b'\\') => {}
            // Context, including an empty context line written without its space.
            _ => old_line = Some(line_number + 1),
        }
    }

    changed
}

/// The old-side start line of a hunk header, `@@ -873,7 +880,9 @@`.
fn hunk_old_start(line: &str) -> Option<i64> {
    let rest = line.strip_prefix("@@ -")?;
    let numbers = rest.split(' ').next()?;
    let start = numbers.split(',').next()?;
    start.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_line_is_changed_and_its_neighbours_are_not() {
        let patch = "\
@@ -10,5 +10,5 @@
 one
 two
-three
+THREE
 four
 five";

        assert_eq!(changed_lines(patch), BTreeSet::from([12]));
    }

    #[test]
    fn an_insertion_changes_nothing() {
        let patch = "\
@@ -10,3 +10,4 @@
 one
+inserted
 two
 three";

        assert!(
            changed_lines(patch).is_empty(),
            "a finding above an insertion moved; it did not change"
        );
    }

    #[test]
    fn several_hunks_each_count_from_their_own_header() {
        let patch = "\
@@ -1,2 +1,2 @@
-first
+FIRST
 second
@@ -100,2 +100,2 @@
 hundred
-hundred and one
+HUNDRED AND ONE";

        assert_eq!(changed_lines(patch), BTreeSet::from([1, 101]));
    }

    #[test]
    fn a_hunk_without_a_length_is_read_as_one_line() {
        let patch = "\
@@ -42 +42 @@
-forty two
+FORTY TWO";

        assert_eq!(changed_lines(patch), BTreeSet::from([42]));
    }

    #[test]
    fn a_no_newline_marker_does_not_advance_the_count() {
        let patch = "\
@@ -1,2 +1,2 @@
 kept
-last
\\ No newline at end of file
+LAST";

        assert_eq!(changed_lines(patch), BTreeSet::from([2]));
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        assert!(changed_lines("").is_empty());
    }
}
