//! A server's command line, as `lsp.servers` gives it: split into the program and its
//! arguments as a shell would, with double or single quotes keeping spaces and a backslash
//! before a quote keeping the quote. Other backslashes stay, as Windows paths need them.

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    pub program: String,
    pub args: Vec<String>,
}

/// Why a command line cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandLineError {
    #[error("{}", t!("lsp.command-line-empty"))]
    Empty,
    #[error("{}", t!("lsp.command-line-unclosed-quote", quote = .0))]
    UnclosedQuote(char),
}

impl CommandLine {
    /// Splits `line`.
    pub fn parse(line: &str) -> Result<Self, CommandLineError> {
        let mut words = Vec::new();
        let mut word = String::new();
        // Whether a word was started, so that `""` is an empty argument.
        let mut started = false;
        let mut quote: Option<char> = None;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match (quote, c) {
                (_, '\\') if chars.peek().is_some_and(|next| matches!(next, '"' | '\'')) => {
                    word.extend(chars.next());
                    started = true;
                }
                (Some(open), c) if c == open => quote = None,
                (Some(_), c) => word.push(c),
                (None, '"' | '\'') => {
                    quote = Some(c);
                    started = true;
                }
                (None, c) if c.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                (None, c) => {
                    word.push(c);
                    started = true;
                }
            }
        }
        if let Some(open) = quote {
            return Err(CommandLineError::UnclosedQuote(open));
        }
        if started {
            words.push(word);
        }
        let mut words = words.into_iter();
        let program = words
            .next()
            .filter(|program| !program.is_empty())
            .ok_or(CommandLineError::Empty)?;
        Ok(Self {
            program,
            args: words.collect(),
        })
    }
}
