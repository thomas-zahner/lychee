use url::Url;

use crate::textfrag::types::FragmentDirective;

/// Fragment Directive extension trait
/// We will use the extension trait pattern to extend [`url::Url`] to support the text fragment feature
pub(crate) trait UrlExt {
    /// Constructs `FragmentDirective`, if the URL contains a fragment and has fragment directive delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective>;
}

impl UrlExt for Url {
    /// Return this URL's fragment directive, if any
    ///
    /// **Note:** A fragment directive is part of the URL's fragment following the `:~:` delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective> {
        FragmentDirective::from_url(self)
    }
}

#[cfg(test)]
mod test_fs_tree {
    use crate::textfrag::TextDirectiveKind;

    use super::*;
    use url::Url;

    #[test]
    fn test_fragment_directive_through_url() {
        let url = Url::parse(
            "https://example.com#:~:text=prefix-,start,end,-suffix&text=unknown_directive",
        )
        .unwrap();

        let fd = url
            .fragment_directive()
            .expect("Expected fragment directive to be present");

        assert!(
            fd.text_directives.len() == 2
                && fd.text_directives[0].prefix == "prefix"
                && fd.text_directives[0].search_kind == TextDirectiveKind::Prefix
        );
    }

    #[test]
    fn test_fragment_directive_error() {
        // without fragment directive delimiter
        let url =
            Url::parse("https://example.com#text=prefix-,start,end,-suffix&text=unknown_directive")
                .unwrap();
        assert!(url.fragment_directive().is_none());

        // malformed fragment directive delimiter
        let url = Url::parse(
            "https://example.com#:~text=prefix-,start,end,-suffix&text=unknown_directive",
        )
        .unwrap();
        assert!(url.fragment_directive().is_none());
    }
}
