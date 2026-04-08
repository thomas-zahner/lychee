#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
/// Text directive kind enum - used by the tokenizer to identify the directive to search for
/// in the block content
pub enum TextDirectiveKind {
    /// Prefix
    Prefix,
    /// Start
    #[default]
    Start,
    /// End
    End,
    /// Suffix
    Suffix,
}
