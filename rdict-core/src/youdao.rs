use crate::parse::{to_chinese, to_english, TranslationData};
use crate::source::DictionarySource;
use crate::{is_cjk, Error};

#[derive(Debug)]
pub(crate) struct YoudaoSource {
    base_url: String,
}

impl YoudaoSource {
    pub(crate) fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_owned(),
        }
    }
}

impl DictionarySource for YoudaoSource {
    fn name(&self) -> &'static str {
        "youdao"
    }

    fn fetch_url(&self, word: &str) -> String {
        format!("{}/result?word={}&lang=en", self.base_url, word)
    }

    fn parse(&self, word: &str, html: &str) -> Result<TranslationData, Error> {
        if is_cjk(word) {
            to_english(word, html).map(TranslationData::ToEnglish)
        } else {
            to_chinese(word, html).map(TranslationData::ToChinese)
        }
    }
}
