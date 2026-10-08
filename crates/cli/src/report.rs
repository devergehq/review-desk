//! What a command has to say, in both the shapes the command line can print it.
//!
//! Every command builds one of these and returns it. Printing happens in one place, so
//! no command can accidentally write text to standard output in the middle of a JSON
//! document.

use serde_json::Value;

/// One command's answer.
#[derive(Debug)]
pub struct Report {
    document: Value,
    plain: String,
}

impl Report {
    /// An answer, as the JSON document and as the short line a person reads.
    pub fn new(document: Value, plain: impl Into<String>) -> Self {
        Report {
            document,
            plain: plain.into(),
        }
    }

    /// An answer that has already been given.
    ///
    /// `serve` prints where the server is before it starts serving, because a report
    /// printed when it returns would be printed when the server stopped. This is what it
    /// returns afterwards: printing a second document would break the promise that
    /// `--json` writes exactly one.
    pub fn silent() -> Self {
        Report {
            document: Value::Null,
            plain: String::new(),
        }
    }

    /// Write this answer to standard output, in one shape or the other.
    pub fn print(&self, json: bool) {
        if self.document.is_null() && self.plain.is_empty() {
            return;
        }

        if json {
            println!("{}", self.document);
        } else if !self.plain.is_empty() {
            println!("{}", self.plain);
        }
    }
}
