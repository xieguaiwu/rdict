use crate::Error;
use crate::parse::{ToChinese, ToEnglish, TranslationData, to_chinese, to_english};
use log::{debug, info};
use owo_colors::OwoColorize;
use reqwest::Client;
use sqlx::Row;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::SqlitePool;
use std::fmt::Write;
use std::fs;

type CacheVariant = (&'static str, fn(String) -> Result<TranslationData, Error>);

#[derive(Debug, Clone)]

pub struct Rdict {
    client: Client,
    base_url: String,
    pool: Option<SqlitePool>,
}

#[derive(Debug)]
pub enum Format {
    /// Markdown with ANSI color escape sequences
    MarkdownColored,
    /// Plain Markdown
    Markdown,
    /// Formatted JSON
    Json,
}

#[derive(Debug, Clone)]
pub struct FetchedResult {
    pub data: TranslationData,
    pub is_cached: bool,
}

impl Rdict {
    pub async fn new(
        base_url: &str,
        cache_db_path: Option<std::path::PathBuf>,
    ) -> Result<Self, Error> {
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
                let init_statements = [
                    "CREATE TABLE to_english_results (
                        text TEXT PRIMARY KEY,
                        data TEXT NOT NULL
                    );",
                    "CREATE TABLE to_chinese_results (
                        text TEXT PRIMARY KEY,
                        data TEXT NOT NULL
                    );",
                ];

                for statement in init_statements {
                    sqlx::query(statement).execute(&pool).await?;
                }
            }

            Some(pool)
        } else {
            None
        };

        Ok(Self {
            client: Client::new(),
            base_url: base_url.to_owned(),
            pool,
        })
    }

    pub async fn get_results(&self, input_text: &str) -> Result<FetchedResult, Error> {
        let is_cjk = contains_cjk(input_text)?;
        debug!("Getting result. input_text: {input_text}, is_cjk: {is_cjk}");

        // Try cache first
        if let Some(pool) = &self.pool {
            let (table, variant): CacheVariant = if is_cjk {
                debug!("Caching enabled, trying fetch data from cache.");
                ("to_english_results", |v| {
                    Ok(TranslationData::ToEnglish(serde_json::from_str(&v)?))
                })
            } else {
                ("to_chinese_results", |v| {
                    Ok(TranslationData::ToChinese(serde_json::from_str(&v)?))
                })
            };

            let query = format!("SELECT data FROM {table} WHERE text = ?");
            let delete_row = async || {
                let query = format!("DELETE FROM {table} WHERE text = ?");

                sqlx::query(&query).bind(input_text).execute(pool).await
            };

            match sqlx::query(&query)
                .bind(input_text)
                .fetch_optional(pool)
                .await
            {
                Ok(Some(result)) => {
                    let data_str: String = result.try_get("data")?;
                    match variant(data_str) {
                        Ok(data) => {
                            return Ok(FetchedResult {
                                data,
                                is_cached: true,
                            });
                        }
                        Err(_) => {
                            delete_row().await?;
                        }
                    }
                }

                Ok(None) => {
                    info!("Translation cache missed, fetching translation data again.");
                }

                Err(_) => {
                    info!(
                        "Database error, deleting row from SQLite cache and fetching translation data again."
                    );
                    delete_row().await?;
                }
            }
        }

        // Fetch from web
        let html = self.fetch_text_html(input_text).await?;

        if is_cjk {
            let result = to_english(input_text, &html)?;
            if let Some(pool) = &self.pool {
                let data = serde_json::to_string(&result)?;

                sqlx::query("INSERT OR REPLACE INTO to_english_results (text, data) VALUES (?, ?)")
                    .bind(input_text)
                    .bind(&data)
                    .execute(pool)
                    .await?;
            }

            Ok(FetchedResult {
                data: TranslationData::ToEnglish(result),
                is_cached: false,
            })
        } else {
            let result = to_chinese(input_text, &html)?;
            if let Some(pool) = &self.pool {
                let data = serde_json::to_string(&result)?;

                sqlx::query("INSERT OR REPLACE INTO to_chinese_results (text, data) VALUES (?, ?)")
                    .bind(input_text)
                    .bind(&data)
                    .execute(pool)
                    .await?;
            }

            Ok(FetchedResult {
                data: TranslationData::ToChinese(result),
                is_cached: false,
            })
        }
    }

    async fn fetch_text_html(&self, text: &str) -> Result<String, reqwest::Error> {
        let url = format!("{}/result", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("word", text), ("lang", "en")])
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36",
            )
            .send()
            .await?
            .text()
            .await?;

        Ok(response)
    }
}

fn contains_cjk(text: &str) -> Result<bool, Error> {
    if text.is_empty() {
        return Err(Error::EmptyInput);
    }
    Ok(text
        .chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch)))
}

