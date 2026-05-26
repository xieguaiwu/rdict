use crate::source::DictionarySource;
use crate::Error;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GermanEntry {
    pub word: String,
    pub gender: Option<String>,
    pub article: Option<String>,
    pub phonetic: Option<String>,
    pub cefr_level: Option<String>,
    pub word_type: Option<String>,
    pub definitions: Vec<String>,
    pub examples: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct WoerterNetSource {
    base_url: String,
}

/// Guess German word type from suffix for URL selection
fn guess_word_type(word: &str) -> &'static str {
    let lower = word.to_lowercase();
    if lower.ends_with("ieren")
        || lower.ends_with("eln")
        || lower.ends_with("ern")
        || lower.ends_with("en")
    {
        "verb"
    } else if lower.ends_with("lich")
        || lower.ends_with("isch")
        || lower.ends_with("ig")
        || lower.ends_with("bar")
        || lower.ends_with("sam")
        || lower.ends_with("los")
        || lower.ends_with("haft")
        || lower.ends_with("arm")
        || lower.ends_with("frei")
        || lower.ends_with("reich")
    {
        "adjective"
    } else {
        "noun"
    }
}

impl WoerterNetSource {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_owned(),
        }
    }
}

impl DictionarySource for WoerterNetSource {
    fn name(&self) -> &'static str {
        "woerter-net"
    }

    fn fetch_url(&self, word: &str) -> String {
        let path = match guess_word_type(word) {
            "verb" => format!("conjugation/{}.htm", word),
            "adjective" => format!("declension/adjectives/{}.htm", word),
            _ => format!("declension/nouns/{}.htm", word),
        };
        format!("{}/{}", self.base_url, path)
    }

    fn parse(&self, word: &str, html: &str) -> Result<crate::parse::TranslationData, Error> {
        let document = Html::parse_document(html);
        let mut entry = GermanEntry {
            word: word.to_owned(),
            ..Default::default()
        };

        let title_sel = sel("title");
        let body_sel = sel("body");

        // 1. Word type from page <title>
        if let Some(el) = document.select(&title_sel).next() {
            let t = el.text().collect::<String>().to_lowercase();
            if t.contains("conjugation") {
                entry.word_type = Some("verb".to_string());
            } else if t.contains("declension of adjective") || t.contains("declension adjective") {
                entry.word_type = Some("adjective".to_string());
            } else if t.contains("declension of noun") || t.contains("declension noun") {
                entry.word_type = Some("noun".to_string());
            }
        }

        // 2. FAQ section — schema.org microdata for CEFR level and gender
        let faq_sel = sel("[itemprop=\"mainEntity\"]");
        for faq in document.select(&faq_sel) {
            let a_sel = sel("[itemprop=\"acceptedAnswer\"] p");
            let answer = faq
                .select(&a_sel)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();

            if answer.is_empty() {
                continue;
            }

            // CEFR level: "A1 level", "belongs to the vocabulary at A1 level", etc.
            let a_lower = answer.to_lowercase();
            for level in ["A1", "A2", "B1", "B2", "C1", "C2"] {
                if a_lower.contains(&level.to_lowercase()) {
                    entry.cefr_level = Some(level.to_string());
                    break;
                }
            }

            // Gender/article: "article is "das"", "Haus is neuter", etc.
            for (article, gender) in [("das", "neuter"), ("der", "masculine"), ("die", "feminine")] {
                if a_lower.contains(&format!("article is \"{article}\""))
                    || a_lower.contains(&format!("\"{word}\" is {gender}"))
                {
                    entry.article = Some(article.to_string());
                    entry.gender = Some(gender.to_string());
                    break;
                }
            }
        }

        // 3. Body text for IPA and validated text extraction
        let body_text: String = document
            .select(&body_sel)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        // IPA: scan first 3000 chars for /.../ patterns, require IPA-specific chars
        let head: String = body_text.chars().take(3000).collect();
        let mut in_ipa = false;
        let mut ipa_buf = String::new();
        for ch in head.chars() {
            if ch == '/' {
                if in_ipa {
                    // Closing slash — validate and extract
                    if ipa_buf.len() >= 2 && ipa_buf.len() <= 25
                        && ipa_buf.contains(|c: char| c == 'ˈ' || c == 'ˌ' || c == 'ː'
                            || c == 'ʃ' || c == 'ʒ' || c == 'ʔ'
                            || c == 'ç' || c == 'x' || c == 'ŋ'
                            || c == 'ʁ' || c == 'ø' || c == 'ɛ'
                            || c == 'œ' || c == 'ʏ' || c == 'ʊ'
                            || c == 'ɪ' || c == 'ə' || c == 'ɐ')
                    {
                        entry.phonetic = Some(ipa_buf.clone());
                        break;
                    }
                }
                // Toggle IPA mode
                in_ipa = !in_ipa;
                ipa_buf.clear();
            } else if in_ipa {
                ipa_buf.push(ch);
            }
        }

        // 4. Definitions from <i> elements (used exclusively for definitions on these pages)
        let i_sel = sel("i");
        for el in document.select(&i_sel) {
            let text = el.text().collect::<String>().trim().to_string();
            if text.len() > 10
                && text.len() < 300
                && !text.starts_with("http")
                && !text.starts_with('@')
            {
                let clean = text.trim().to_string();
                if !entry.definitions.contains(&clean) {
                    entry.definitions.push(clean);
                    if entry.definitions.len() >= 8 {
                        break;
                    }
                }
            }
        }

        // 5. Examples from ul.rLstGt li
        let ul_sel = sel("ul.rLstGt li");
        for li in document.select(&ul_sel).take(3) {
            let all_text: String = li.text().collect::<Vec<_>>().join(" ");
            let span_sel = sel("span");
            let english: String = li
                .select(&span_sel)
                .last()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let german = if !english.is_empty() {
                all_text
                    .trim_end_matches(&english)
                    .trim_end()
                    .to_string()
            } else {
                all_text.trim().to_string()
            };

            if german.len() > 5 && english.len() > 5 {
                entry.examples.push((german, english));
            }
        }

        // Validate — error on empty result
        if entry.word_type.is_none()
            && entry.cefr_level.is_none()
            && entry.phonetic.is_none()
            && entry.definitions.is_empty()
            && entry.examples.is_empty()
        {
            return Err(Error::Parse(format!(
                "No German dictionary data found for \"{word}\""
            )));
        }

        Ok(crate::parse::TranslationData::German(entry))
    }
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect(&format!("invalid CSS selector: {s}"))
}
