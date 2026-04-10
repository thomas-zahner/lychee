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
use std::sync::LazyLock;

use fancy_regex::Regex;

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
    pub(crate) prefix: Option<String>,
    /// Start directive - If only start is given, first instance of the string
    /// specified as *start* is the target
    pub(crate) start: String,
    /// End directive - with this specified, a range of text in the page or block content
    /// is to be found.
    /// Target text range startis from *start*, until the first instance
    /// of the *end* (after *start*)
    pub(crate) end: Option<String>,
    /// Suffix directive - a contextual term to identify the text immediately after (the *end*)
    /// ends with a hyphen (-) to separate from the *end* term
    pub(crate) suffix: Option<String>,

    /// Text Directive validation Status - updated by the tokenizer state machine
    pub(crate) status: TextDirectiveStatus,
    /// Current search string - this will be dynamically updated by the tokenizer state machine
    pub(crate) search_kind: TextDirectiveKind,
    /// start offset to start searching `**search_str**` on the block element content
    pub(crate) next_offset: usize,
    /// Tokenizer resultant string - contains the found content
    pub(crate) result_str: String,
}

/// Regex to match `TextDirective` in `[url:Url]`'s fragment
static TEXT_DIRECTIVE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)                                 # Enable extended mode for whitespace and comments
        ^text=                                  # Beginning of a fragment text directive
        (?:\s*(?<prefix>[^,&-]*)-\s*[,$]?\s*)?  # Optional prefix
        (?:\s*(?<start>[^-&,]+)\s*)             # Mandatory start
        (?:\s*,\s*(?<end>[^,&-]*)\s*)?          # Optional end
        (?:\s*,\s*-(?<suffix>[^,&-]*)\s*)?      # Optional suffix
        $"#,
    )
    .unwrap()
});

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
            if self.prefix.is_some() {
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
        let Ok(Some(result)) = TEXT_DIRECTIVE_REGEX.captures(fragment) else {
            return Err(TextFragmentError::NotTextDirective);
        };

        let start = TextDirective::percent_decode(&result["start"])?;

        let mut search_kind = TextDirectiveKind::Start;

        let prefix = result
            .name("prefix")
            .map(|m| TextDirective::percent_decode(m.as_str()))
            .transpose()?;

        if prefix.is_some() {
            search_kind = TextDirectiveKind::Prefix;
        }

        let end = result
            .name("end")
            .map(|m| TextDirective::percent_decode(m.as_str()))
            .transpose()?;

        let suffix = result
            .name("suffix")
            .map(|m| TextDirective::percent_decode(m.as_str()))
            .transpose()?;

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
    }

    /// Returns the decoded string or error
    /// # Errors
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails
    fn percent_decode(input: &str) -> Result<String, TextFragmentError> {
        Ok(percent_encoding::percent_decode_str(input)
            .decode_utf8()?
            .to_string())
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
        let directive = TextDirective::from_fragment_as_str("text=repeated, block").unwrap();
        assert_eq!(
            directive,
            TextDirective {
                start: "repeated".into(),
                end: Some("block".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        let directive = TextDirective::from_fragment_as_str("text=with-,repeated").unwrap();
        assert_eq!(
            directive,
            TextDirective {
                start: "repeated".into(),
                prefix: Some("with".into()),
                search_kind: TextDirectiveKind::Prefix,
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        let directive =
            TextDirective::from_fragment_as_str("text=linked%20URL,-'s%20format").unwrap();
        assert_eq!(
            directive,
            TextDirective {
                start: "linked URL".into(),
                suffix: Some("'s format".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        let directive =
            TextDirective::from_fragment_as_str("text=with-,repeated,-instance").unwrap();
        assert_eq!(
            directive,
            TextDirective {
                prefix: Some("with".into()),
                start: "repeated".into(),
                suffix: Some("instance".into()),
                search_kind: TextDirectiveKind::Prefix,
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        let directive =
            TextDirective::from_fragment_as_str("text=with-,repeated, For, -instance").unwrap();
        assert_eq!(
            directive,
            TextDirective {
                prefix: Some("with".into()),
                start: "repeated".into(),
                end: Some("For".into()),
                suffix: Some("instance".into()),
                search_kind: TextDirectiveKind::Prefix,
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_missing_start() {
        let directive = TextDirective::from_fragment_as_str("text=suffix-");
        assert_eq!(directive, Err(TextFragmentError::NotTextDirective));
    }

    #[test]
    fn test_not_directive() {
        const FRAGMENT: &str = "prefix-";

        let directive = TextDirective::from_fragment_as_str(FRAGMENT);
        assert_eq!(directive, Err(TextFragmentError::NotTextDirective));
    }

    #[test]
    fn test_percent_decode_error() {
        const FRAGMENT: &str = "text=with%00%9F%92%96";

        let directive = TextDirective::from_fragment_as_str(FRAGMENT);
        assert_eq!(
            directive,
            Err(TextFragmentError::PercentDecodeError(
                "invalid utf-8 sequence of 1 bytes from index 5".to_string()
            ))
        );
    }
}
