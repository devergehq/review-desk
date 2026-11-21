//! The hidden tag a review posted through Review Desk carries, grammar v1 — DEV-1347.
//!
//! ```text
//! <!-- review-desk:posted v=1 install=<id> basis=typed|desk statement=hmac:<64 hex> [line=hmac:<64 hex>] -->
//! ```
//!
//! One HTML comment at the very end of the review body, at most [`TAG_MAX_BYTES`] bytes,
//! with no newline. GitHub does not draw it and returns it verbatim.
//!
//! **Everything in it is keyed with a salt only the record holds.** `statement` is
//! `HMAC-SHA256(salt, "review-desk/statement\0" ‖ JCS(approval))` and `line` is
//! `HMAC-SHA256(salt, "review-desk/line\0" ‖ NFC(trimmed line))`. The salt is 32 random
//! bytes per post, so the same approval or the same "nothing, post it" on two reviews gives
//! two unrelated values, and nobody reading the pull request can confirm a guess about
//! either. A plain `sha256:` of anything would let them, which is why a reader refuses one.
//!
//! **The reader is strict on purpose.** A tag is a claim about how a review was posted, so a
//! tag with a key this grammar does not have (`seq=`, `key=`), a `sha256:` value, or a second
//! tag in the same body is refused rather than read as far as it goes.

use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;

use crate::model::{PostBasis, ReviewEvent};

/// The longest a tag may be, in bytes.
pub const TAG_MAX_BYTES: usize = 220;

/// The schema name the approval statement carries.
pub const APPROVAL_SCHEMA: &str = "review-desk/approval@1";

/// What every tag starts with.
const OPENING: &str = "<!-- review-desk:posted";

const STATEMENT_DOMAIN: &str = "review-desk/statement";
const LINE_DOMAIN: &str = "review-desk/line";

/// What the developer approved, as the statement a post is bound to.
///
/// **`review-desk/approval@1`.** The three values DEV-1346 records at approval — the hash
/// of the body, the hash of the threads and the draft's revision — plus the identity of the
/// review they belong to: which review, on which pull request, at which commit, in which
/// round, posted as which event. Any of them differing is a different approval.
///
/// | Field | What it is |
/// |---|---|
/// | `schema` | always `review-desk/approval@1` |
/// | `review_id` | the review's id in this record |
/// | `repo` | `owner/name` |
/// | `pr` | the pull request's number |
/// | `head_sha` | the commit reviewed, which the post pins as `commit_id` |
/// | `round` | the round of review on that pull request, from 1 |
/// | `event` | `comment`, `request_changes` or `approve` |
/// | `body_sha256` | SHA-256 of the approved body, lower-case hex |
/// | `threads_sha256` | SHA-256 of the approved threads' JSON, lower-case hex |
/// | `draft_rev` | the draft's revision when it was approved |
///
/// Serialised with [`jcs`] — RFC 8785 — so the bytes the HMAC is taken over do not depend
/// on field order or whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalStatement {
    pub schema: &'static str,
    pub review_id: i64,
    pub repo: String,
    pub pr: i64,
    pub head_sha: String,
    pub round: i64,
    pub event: ReviewEvent,
    pub body_sha256: String,
    pub threads_sha256: String,
    pub draft_rev: i64,
}

impl ApprovalStatement {
    /// The statement's canonical bytes.
    pub fn canonical(&self) -> String {
        // Every field is a string or an integer well inside 2^53, so `jcs` cannot refuse it.
        jcs(&serde_json::to_value(self).unwrap_or(Value::Null)).unwrap_or_default()
    }
}

/// RFC 8785 (JCS) canonical JSON, for the values this crate canonicalises.
///
/// Object members sorted by their UTF-16 code units, no insignificant whitespace, strings
/// escaped as ECMAScript's `JSON.stringify` escapes them — which is what `serde_json` writes:
/// `"`, `\` and the C0 controls only, the short forms where there is one and lower-case
/// `\u00xx` otherwise — and integers in their shortest form.
///
/// **Floating-point numbers are refused rather than approximated.** JCS writes them in
/// ECMAScript's shortest round-trip form, and nothing here has one; `None` is the honest
/// answer to a value this was not written for.
pub fn jcs(value: &Value) -> Option<String> {
    let mut out = String::new();
    write_jcs(value, &mut out)?;
    Some(out)
}

