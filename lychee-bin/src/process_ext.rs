use anyhow::Result;
use clap::{Arg, ArgAction, Args, Command, error::ErrorKind};
use serde::Deserializer;

// there isn't a derive api for getting grouped values yet,
// so we have to use hand-rolled parsing for exec and exec-batch
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProcessExt {
    command: Option<CommandSet>,
}

// Custom deserializer used for e.g. config file parsing
pub(crate) fn deserialize_process_ext<'de, D>(deserializer: D) -> Result<ProcessExt, D::Error>
where
    D: Deserializer<'de>,
{
    todo!()
}

impl clap::FromArgMatches for ProcessExt {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let command = matches
            .get_occurrences::<String>("process-ext")
            .map(CommandSet::new)
            .transpose()
            .map_err(|e| clap::Error::raw(ErrorKind::InvalidValue, e))?;
        Ok(ProcessExt { command })
    }

    fn update_from_arg_matches(
        &mut self,
        matches: &clap::ArgMatches,
    ) -> std::result::Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Args for ProcessExt {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(
            Arg::new("process-ext")
                .action(ArgAction::Append)
                .long("process-ext")
                .short('p')
                .num_args(1..)
                .allow_hyphen_values(true)
                .value_terminator(";")
                .value_name("cmd")
                // .help("Execute a command for each search result")
                .long_help(
                    "Preprocess files based on their extension.
This option allows to preprocess and convert input files
into a plain text file format that lychee can work with.",
                ),
        )
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct CommandSet {
    commands: Vec<CommandTemplate>,
}

impl CommandSet {
    fn new<I, T, S>(input: I) -> Result<CommandSet>
    where
        I: IntoIterator<Item = T>,
        T: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Ok(CommandSet {
            commands: input
                .into_iter()
                .map(CommandTemplate::new)
                .collect::<Result<_>>()?,
        })
    }
}

/// Represents a template that is utilized to generate command strings.
///
/// The template is meant to be coupled with an input in order to generate a command. The
/// `generate_and_execute()` method will be used to generate a command and execute it.
#[derive(Debug, Clone, PartialEq)]
struct CommandTemplate {
    // args: Vec<FormatTemplate>,
}

impl CommandTemplate {
    fn new<I, S>(input: I) -> Result<CommandTemplate>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        // let mut args = Vec::new();
        let mut has_placeholder = false;

        for arg in input {
            let arg = arg.as_ref();
            dbg!(arg);
        }

        // todo
        Ok(CommandTemplate {})
    }
}