#[must_use]
pub fn render_chinese_colored(result: &ToChinese) -> String {
    let mut output = String::new();

    if result.pronunciation.uk.is_some() || result.pronunciation.us.is_some() {
        writeln!(output, "{}", "# Pronunciation".bright_black()).unwrap();

        if let Some(ref uk) = result.pronunciation.uk {
            writeln!(output, "英：[{}]", uk.green()).unwrap();
        }

        if let Some(ref us) = result.pronunciation.us {
            writeln!(output, "美：[{}]", us.green()).unwrap();
        }

        writeln!(output).unwrap();
    }

    if !result.meanings.is_empty() {
        writeln!(output, "{}", "# Meanings".bright_black()).unwrap();
        for me in &result.meanings {
            if let Some(ref pa) = me.part_of_speech {
                writeln!(output, "[{pa}]").unwrap();
            }
            for de in &me.definitions {
                writeln!(output, "* {}", de.green()).unwrap();
            }
            writeln!(output).unwrap();
        }
    }

    if !result.examples.is_empty() {
        writeln!(output, "{}", "# Examples".bright_black()).unwrap();
        for ex in &result.examples {
            writeln!(output, "* {}", ex.en.green()).unwrap();
            writeln!(output, "  {}", ex.zh.magenta()).unwrap();
        }
        writeln!(output).unwrap();
    }

    output.trim_end().to_string()
}

#[must_use]
pub fn render_english_colored(result: &ToEnglish) -> String {
    let mut output = String::new();

    if !result.meanings.is_empty() {
        writeln!(output, "{}", "# Meanings".bright_black()).unwrap();
        for me in &result.meanings {
            writeln!(output, "* {}", me.green()).unwrap();
        }
        writeln!(output).unwrap();
    }

    if !result.examples.is_empty() {
        writeln!(output, "{}", "# Examples".bright_black()).unwrap();
        for ex in &result.examples {
            writeln!(output, "* {}", ex.en.green()).unwrap();
            writeln!(output, "  {}", ex.zh.magenta()).unwrap();
        }
        writeln!(output).unwrap();
    }

    output.trim_end().to_string()
}

#[must_use]
pub fn render_chinese_plain(result: &ToChinese) -> String {
    let mut output = String::new();

    if result.pronunciation.uk.is_some() || result.pronunciation.us.is_some() {
        writeln!(output, "# Pronunciation").unwrap();

        if let Some(ref uk) = result.pronunciation.uk {
            writeln!(output, "英：[{uk}]").unwrap();
        }

        if let Some(ref us) = result.pronunciation.us {
            writeln!(output, "美：[{us}]").unwrap();
        }

        writeln!(output).unwrap();
    }

    if !result.meanings.is_empty() {
        writeln!(output, "# Meanings").unwrap();
        for me in &result.meanings {
            if let Some(ref pa) = me.part_of_speech {
                writeln!(output, "[{pa}]").unwrap();
            }
            for de in &me.definitions {
                writeln!(output, "* {de}").unwrap();
            }
            writeln!(output).unwrap();
        }
    }

    if !result.examples.is_empty() {
        writeln!(output, "# Examples").unwrap();
        for ex in &result.examples {
            writeln!(output, "* {}", ex.en).unwrap();
            writeln!(output, "  {}", ex.zh).unwrap();
        }
        writeln!(output).unwrap();
    }

    output.trim_end().to_string()
}

#[must_use]
pub fn render_english_plain(result: &ToEnglish) -> String {
    let mut output = String::new();

    if !result.meanings.is_empty() {
        writeln!(output, "# Meanings").unwrap();
        for me in &result.meanings {
            writeln!(output, "* {me}").unwrap();
        }
        writeln!(output).unwrap();
    }

    if !result.examples.is_empty() {
        writeln!(output, "# Examples").unwrap();
        for ex in &result.examples {
            writeln!(output, "* {}", ex.en).unwrap();
            writeln!(output, "  {}", ex.zh).unwrap();
        }
        writeln!(output).unwrap();
    }

    output.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    #[test]
    fn test_contains_cjk_with_cjk() {
        assert!(contains_cjk("你好").unwrap());
    }

    #[test]
    fn test_contains_cjk_without_cjk() {
        assert!(!contains_cjk("hello").unwrap());
    }

    #[test]
    fn test_contains_cjk_mixed_input() {
        assert!(contains_cjk("hello你好").unwrap());
    }

    #[test]
    fn test_contains_cjk_empty() {
        contains_cjk("").unwrap_err();
    }

    #[tokio::test]
    #[expect(clippy::significant_drop_tightening)]
    async fn test_fetch_text_html_success_with_mock_server() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("GET", "/result")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("word".into(), "hello".into()),
                Matcher::UrlEncoded("lang".into(), "en".into()),
            ]))
            .with_status(200)
            .with_body(include_str!("fixtures/hello_response.html"))
            .create();

        let client = Rdict::new(&server.url(), None).await.unwrap();

        let html = client.fetch_text_html("hello").await.unwrap();
        assert!(html.contains("Hello"));
        mock.assert();
    }
}
