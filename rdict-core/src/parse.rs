use crate::Error;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Pronunciation {
    pub uk: Option<String>,
    pub us: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Example {
    pub en: String,
    pub zh: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Meaning {
    pub part_of_speech: Option<String>,
    pub definitions: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ToChinese {
    pub input_text: String,
    pub pronunciation: Pronunciation,
    pub meanings: Vec<Meaning>,
    pub examples: Vec<Example>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ToEnglish {
    pub input_text: String,
    pub meanings: Vec<String>,
    pub examples: Vec<Example>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum TranslationData {
    #[serde(rename = "to_chinese")]
    ToChinese(ToChinese),

    #[serde(rename = "to_english")]
    ToEnglish(ToEnglish),
}

macro_rules! selector {
    ($css:expr) => {
        // Selectors are parsed at runtime...
        LazyLock::new(|| Selector::parse($css).expect(concat!("Invalid selector: ", $css)))
    };
}

#[rustfmt::skip]
mod selectors {
    use std::sync::LazyLock;

    use super::Selector;

    pub static BODY_SELECTOR:                   LazyLock<Selector> = selector!(".search_result-dict");
    pub static WORD_SELECTOR:                   LazyLock<Selector> = selector!(".word-head .title");
    pub static PRONUNCIATION_SELECTOR:          LazyLock<Selector> = selector!(".phone_con .per-phone .phonetic");
    pub static MEANINGS_SELECTOR:               LazyLock<Selector> = selector!(".trans-container .basic .word-exp");
    pub static DEFINITIONS_SELECTOR:            LazyLock<Selector> = selector!(".trans");
    pub static PART_OF_SPEECH_SELECTOR:         LazyLock<Selector> = selector!(".pos");
    pub static EXAMPLE_SELECTOR:                LazyLock<Selector> = selector!(".trans-container .mcols-layout .col2");
    pub static EN_SELECTOR:                     LazyLock<Selector> = selector!(".sen-eng");
    pub static ZH_SELECTOR:                     LazyLock<Selector> = selector!(".sen-ch");
    pub static TO_ENGLISH_TRANSLATION_SELECTOR: LazyLock<Selector> = selector!(".trans-container .basic .col2 .point");
}

/// Parses English, returns Chinese
pub fn to_chinese(input_text: &str, html: &str) -> std::result::Result<ToChinese, Error> {
    let binding = Html::parse_document(html);
    let document = binding
        .select(&selectors::BODY_SELECTOR)
        .next()
        .ok_or(Error::Parse("no .search_result-dict found".into()))?;

    let mut result = ToChinese {
        input_text: input_text.to_owned(),
        ..Default::default()
    };

    for (i, element) in document
        .select(&selectors::PRONUNCIATION_SELECTOR)
        .take(2)
        .enumerate()
    {
        let text = element
            .text()
            .collect::<String>()
            .trim_matches('/')
            .trim()
            .to_owned();

        let text = if text.is_empty() { None } else { Some(text) };

        match i {
            0 => result.pronunciation.uk = text,
            1 => result.pronunciation.us = text,
            _ => unreachable!(),
        }
    }

    for element in document.select(&selectors::MEANINGS_SELECTOR) {
        let part_of_speech = element
            .select(&selectors::PART_OF_SPEECH_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_owned());

        let definitions: Vec<String> = element
            .select(&selectors::DEFINITIONS_SELECTOR)
            .next()
            .map(|e| {
                e.text()
                    .collect::<String>()
                    .trim()
                    .split('；')
                    .map(|s| s.trim().to_owned())
                    .collect()
            })
            .unwrap_or_default();

        if part_of_speech.is_some() || !definitions.is_empty() {
            result.meanings.push(Meaning {
                part_of_speech,
                definitions,
            });
        }
    }

    // Example sentences
    for element in document.select(&selectors::EXAMPLE_SELECTOR) {
        let en = element
            .select(&selectors::EN_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        let zh = element
            .select(&selectors::ZH_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        if !en.is_empty() || !zh.is_empty() {
            result.examples.push(Example { en, zh });
        }
    }

    if result.examples.is_empty()
        && result.meanings.is_empty()
        && result.pronunciation.uk.is_none()
        && result.pronunciation.us.is_none()
    {
        return Err(Error::NoTranslationResults);
    }

    Ok(result)
}

/// Parses Chinese, returns English
pub fn to_english(input_text: &str, html: &str) -> std::result::Result<ToEnglish, Error> {
    let binding = Html::parse_document(html);
    let document = binding
        .select(&selectors::BODY_SELECTOR)
        .next()
        .ok_or(Error::Parse("no .search_result-dict found".into()))?;

    let mut result = ToEnglish {
        input_text: input_text.to_owned(),
        ..Default::default()
    };

    // Meanings
    for element in document.select(&selectors::TO_ENGLISH_TRANSLATION_SELECTOR) {
        let text = element.text().collect::<String>().trim().to_owned();
        if !text.is_empty() {
            result.meanings.push(text);
        }
    }

    // Example sentences
    for element in document.select(&selectors::EXAMPLE_SELECTOR) {
        let en = element
            .select(&selectors::EN_SELECTOR)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        let zh = element
            .select(&selectors::ZH_SELECTOR)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        if !en.is_empty() || !zh.is_empty() {
            result.examples.push(Example { en, zh });
        }
    }

    if result.examples.is_empty() && result.meanings.is_empty() {
        return Err(Error::NoTranslationResults);
    }

    Ok(result)
}