fn write_jcs(value: &Value, out: &mut String) -> Option<()> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => {
            out.push_str(&serde_json::to_string(value).ok()?)
        }
        Value::Number(number) => {
            // Integers beyond 2^53 are not exact in ECMAScript, and JCS is defined there.
            const SAFE: u64 = (1 << 53) - 1;
            match (number.as_i64(), number.as_u64()) {
                (Some(int), _) if int.unsigned_abs() <= SAFE => out.push_str(&int.to_string()),
                (None, Some(int)) if int <= SAFE => out.push_str(&int.to_string()),
                _ => return None,
            }
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_jcs(item, out)?;
            }
            out.push(']');
        }
        Value::Object(members) => {
            let mut keys: Vec<&String> = members.keys().collect();
            keys.sort_by(|one, two| one.encode_utf16().cmp(two.encode_utf16()));
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).ok()?);
                out.push(':');
                write_jcs(&members[key], out)?;
            }
            out.push('}');
        }
    }
    Some(())
}

/// `HMAC-SHA256(salt, domain ‖ 0x00 ‖ message)`, lower-case hex.
fn keyed(salt: &[u8], domain: &str, message: &[u8]) -> String {
    // HMAC takes a key of any length, so `new_from_slice` cannot fail here.
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(salt).expect("HMAC takes any key");
    mac.update(domain.as_bytes());
    mac.update(&[0]);
    mac.update(message);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The tag's `statement` value, without its `hmac:` prefix.
pub fn statement_hmac(salt: &[u8], canonical_statement: &str) -> String {
    keyed(salt, STATEMENT_DOMAIN, canonical_statement.as_bytes())
}

/// The go-ahead line as it is hashed and kept: trimmed, then NFC.
pub fn normalise_line(line: &str) -> String {
    line.trim().nfc().collect()
}

/// The tag's `line` value, without its `hmac:` prefix.
pub fn line_hmac(salt: &[u8], line: &str) -> String {
    keyed(salt, LINE_DOMAIN, normalise_line(line).as_bytes())
}

/// One tag's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub install: String,
    pub basis: PostBasis,
    /// 64 lower-case hex characters.
    pub statement_hmac: String,
    /// 64 lower-case hex characters, when a go-ahead line was given.
    pub line_hmac: Option<String>,
}

impl Tag {
    /// The tag as it is appended to a body.
    pub fn render(&self) -> String {
        let mut tag = format!(
            "{OPENING} v=1 install={} basis={} statement=hmac:{}",
            self.install, self.basis, self.statement_hmac
        );
        if let Some(line) = &self.line_hmac {
            tag.push_str(&format!(" line=hmac:{line}"));
        }
        tag.push_str(" -->");
        tag
    }
}

/// The body that is posted: the approved body, then a blank line, then the tag.
pub fn tagged_body(body: &str, tag: &Tag) -> String {
    format!("{}\n\n{}", body.trim_end(), tag.render())
}

/// Read the tag out of a posted review's body.
///
/// `Ok(None)` is a body with no tag. `Err` is a body that has something claiming to be one
/// and is refused, with why.
pub fn parse(body: &str) -> Result<Option<Tag>, String> {
    let openings: Vec<usize> = body.match_indices(OPENING).map(|(at, _)| at).collect();
    let Some(&last) = openings.last() else {
        return Ok(None);
    };

    // Whether the comment opening at `at` reads, on its own, as a complete tag.
    let complete = |at: usize| {
        let rest = &body[at..];
        rest.find("-->")
            .is_some_and(|end| fields(&rest[..end + "-->".len()]).is_ok())
    };

    let rest = &body[last..];
    let end = rest.find("-->").map(|end| end + "-->".len());
    let at_the_end = end.is_some_and(|end| rest[end..].trim().is_empty());

    // **A mention of the grammar is not a tag.** Prose or a code span describing it —
    // `install=<id>` — reads as nothing, so a review that talks about tags is "no tag" and
    // can still carry one. Only something that parses as a complete tag counts.
    if !at_the_end {
        return if openings.iter().any(|&at| complete(at)) {
            Err("the tag is not at the end of the body".to_string())
        } else {
            Ok(None)
        };
    }
    if openings[..openings.len() - 1]
        .iter()
        .any(|&at| complete(at))
    {
        return Err("the body carries two tags, and a review is posted once".to_string());
    }
    fields(&rest[..end.unwrap_or(rest.len())]).map(Some)
}

