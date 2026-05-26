use clap::{Parser, ValueEnum};
use clap_complete::Shell;
use std::fmt;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DictionarySource {
    /// English ⇄ Chinese translation (default)
    Youdao,
    /// German dictionary (Netzverb / verbformen.com)
    WoerterNet,
}

#[derive(Parser)]
#[command(name = "rdict", version, about, long_about = None)]
pub struct Args {
    #[arg(value_name = "TEXT")]
    pub(crate) input_text: Vec<String>,

    /// Disable translation caches
    #[arg(long)]
    pub(crate) no_cache: bool,

    /// Output using JSON
    #[arg(long)]
    pub(crate) json: bool,

    /// Dictionary source to use
    #[arg(long, default_value_t = DictionarySource::Youdao)]
    pub(crate) source: DictionarySource,

    /// Generate shell completions
    #[arg(long)]
    pub(crate) completion: Option<Shell>,
}

impl fmt::Display for DictionarySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Youdao => write!(f, "youdao"),
            Self::WoerterNet => write!(f, "woerter-net"),
        }
    }
}
