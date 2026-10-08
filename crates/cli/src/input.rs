//! Reading the JSON a command was given.
//!
//! From a file, or from standard input when the path is `-`, so an agent can pipe a
//! finding straight in without writing a temporary file.

use std::io::Read;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::error::{CliError, Result};

/// The path that means standard input.
const STDIN: &str = "-";

/// Read one JSON document from `path`, or from standard input when `path` is `-`.
///
/// `closed_sets`, when given, is appended to a refusal from the *parse*, naming what the
/// document could have said. serde names a closed set's values when the value is an
/// unknown string, but not when it is the wrong type and not when the field is missing —
/// and all three are an invalid value the caller has to be told about.
///
/// It is not appended to a refusal from the read: naming the valid severities on "no such
/// file" points the reader at the wrong mistake.
pub fn read_json<T: DeserializeOwned>(path: &Path, closed_sets: Option<&str>) -> Result<T> {
    let (source, text) = if path == Path::new(STDIN) {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| CliError::bad_input("cannot read standard input", error))?;
        ("standard input".to_string(), text)
    } else {
        let text = std::fs::read_to_string(path).map_err(|error| {
            CliError::bad_input(format!("cannot read {}", path.display()), error)
        })?;
        (path.display().to_string(), text)
    };

    serde_json::from_str(&text).map_err(|error| {
        let refusal = CliError::bad_input(format!("cannot use the JSON in {source}"), error);
        match closed_sets {
            Some(values) => refusal.naming(values),
            None => refusal,
        }
    })
}
