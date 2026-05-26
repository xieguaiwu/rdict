use crate::parse::TranslationData;
use crate::Error;
use std::fmt::Debug;

pub trait DictionarySource: Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn fetch_url(&self, word: &str) -> String;

    fn parse(&self, word: &str, html: &str) -> Result<TranslationData, Error>;
}
