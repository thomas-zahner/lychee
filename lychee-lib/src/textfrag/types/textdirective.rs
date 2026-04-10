//! This module defines the `TextDirective` struct, which represents a range of text in a
//! web page for highlighting to the user.
//!
//! The syntax for a text directive is `text=[prefix-,]start[,end][,-suffix]`
//!
//! * `start` is required to be non-null, with the other three terms marked as optional.
//! * An empty string is NOT valid for any of the directive items.
//! * `start` with `end` constitutes a text range.
//! * `prefix` and `suffix` are contextual terms and are not part of the text fragments to search and gather.
//!
//! NOTE: Directives are percent-encoded by the caller. The `TextDirective` will return
//! percent-decoded directives.
//!
//! # Example
//!
//! ```rust
//! use lychee_lib::textfrag::types::TextDirective;
//!
//!
//! let fragment = "text=prefix-,start,end,-suffix";
//! let text_directive = TextDirective::from_fragment_as_str(fragment).unwrap();
//! println!("Prefix: {}", text_directive.prefix());
//! println!("Start: {}", text_directive.start());
//! println!("End: {}", text_directive.end());
//! println!("Suffix: {}", text_directive.suffix());
//!
//! ```
use fancy_regex::Regex;
use percent_encoding::percent_decode_str;

use crate::textfrag::types::{
    TextDirectiveKind, error::TextFragmentError, status::TextDirectiveStatus,
};

/// Text Directive represents the range of text in the web-page for highlighting to the user
/// with the syntax
///     text=[prefix-,]start[,end][,-suffix]
/// *start* is required to be non-null with the other three terms marked as optional.
/// Empty string is NOT valid for all of the directive items
/// *start* with *end* constitutes a text range
/// *prefix* and *suffix* are contextual terms and they are not part of the text fragments to
/// search and gather
///
/// NOTE: directives are percent-encoded by the caller
/// Text Directive will return percent-decoded directives
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextDirective {
    /// Prefix directive - a contextual term to help identity text immediately before (the *start*)
    /// the directive ends with a hyphen (-) to separate from the *start* term
    /// starts on the word boundary
    /// OPTIONAL
    pub(crate) prefix: String,
    /// Start directive - If only start is given, first instance of the string
    /// specified as *start* is the target
    /// MANDATORY
    pub(crate) start: String,
    /// End directive - with this specified, a range of text in the page or block content
    /// is to be found.
    /// Target text range startis from *start*, until the first instance
    /// of the *end* (after *start*)
    /// OPTIONAL
    pub(crate) end: String,
    /// Suffix directive - a contextual term to identify the text immediately after (the *end*)
    /// ends with a hyphen (-) to separate from the *end* term
    /// OPTIONAL
    pub(crate) suffix: String,

    /// Text Directive validation Status - updated by the tokenizer state machine
    pub(crate) status: TextDirectiveStatus,
    /// Current search string - this will be dynamically updated by the tokenizer state machine
    pub(crate) search_kind: TextDirectiveKind,
    /// start offset to start searching `**search_str**` on the block element content
    pub(crate) next_offset: usize,
    /// Tokenizer resultant string - contains the found content
    pub(crate) result_str: String,
}

/// `TextDirective` delimiter
pub(crate) const TEXT_DIRECTIVE_DELIMITER: &str = "text=";

/// Regex to match `TextDirective` in `[url:Url]`'s fragment
const TEXT_DIRECTIVE_REGEX: &str = r"(?s)^text=(?:\s*(?P<prefix>[^,&-]*)-\s*[,$]?\s*)?(?:\s*(?P<start>[^-&,]*)\s*)(?:\s*,\s*(?P<end>[^,&-]*)\s*)?(?:\s*,\s*-(?P<suffix>[^,&-]*)\s*)?$";

/// Text Directive getters and setters
impl TextDirective {
    /// Resets the `TextDirective` state information.
    ///
    /// When called with current search directive as `End` element, the reset will
    /// skip over the resultant string content, in anticipation that the `End`
    /// shall span over and be found across blocks.
    ///
    /// For rest of the directive state, reset will force restart of the search.
    pub(crate) fn reset(&mut self) {
        // reset the search kind, and offset fields
        self.next_offset = 0;
        self.status = TextDirectiveStatus::NotFound;

        // End directive can span across blocks (rest other directives MUST be on the same block)
        // If the next directive is End, we retain the resultant string found so far
        if TextDirectiveKind::End != self.search_kind {
            self.result_str.clear();

            // Restart the search
            self.search_kind = TextDirectiveKind::Start;
            if !self.prefix.is_empty() {
                self.search_kind = TextDirectiveKind::Prefix;
            }
        }
    }

    /// Update resultant string content (padding with whitespace, for readability)
    pub(crate) fn append_result_str(&mut self, content: &str) {
        self.result_str.push_str(&format!("{content} "));
    }
}

/// Implementation of `TextDirective` object construction from `[url:Url]`'s fragment and
/// percent decode support method.
impl TextDirective {
    /// Percent decode the input string
    /// Returns the decoded string or error
    /// # Errors
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails
    fn percent_decode(input: &str) -> Result<String, TextFragmentError> {
        let decode = percent_decode_str(input).decode_utf8();

        match decode {
            Ok(decode) => Ok(decode.to_string()),
            Err(e) => Err(TextFragmentError::PercentDecodeError(e.to_string())),
        }
    }