/// One complete tag's fields, or why it is not one.
fn fields(tag: &str) -> Result<Tag, String> {
    if tag.len() > TAG_MAX_BYTES {
        return Err(format!(
            "the tag is {} bytes, and v1 allows {TAG_MAX_BYTES}",
            tag.len()
        ));
    }
    if tag.contains(['\n', '\r']) {
        return Err("the tag spans more than one line".to_string());
    }

    let inner = tag[OPENING.len()..tag.len() - "-->".len()].trim_end_matches(' ');
    let Some(inner) = inner.strip_prefix(' ') else {
        return Err("the tag has no fields".to_string());
    };

    let mut fields: Vec<(&str, &str)> = Vec::new();
    for token in inner.split(' ') {
        let (key, value) = token
            .split_once('=')
            .ok_or_else(|| format!("`{token}` is not key=value"))?;
        if !matches!(key, "v" | "install" | "basis" | "statement" | "line") {
            return Err(format!("`{key}=` is not a key in tag v1"));
        }
        if value.starts_with("sha256:") {
            return Err(format!(
                "`{key}` is a plain sha256, which anyone could recompute; v1 values are hmac:"
            ));
        }
        if fields.iter().any(|(seen, _)| *seen == key) {
            return Err(format!("`{key}=` appears twice"));
        }
        fields.push((key, value));
    }

    let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
    if keys != ["v", "install", "basis", "statement"]
        && keys != ["v", "install", "basis", "statement", "line"]
    {
        return Err(format!(
            "the fields are {}, and v1 is v, install, basis, statement and an optional line, \
             in that order",
            keys.join(", ")
        ));
    }

    let value = |key: &str| {
        fields
            .iter()
            .find(|(seen, _)| *seen == key)
            .map(|(_, value)| *value)
    };

    if value("v") != Some("1") {
        return Err("only v=1 is read".to_string());
    }
    let install = value("install").unwrap_or_default();
    if install.len() != 12
        || !install
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("`install={install}` is not an install id"));
    }
    let basis = value("basis")
        .unwrap_or_default()
        .parse::<PostBasis>()
        .map_err(|_| "`basis` is neither typed nor desk".to_string())?;
    let statement = hmac_value("statement", value("statement").unwrap_or_default())?;
    let line = value("line")
        .map(|line| hmac_value("line", line))
        .transpose()?;

    Ok(Tag {
        install: install.to_string(),
        basis,
        statement_hmac: statement,
        line_hmac: line,
    })
}

