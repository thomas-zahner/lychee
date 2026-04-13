//! Fragment Directive object is collection of text fragments in the URL's fragment
//! The delimiter `:~:` is the fragment directive delmiter to separate a list of `[TextDirective]`'s
//! This module defines the functionality to parse, construct and store the `[TextDirective]`'s defined in
//! `[url:Url]`'s fragment.
//!
//! # Example
//!
//! ```rust
//!
//! use url::Url;
//! use lychee_lib::textfrag::types::FragmentDirective;
//!
//! let url = Url::parse("https://example.com/#:~:text=prefix-,start,end,-suffix").unwrap();
//! if let Some(fragment_directive) = FragmentDirective::from_url(&url) {
//!    let directives = fragment_directive.text_directives();
//!    for directive in directives {
//!        println!("Directive: {:?}", directive);
//!    }
//! }
//! ```

use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
use url::Url;

use crate::textfrag::{
    extract::FragmentDirectiveTokenizer,
    types::{FragmentDirectiveError, TextDirective, TextDirectiveStatus, TextFragmentError},
};

/// Fragment directive delimiter constant
pub(crate) const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";

/// Fragment Directive defines the base url and collection of Text Directives
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct FragmentDirective {
    pub(crate) text_directives: Vec<TextDirective>,
}

impl FragmentDirective {
    /// Extract Text Directives, from the Url fragment string, and return a list
    /// of `TextDirective`'s as vector
    ///
    /// The method supports multiple text directives - each text directive is delimited by `&`.
    /// If the text directive is malformed, the method will skip over it and continue processing
    /// the next text directive.
    ///
    /// # Errors
    /// - `FragmentDirectiveDelimiterMissing` - if the fragment directive delimiter is not found
    /// in the `[url:Url]`'s fragment
    fn build_text_directives(fragment: &str) -> Result<Vec<TextDirective>, TextFragmentError> {
        let Some(offset) = fragment.find(FRAGMENT_DIRECTIVE_DELIMITER) else {
            return Err(TextFragmentError::FragmentDirectiveDelimiterMissing);
        };

        fragment[offset + FRAGMENT_DIRECTIVE_DELIMITER.len()..]
            .split('&')
            .map(TextDirective::from_fragment_as_str)
            // .filter(|r| r.is_ok()) // TODO!
            .collect()
    }

    /// Constructs `FragmentDirective` object, containing a list of `TextDirective`'s
    /// processed from the `[url:Url]`'s fragment string, and returns the object.
    ///
    /// If no fragment directive is found in the `[url:Url]`'s fragment, returns None
    #[must_use]
    pub(crate) fn from_fragment_as_str(fragment: &str) -> Option<FragmentDirective> {
        if let Ok(text_directives) = FragmentDirective::build_text_directives(fragment) {
            return Some(Self { text_directives });
        }

        None
    }

    /// Finds the Fragment Directive from the Url
    /// If the fragment directive is not found, return None
    #[must_use]
    pub(crate) fn from_url(url: &Url) -> Option<FragmentDirective> {
        let fragment = url.fragment()?;
        FragmentDirective::from_fragment_as_str(fragment)
    }

    /// Check the presence of given directive on the (web site response) input
    ///
    /// A fragment directive shall have multiple text directives included - the check will validate
    /// each of the text directives
    ///
    /// # Errors
    ///
    /// Return an error if
    /// - No match is found
    /// - Suffix error (spec instructs the fragment SHALL be upto **Suffix** and this error is returned if this condition is violated)
    pub(crate) fn check(&self, input: &str) -> Result<(), FragmentDirectiveError> {
        self.check_fragment_directive(input)
    }

