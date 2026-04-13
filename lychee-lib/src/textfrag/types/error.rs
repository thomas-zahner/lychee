use crate::textfrag::FRAGMENT_DIRECTIVE_DELIMITER;
use std::str::Utf8Error;

/// Text Fragment Error codes
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
/// `TextDirective` check error statuses returned during the construction from
/// text directive passed in the `[url:Url]`'s fragment
pub enum TextFragmentError {
    /// Error indicating delimiter is missing in the URLs fragment string
    #[error("Fragment directive delimiter '{FRAGMENT_DIRECTIVE_DELIMITER}' missing")]
    FragmentDirectiveDelimiterMissing,

    /// Not a text directive error
    #[error("Not a text directive")]
    NotTextDirective,

    /// `TextDirective` is percent encoded - the error is returned if the decoding fails
    #[error("Percent decode error: {0}")]
    PercentDecodeError(String),
}

impl From<Utf8Error> for TextFragmentError {
    fn from(value: Utf8Error) -> Self {
        Self::PercentDecodeError(value.to_string())
    }
}
