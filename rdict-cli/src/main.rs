#![forbid(unsafe_code)]

mod args;
mod pager;

use crate::args::Args;
use anyhow::{Context, Result, ensure};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use directories_next::ProjectDirs;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use owo_colors::OwoColorize;
use rdict_core::parse::TranslationData;
use rdict_core::rdict::{self, Format, Rdict};
use rdict_core::german::WoerterNetSource;
use rustyline::DefaultEditor;
use std::env;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

struct App {
    format: Format,
    client: Rdict,
    query: String,
}

impl App {
    async fn new(cli: Args) -> Result<Self> {
        let format = if cli.json {
            Format::Json
        } else {
            Format::Markdown {
                colored: console::colors_enabled(),
            }
        };

        let query = cli.input_text.join(" ");
        let db_path: Option<PathBuf> = if cli.no_cache {
            None
        } else {
            let proj_dirs = ProjectDirs::from("dev", "ny4", "rdict")
                .context("Could not determine project directory")?;
            Some(proj_dirs.cache_dir().join("cache.db"))
        };

        use crate::args::DictionarySource;

        let client = match cli.source {
            DictionarySource::Youdao => {
                Rdict::new("https://m.youdao.com", db_path).await?
            }
            DictionarySource::WoerterNet => {
                let source = Box::new(WoerterNetSource::new("https://www.verbformen.com"));
                Rdict::with_source(source, db_path).await?
            }
        };

        Ok(Self {
            format,
            client,
            query,
        })
    }

    async fn run(&self) -> Result<()> {
        let stdin_is_piped = !io::stdin().is_terminal();

        if !self.query.is_empty() {
            info!("`input_text` provided through argument.");
            self.output_results(&self.query).await?;
        } else if stdin_is_piped {
            info!("`input_text` provided through pipe.");
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            let input_text = buffer.trim();
            ensure!(!input_text.is_empty(), "No input_text specified");
            self.output_results(input_text).await?;
        } else {
            info!("`input_text` not provided, entering interactive mode.");
            self.interactive_mode()
                .await
                .context("Interactive mode failed")?;
        }

        Ok(())
    }

    async fn interactive_mode(&self) -> rustyline::Result<()> {
        let mut rl = DefaultEditor::new()?;
        loop {
            let readline = if cfg!(target_family = "windows") || !console::colors_enabled() {
                rl.readline("[rdict]# ")
            } else {
                rl.readline(format!("{}# ", "[rdict]".green()).as_str())
            };
            match readline {
                Ok(line) => {
                    if !line.is_empty() {
                        rl.add_history_entry(line.as_str())?;
                        let input_text = line.trim();
                        if let Err(err) = self.output_results(input_text).await {
                            println!("Error: {err:?}");
                        }
                    }
                }
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn output_results(&self, input_text: &str) -> Result<()> {
        let spinner = supports_ansi().then(|| {
            let spinner = ProgressBar::new_spinner();
            spinner.set_message("Fetching data...");
            spinner.enable_steady_tick(Duration::from_millis(100));
            #[expect(clippy::literal_string_with_formatting_args)]
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                    .template("{spinner} {msg}")
                    .unwrap(),
            );

            spinner
        });

        let result = self.client.get_results(input_text).await?;

        if let Some(spinner) = spinner {
            spinner.finish_and_clear();
        }

        match &self.format {
            Format::Markdown { colored } => {
                let is_colored = *colored;
                let output = match &result.data {
                    TranslationData::ToChinese(tc) => rdict::render_chinese(tc, is_colored),
                    TranslationData::ToEnglish(te) => rdict::render_english(te, is_colored),
                    TranslationData::German(ge) => rdict::render_german_entry(ge, is_colored),
                };

                let mut indented_output = output
                    .lines()
                    .map(|line| format!("  {line}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                if result.is_cached {
                    indented_output.push_str(&format!(
                        "\n\n  {}",
                        format!("[ {input_text} ] From cache").bright_black()
                    ));
                }

                let indented_output = format!("\n{indented_output}\n");

                let (_, height) =
                    crossterm::terminal::size().context("Failed to get terminal size")?;
                if height > 4 && height - 4 < indented_output.lines().count() as u16 {
                    let mut terminal = ratatui::init();
                    (pager::Pager {
                        text: indented_output,
                        ..Default::default()
                    })
                    .run(&mut terminal)?;
                    ratatui::restore();
                } else {
                    println!("{indented_output}");
                }
            }
            Format::Json => println!("{}", serde_json::to_string_pretty(&result.data)?),
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cli = Args::parse();

    // Generate shell completions
    if let Some(generator) = cli.completion {
        let mut cmd = Args::command();
        let name = cmd.get_name().to_string();
        generate(generator, &mut cmd, &name, &mut io::stdout());
        return Ok(());
    }

    let app = App::new(cli).await?;
    app.run().await
}

#[must_use]
pub fn supports_ansi() -> bool {
    if env::var("NO_COLOR").is_ok() {
        return false;
    }

    matches!(env::var("TERM"), Ok(term) if term != "dumb")
}
