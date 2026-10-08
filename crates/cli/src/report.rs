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

    /// Write this answer to standard output, in one shape or the other.
    pub fn print(&self, json: bool) {
        if json {
            println!("{}", self.document);
        } else if !self.plain.is_empty() {
            println!("{}", self.plain);
        }
    }
}