/// `hmac:<64 lower-case hex>`, or why not.
fn hmac_value(key: &str, value: &str) -> Result<String, String> {
    let hex = value
        .strip_prefix("hmac:")
        .ok_or_else(|| format!("`{key}` is not an hmac: value"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("`{key}` is not 64 lower-case hex characters"));
    }
    Ok(hex.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_tag(line: bool) -> Tag {
        Tag {
            install: "0123456789ab".to_string(),
            basis: PostBasis::Typed,
            statement_hmac: "a".repeat(64),
            line_hmac: line.then(|| "b".repeat(64)),
        }
    }

    /// **The longest tag v1 can write is inside 220 bytes, on one line.** `typed` is the
    /// longer basis and a line is the optional field, so this is the worst case.
    #[test]
    fn the_longest_tag_fits_in_220_bytes_on_one_line() {
        let tag = a_tag(true).render();
        assert!(tag.len() <= TAG_MAX_BYTES, "{} bytes: {tag}", tag.len());
        assert!(!tag.contains('\n'));
        assert_eq!(
            parse(&tagged_body("Body.", &a_tag(true))),
            Ok(Some(a_tag(true)))
        );
        assert_eq!(
            parse(&tagged_body("Body.", &a_tag(false))),
            Ok(Some(a_tag(false)))
        );
    }

    /// **A review that talks about the grammar can still carry a tag.** Only an earlier
    /// comment that parses as a complete tag counts as a second one.
    #[test]
    fn a_mention_of_the_grammar_is_not_a_second_tag() {
        let mention = "The tag reads `<!-- review-desk:posted v=1 install=<id> basis=desk \
                       statement=hmac:<64 hex> -->`, and review-desk:posted is its name.";
        let body = tagged_body(mention, &a_tag(false));
        assert_eq!(parse(&body), Ok(Some(a_tag(false))));
        assert_eq!(parse(mention), Ok(None), "a mention alone is no tag");
    }

    #[test]
    fn an_install_id_that_is_not_twelve_characters_is_refused() {
        let good = a_tag(false).render();
        assert!(parse(&good.replace("install=0123456789ab", "install=0123456789abc")).is_err());
        assert!(parse(&good.replace("install=0123456789ab", "install=0123456789a")).is_err());
    }

    #[test]
    fn a_body_with_no_tag_has_none() {
        assert_eq!(parse("Looks good.\n"), Ok(None));
    }

    /// The ticket's four refusals, each on an otherwise well-formed tag.
    #[test]
    fn seq_key_sha256_and_a_second_tag_are_refused() {
        let good = a_tag(false).render();
        let seq = good.replace(" -->", " seq=2 -->");
        let key = good.replace(" -->", " key=abc -->");
        let sha = good.replace("statement=hmac:", "statement=sha256:");
        let twice = format!("{good}\n\n{good}");

        for (what, body) in [
            ("seq", seq),
            ("key", key),
            ("sha256", sha),
            ("twice", twice),
        ] {
            assert!(parse(&body).is_err(), "{what} was read: {body}");
        }
    }

    #[test]
    fn a_tag_not_at_the_end_or_over_two_lines_is_refused() {
        let good = a_tag(false).render();
        assert!(parse(&format!("{good}\n\nMore text.")).is_err());
        assert!(parse(&good.replace(" basis=", "\nbasis=")).is_err());
        assert!(parse(&good.replace("v=1", "v=2")).is_err());
    }

    /// RFC 8785's ordering is by UTF-16 code unit, which differs from UTF-8 byte order
    /// above the BMP; and its strings escape only what JSON requires.
    #[test]
    fn jcs_sorts_by_utf16_and_writes_no_whitespace() {
        let value = serde_json::json!({
            "b": [1, "two", null, true],
            "a": "line\nbreak \u{1f} é",
            "\u{ff61}": 1,
            "\u{1f600}": 2,
        });
        assert_eq!(
            jcs(&value).unwrap(),
            "{\"a\":\"line\\nbreak \\u001f é\",\"b\":[1,\"two\",null,true],\
             \"\u{1f600}\":2,\"\u{ff61}\":1}"
        );
        assert_eq!(jcs(&serde_json::json!(1.5)), None, "a float is refused");
    }

    #[test]
    fn the_line_is_trimmed_and_normalised_before_it_is_keyed() {
        let salt = [7u8; 32];
        // "é" as one code point and as e + combining acute are the same line.
        assert_eq!(
            line_hmac(&salt, "  caf\u{e9}, post it \n"),
            line_hmac(&salt, "cafe\u{301}, post it")
        );
        assert_ne!(
            line_hmac(&salt, "post it"),
            line_hmac(&[8u8; 32], "post it")
        );
    }

    /// The two domains keep a statement and a line apart even over the same bytes.
    #[test]
    fn a_statement_and_a_line_over_the_same_text_differ() {
        let salt = [1u8; 32];
        assert_ne!(statement_hmac(&salt, "x"), line_hmac(&salt, "x"));
    }
}
