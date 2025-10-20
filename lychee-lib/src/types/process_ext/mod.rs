//! Process file extensions prior to the link extraction step.
//! In lychee-bin this behaviour is configured through the
//! `process_ext` flag.

use std::str::FromStr;

use serde_with::DeserializeFromStr;
use thiserror::Error;

/// TODO
///
/// Example: `pdf:pdftotext {} -`
#[derive(Clone, Debug, DeserializeFromStr, PartialEq, Eq, Hash)]
pub struct ProcessExt {}

impl FromStr for ProcessExt {
    type Err = ProcessExtParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        todo!("{s}")
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ProcessExtParseError {}
