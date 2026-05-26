use crate::source::DictionarySource;
use crate::youdao::YoudaoSource;
use crate::Error;
use crate::parse::{ToChinese, ToEnglish, TranslationData};
use log::{debug, info};
use reqwest::Client;
use sqlx::Row;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePool;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

/// Cache table storing all dictionary results as serde-tagged JSON
const CACHE_TABLE: &str = "cache_results";

pub struct Rdict {
    client: Client,
    source: Box<dyn DictionarySource>,
    pool: Option<SqlitePool>,
}

#[derive(Debug)]
pub enum Format {
    Markdown { colored: bool },
    Json,
}

pub struct FetchedResult {
    pub data: TranslationData,
    pub is_cached: bool,
}

impl Rdict {
    /// Create a new Rdict with a custom dictionary source
    pub async fn with_source(
        source: Box<dyn DictionarySource>,
        cache_db_path: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let pool = Self::init_cache(cache_db_path).await?;

        Ok(Self {
            client: Client::new(),
            source,
            pool,
        })
    }

    /// Create a new Rdict using the default Youdao source (backward compatible)
    pub async fn new(
        base_url: &str,
        cache_db_path: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let source = Box::new(YoudaoSource::new(base_url));
        Self::with_source(source, cache_db_path).await
    }

    async fn init_cache(cache_db_path: Option<PathBuf>) -> Result<Option<SqlitePool>, Error> {
        let pool: Option<SqlitePool> = if let Some(db_path) = cache_db_path {
            let should_init_db = !db_path.exists();

            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let db_url = db_path
                .to_str()
                .ok_or_else(|| Error::InvalidDatabasePath(db_path.clone()))?;

            if !sqlx::Sqlite::database_exists(db_url).await? {
                sqlx::Sqlite::create_database(db_url).await?;
            }

            let pool = SqlitePool::connect(db_url).await?;

            if should_init_db {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS cache_results (
                        text TEXT PRIMARY KEY,
                        data TEXT NOT NULL
                    );",
                )
                .execute(&pool)
                .await?;
            }

            Some(pool)
        } else {
            None
        };

        Ok(pool)
    }

    pub async fn get_results(&self, input_text: &str) -> Result<FetchedResult, Error> {
        let source_name = self.source.name();
        debug!("Getting result. input_text: {input_text}, source: {source_name}");

        // Try cache first if this is a youdao query
        if let Some(pool) = &self.pool {
            let cache_info = self.try_cache(pool, input_text).await?;
            if let Some(result) = cache_info {
                return Ok(result);
            }
        }

        // Fetch from web using source
        let html = self.fetch_source_html(input_text).await?;
        let result = self.source.parse(input_text, &html)?;

        // Store in cache
        self.store_cache(input_text, &result).await?;

        Ok(FetchedResult {
            data: result,
            is_cached: false,
        })
    }

    async fn try_cache(
        &self,
        pool: &SqlitePool,
        input_text: &str,
    ) -> Result<Option<FetchedResult>, Error> {
        let query = format!("SELECT data FROM {CACHE_TABLE} WHERE text = ?");
        match sqlx::query(&query).bind(input_text).fetch_optional(pool).await {
            Ok(Some(result)) => {
                let data_str: String = match result.try_get("data") {
                    Ok(s) => s,
                    Err(_) => {
                        info!("Cache column error for {input_text}, re-fetching");
                        return Ok(None);
                    }
                };
                match serde_json::from_str::<TranslationData>(&data_str) {
                    Ok(data) => Ok(Some(FetchedResult {
                        data,
                        is_cached: true,
                    })),
                    Err(_) => {
                        info!("Stale cache entry for {input_text}, deleting and re-fetching");
                        let del = format!("DELETE FROM {CACHE_TABLE} WHERE text = ?");
                        let _ = sqlx::query(&del).bind(input_text).execute(pool).await;
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                info!("Translation cache missed, fetching translation data again.");
                Ok(None)
            }
            Err(e) => {
                info!("Cache read error: {e}");
                Ok(None)
            }
        }
    }

    async fn store_cache(&self, input_text: &str, data: &TranslationData) -> Result<(), Error> {
        if let Some(pool) = &self.pool {
            let json_data = serde_json::to_string(data)?;
            let query = format!(
                "INSERT OR REPLACE INTO {CACHE_TABLE} (text, data) VALUES (?, ?)"
            );
            sqlx::query(&query)
                .bind(input_text)
                .bind(&json_data)
                .execute(pool)
                .await?;
        }

        Ok(())
    }

    async fn fetch_source_html(&self, text: &str) -> Result<String, Error> {
        let url = self.source.fetch_url(text);
        debug!("Fetching URL: {url}");

        let response = self
            .client
            .get(&url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36",
            )
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let source = self.source.name();
            return Err(Error::Parse(format!(
                "HTTP {status} from {source} while fetching \"{text}\""
            )));
        }

        let html = response.text().await?;
        Ok(html)
    }
}

macro_rules! c {
    ($output:expr, $colored:expr, $fmt:literal $(, $arg:expr)*) => {
        if $colored {
            writeln!($output, $fmt $(, $arg)*).unwrap()
        } else {
            let _ = writeln!($output, $fmt $(, $arg)*);
        }
    };
}

