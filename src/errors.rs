use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum Errors {
    MissingCredentialsFile,
    FailedQuery,
    MissingLanguage,
    InvalidShell,
    InvalidLanguage,
    NoTranslations,
    CantParseResource,
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for Errors {}