    fn check_fragment_directive(&self, buf: &str) -> Result<(), FragmentDirectiveError> {
        let fd_checker = FragmentDirectiveTokenizer::new(self.text_directives.clone());

        let tok = Tokenizer::new(fd_checker, TokenizerOpts::default());

        let input = BufferQueue::default();
        input.pop_front();
        input.push_back(buf.into());

        let _res = tok.feed(&input);
        tok.end();

        if tok
            .sink
            .get_text_directives()
            .iter()
            .any(|t| t.status != TextDirectiveStatus::Completed)
        {
            Err(FragmentDirectiveError::NotFoundError)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::FragmentDirective;
    use crate::textfrag::{
        TextDirective,
        types::{TextDirectiveKind, UrlExt},
    };

    const MULTILINE_INPUT: &str = "Is there a way to deal with repeated instances of this split in a block of text? FOr instance:\
     \"This is just\na simple sentence. Here is some additional stuff. This is just\na simple sentence. And here is some more stuff.\
      This is just\na simple sentence. \". Currently it matches the entire string, rather than and therefore each instance. prefix

    start (immediately after the prefix and this) is start the statement and continue till the end. the suffix shall come into effect \
    as well.there is going to be a test for starting is mapped or not.

    actual end is this new line.

    type
    Hints at the linked URL's format with a MIME type. No built-in functionality.
    ";

    #[test]
    fn test_fragment_directive_start_only() {
        let fd = FragmentDirective::from_fragment_as_str(":~:text=repeated").unwrap();

        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    start: "repeated".into(),
                    search_kind: TextDirectiveKind::Start,
                    ..Default::default()
                }]
            }
        );
        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_start_end() {
        let fd = FragmentDirective::from_fragment_as_str(":~:text=repeated, block").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    start: "repeated".into(),
                    end: Some("block".into()),
                    search_kind: TextDirectiveKind::Start,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_prefix_start_end() {
        const INPUT: &str = r#"
                <html>
                    <body>
                        <p>This is a paragraph with some inline <code>https://example.com</code> and a normal
                            <a style="display:none;" href="https://example.org">example</a>
                        </p>
                    </body>
                </html>
                "#;

        let fd = FragmentDirective::from_fragment_as_str(":~:text=a-,paragraph,inline").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    prefix: Some("a".into()),
                    start: "paragraph".into(),
                    end: Some("inline".into()),
                    search_kind: TextDirectiveKind::Prefix,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        let fd = FragmentDirective::from_fragment_as_str(":~:text=with-,repeated").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    prefix: Some("with".into()),
                    start: "repeated".into(),
                    search_kind: TextDirectiveKind::Prefix,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        let fd =
            FragmentDirective::from_fragment_as_str(":~:text=linked%20URL,-'s format").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    start: "linked URL".into(),
                    suffix: Some("'s format".into()),
                    search_kind: TextDirectiveKind::Start,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        let fd =
            FragmentDirective::from_fragment_as_str(":~:text=with-,repeated,-instance").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    prefix: Some("with".into()),
                    start: "repeated".into(),
                    suffix: Some("instance".into()),
                    search_kind: TextDirectiveKind::Prefix,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        let fd =
            FragmentDirective::from_fragment_as_str(":~:text=with-,repeated, mapped, -or").unwrap();
        assert_eq!(
            fd,
            FragmentDirective {
                text_directives: vec![TextDirective {
                    prefix: Some("with".into()),
                    start: "repeated".into(),
                    suffix: Some("or".into()),
                    end: Some("mapped".into()),
                    search_kind: TextDirectiveKind::Prefix,
                    ..Default::default()
                }]
            }
        );

        assert!(fd.check(MULTILINE_INPUT).is_ok());
    }

    #[test]
    fn test_fragment_directive_as_url() {
        const TEST_FRAGMENT: &str = ":~:text=prefix-,start,end,-suffix&text=start,-suffix%2Dwith%2Ddashes&unknown_directive&text=prefix%2Donly-";
        let url = Url::parse(&("https://example.com/#test".to_owned() + TEST_FRAGMENT)).unwrap();
        let fd = url
            .fragment_directive()
            .expect("Expected to have directive");

        assert_eq!(
            fd.text_directives,
            vec![
                TextDirective {
                    prefix: Some("prefix".into()),
                    start: "start".into(),
                    end: Some("end".into()),
                    suffix: Some("suffix".into()),
                    search_kind: TextDirectiveKind::Prefix,
                    ..Default::default()
                },
                TextDirective {
                    start: "start".into(),
                    suffix: Some("suffix-with-dashes".into()),
                    ..Default::default()
                },
            ]
        );
    }
}