#[must_use]
pub fn render_chinese(result: &ToChinese, colored: bool) -> String {
    use owo_colors::OwoColorize;
    let mut output = String::new();

    if result.pronunciation.uk.is_some() || result.pronunciation.us.is_some() {
        c!(output, colored, "# Pronunciation");
        if let Some(ref uk) = result.pronunciation.uk {
            if colored {
                let _ = writeln!(output, "英：[{}]", uk.green());
            } else {
                let _ = writeln!(output, "英：[{uk}]");
            }
        }
        if let Some(ref us) = result.pronunciation.us {
            if colored {
                let _ = writeln!(output, "美：[{}]", us.green());
            } else {
                let _ = writeln!(output, "美：[{us}]");
            }
        }
        let _ = writeln!(output);
    }

    if !result.meanings.is_empty() {
        c!(output, colored, "# Meanings");
        for me in &result.meanings {
            if let Some(ref pa) = me.part_of_speech {
                if colored {
                    let _ = writeln!(output, "[{pa}]");
                } else {
                    let _ = writeln!(output, "[{pa}]");
                }
            }
            for de in &me.definitions {
                if colored {
                    let _ = writeln!(output, "* {}", de.green());
                } else {
                    let _ = writeln!(output, "* {de}");
                }
            }
            let _ = writeln!(output);
        }
    }

    if !result.examples.is_empty() {
        c!(output, colored, "# Examples");
        for ex in &result.examples {
            if colored {
                let _ = writeln!(output, "* {}", ex.en.green());
                let _ = writeln!(output, "  {}", ex.zh.magenta());
            } else {
                let _ = writeln!(output, "* {}", ex.en);
                let _ = writeln!(output, "  {}", ex.zh);
            }
        }
        let _ = writeln!(output);
    }

    output.trim_end().to_string()
}

#[must_use]
pub fn render_english(result: &ToEnglish, colored: bool) -> String {
    use owo_colors::OwoColorize;
    let mut output = String::new();

    if !result.meanings.is_empty() {
        c!(output, colored, "# Meanings");
        for me in &result.meanings {
            if colored {
                let _ = writeln!(output, "* {}", me.green());
            } else {
                let _ = writeln!(output, "* {me}");
            }
        }
        let _ = writeln!(output);
    }

    if !result.examples.is_empty() {
        c!(output, colored, "# Examples");
        for ex in &result.examples {
            if colored {
                let _ = writeln!(output, "* {}", ex.en.green());
                let _ = writeln!(output, "  {}", ex.zh.magenta());
            } else {
                let _ = writeln!(output, "* {}", ex.en);
                let _ = writeln!(output, "  {}", ex.zh);
            }
        }
        let _ = writeln!(output);
    }

    output.trim_end().to_string()
}

#[must_use]
pub fn render_german_entry(entry: &crate::german::GermanEntry, colored: bool) -> String {
    use owo_colors::OwoColorize;
    let mut output = String::new();

    if colored {
        let _ = writeln!(output, "{}", "# German Dictionary".bright_black());
    } else {
        let _ = writeln!(output, "# German Dictionary");
    }
    let _ = writeln!(output);

    let article_str = entry.article.as_deref().unwrap_or("");
    if colored {
        if !article_str.is_empty() {
            let _ = write!(output, "{} {}", article_str.cyan(), entry.word.green());
        } else {
            let _ = write!(output, "{}", entry.word.green());
        }
        if let Some(ref level) = entry.cefr_level {
            let _ = write!(output, "  [{}]", level.yellow());
        }
    } else {
        if !article_str.is_empty() {
            let _ = write!(output, "{article_str} {}", entry.word);
        } else {
            let _ = write!(output, "{}", entry.word);
        }
        if let Some(ref level) = entry.cefr_level {
            let _ = write!(output, "  [{level}]");
        }
    }
    let _ = writeln!(output);

    if let Some(ref wt) = entry.word_type {
        if let Some(ref g) = entry.gender {
            if colored {
                let _ = writeln!(output, "{} {} | {}", "# Type".bright_black(), wt.cyan(), g.yellow());
            } else {
                let _ = writeln!(output, "* {wt} ({g})");
            }
        } else if colored {
            let _ = writeln!(output, "{} {}", "# Type".bright_black(), wt.cyan());
        } else {
            let _ = writeln!(output, "* {wt}");
        }
    }

    if let Some(ref ph) = entry.phonetic {
        if colored {
            let _ = writeln!(output, "{}  /{}/", "# IPA".bright_black(), ph.green());
        } else {
            let _ = writeln!(output, "* IPA /{ph}/");
        }
    }

    if !entry.definitions.is_empty() {
        c!(output, colored, "# Definitions");
        for def in &entry.definitions {
            if colored {
                let _ = writeln!(output, "* {}", def.green());
            } else {
                let _ = writeln!(output, "* {def}");
            }
        }
    }

    if !entry.examples.is_empty() {
        c!(output, colored, "# Examples");
        for (de, en) in &entry.examples {
            if colored {
                let _ = writeln!(output, "* {}", de.green());
                let _ = writeln!(output, "  {}", en.magenta());
            } else {
                let _ = writeln!(output, "* {de}");
                let _ = writeln!(output, "  {en}");
            }
        }
    }

    output.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use crate::is_cjk;

    macro_rules! assert_cjk {
        ($input:expr, $expected:expr) => {
            if $expected {
                assert!(is_cjk($input), "Expected CJK: {:?}", $input);
            } else {
                assert!(!is_cjk($input), "Expected non-CJK: {:?}", $input);
            }
        };
    }

    #[test]
    fn test_cjk_with_cjk_only() {
        assert_cjk!("你好", true);
    }

    #[test]
    fn test_cjk_ascii_only() {
        assert_cjk!("hello", false);
    }

    #[test]
    fn test_cjk_mixed_input() {
        assert_cjk!("hello你好", true);
    }
}
