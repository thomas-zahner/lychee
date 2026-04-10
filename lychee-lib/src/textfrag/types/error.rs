use std::str::Utf8Error;

/// Text Fragment Error codes
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
/// `TextDirective` check error statuses returned during the construction from
/// text directive passed in the `[url:Url]`'s fragment
pub enum TextFragmentError {
    /// Error indicating `FragmentDirective` delimiter is missing in the
    /// `[url:Url]`'s fragment string
    #[error("Fragment Directive delimiter missing")]
    FragmentDirectiveDelimiterMissing,

    /// Not a text directive error
    #[error("Not a Text Directive")]
    NotTextDirective,

    /// `TextDirective` is percent encoded - the error is returned if the decoding fails
    #[error("Percent decode error")]
    PercentDecodeError(String),

    /// Returns when the Text directive is not found in the content
    #[error("Text directive {0} not found")]
    TextDirectiveNotFound(String),

    /// Text directive suffix match failed error
    #[error("Suffix match error - expected {0} but matched {1}")]
    TextDirectiveRangeError(String, String),

    /// Returned when partial match is found
    #[error("Partial text directive match found!")]
    TextDirectivePartialMatchFoundError,
}

impl From<Utf8Error> for TextFragmentError {
    fn from(value: Utf8Error) -> Self {
        TextFragmentError::PercentDecodeError(value.to_string())
    }
}