    /// Extract `TextDirective` from fragment string
    ///
    /// Text directives are percent encoded; we'll extract the directives first
    /// and will decode the extracted directives
    ///
    /// Start is MANDATORY field - cannot be empty
    /// end, prefix & suffix are optional
    ///
    /// # Errors
    /// - `TextFragmentError::NotTextDirective`, if delimiter (text=) is missing
    /// - `TextFragmentError::RegexNoCaptureError`, if the regex capture returns empty
    /// - `TextFragmentError::StartDirectiveMissingError`, if the *start* is missing in the directives
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails for the directive
    ///
    pub(crate) fn from_fragment_as_str(fragment: &str) -> Result<TextDirective, TextFragmentError> {
        // If text directive delimiter (text=) is not found, return error
        if !fragment.contains(TEXT_DIRECTIVE_DELIMITER) {
            return Err(TextFragmentError::NotTextDirective);
        }

        if let Ok(regex) = Regex::new(TEXT_DIRECTIVE_REGEX) {
            if let Ok(Some(result)) = regex.captures(fragment) {
                let start = result
                    .name("start")
                    .map(|start| start.as_str())
                    .unwrap_or_default();
                let start = TextDirective::percent_decode(start)?;

                // Start is MANDATORY - check for valid directive input
                if start.is_empty() {
                    return Err(TextFragmentError::StartDirectiveMissingError);
                }

                let mut search_kind = TextDirectiveKind::Start;

                let prefix = result
                    .name("prefix")
                    .map(|m| m.as_str())
                    .unwrap_or_default();
                let prefix = TextDirective::percent_decode(prefix)?;
                if !prefix.is_empty() {
                    search_kind = TextDirectiveKind::Prefix;
                }

                let end = result.name("end").map(|e| e.as_str()).unwrap_or_default();
                let end = TextDirective::percent_decode(end)?;

                let suffix = result
                    .name("suffix")
                    .map(|m| m.as_str())
                    .unwrap_or_default();
                let suffix = TextDirective::percent_decode(suffix)?;

                Ok(TextDirective {
                    prefix,
                    start,
                    end,
                    suffix,
                    status: TextDirectiveStatus::NotStarted,
                    search_kind,
                    next_offset: 0,
                    result_str: String::new(),
                })
            } else {
                Err(TextFragmentError::RegexCaptureError(
                    fragment.to_string(),
                    TEXT_DIRECTIVE_REGEX.to_string(),
                ))
            }
        } else {
            log::error!(
                "Error constructing the regex object: {}",
                TEXT_DIRECTIVE_REGEX
            );
            Err(TextFragmentError::RegexConsructionError(
                TEXT_DIRECTIVE_REGEX.to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::textfrag::types::{TextDirective, TextDirectiveKind, TextFragmentError};

    #[test]
    fn test_fragment_directive_start_only() {
        const FRAGMENT: &str = "text=repeated";

        let td = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(td.start, "repeated");
        assert_eq!(td.search_kind, TextDirectiveKind::Start);
    }

    #[test]
    fn test_fragment_directive_start_end() {
        const FRAGMENT: &str = "text=repeated, block";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(tdirective.start, "repeated");
        assert_eq!(tdirective.end, "block");
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        const FRAGMENT: &str = "text=with-,repeated";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(tdirective.prefix, "with");
        assert_eq!(tdirective.start, "repeated");
        assert_eq!(tdirective.search_kind, TextDirectiveKind::Prefix);
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        const FRAGMENT: &str = "text=linked%20URL,-'s%20format";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(tdirective.start, "linked URL");
        assert_eq!(tdirective.suffix, "'s format");
        assert_eq!(tdirective.search_kind, TextDirectiveKind::Start);
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        const FRAGMENT: &str = "text=with-,repeated,-instance";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(tdirective.prefix, "with");
        assert_eq!(tdirective.start, "repeated");
        assert_eq!(tdirective.suffix, "instance");
        assert_eq!(tdirective.search_kind, TextDirectiveKind::Prefix);
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        const FRAGMENT: &str = "text=with-,repeated, For, -instance";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT).unwrap();
        assert_eq!(tdirective.prefix, "with");
        assert_eq!(tdirective.start, "repeated");
        assert_eq!(tdirective.suffix, "instance");
        assert_eq!(tdirective.end, "For");
        assert_eq!(tdirective.search_kind, TextDirectiveKind::Prefix);
    }

    #[test]
    fn test_missing_start() {
        const FRAGMENT: &str = "text=suffix-";
        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert_eq!(
            tdirective,
            Err(TextFragmentError::StartDirectiveMissingError)
        );
    }

    #[test]
    fn test_not_directive() {
        const FRAGMENT: &str = "prefix-";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert_eq!(tdirective, Err(TextFragmentError::NotTextDirective));
    }

    #[test]
    fn test_percent_decode_error() {
        const FRAGMENT: &str = "text=with%00%9F%92%96";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert_eq!(
            tdirective,
            Err(TextFragmentError::PercentDecodeError(
                "invalid utf-8 sequence of 1 bytes from index 5".to_string()
            ))
        );
    }
}
