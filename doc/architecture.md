# rdict Architecture

## Overview

rdict is a **Rust workspace** project containing three crates:

```
rdict/                   (workspace root)
├── rdict-core/          Core library: HTTP fetch, HTML parse, caching
├── rdict-cli/           CLI application: clap, ratatui, rustyline
└── rdict-telegram/      Telegram bot: teloxide
```

Current version: **0.3.0**
Language: **Rust** (edition 2024)
Build system: **Cargo** workspace + Nix flake

---

## Crate Architecture

### rdict-core (library)

**Purpose**: Dictionary data retrieval via pluggable sources. Fetches HTML from configured sources, parses it, caches results in SQLite.

**Key types** (`parse.rs`):

| Type | Description |
|------|-------------|
| `TranslationData` | Enum: `ToChinese`, `ToEnglish`, or `German` (serde-tagged) |
| `ToChinese` | English→Chinese: pronunciation, meanings (with POS), examples |
| `ToEnglish` | Chinese→English: meanings (flat strings), examples |
| `GermanEntry` | German: word, gender, article, phonetic, CEFR level, word type, definitions, examples |
| `Meaning` | part_of_speech + definitions |
| `Example` | en (source) + zh (translation) |
| `Pronunciation` | uk + us phonetic strings |

**Source abstraction** (`source.rs`):

```rust
pub trait DictionarySource: Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn fetch_url(&self, word: &str) -> String;
    fn parse(&self, word: &str, html: &str) -> Result<TranslationData, Error>;
}
```

**Available sources**:

| Source | `--source` value | Language | URL |
|--------|-----------------|----------|-----|
| Youdao (default) | `youdao` | EN ↔ ZH | m.youdao.com |
| Netzverb / woerter.net | `woerter-net` | DE → DE | verbformen.com |

**Source modules**:

| Module | Contents |
|--------|----------|
| `source.rs` | `DictionarySource` trait definition |
| `youdao.rs` | `YoudaoSource` — wraps m.youdao.com HTML parsing |
| `german.rs` | `GermanEntry` struct + `WoerterNetSource` for verbformen.com |

**Key struct** (`rdict.rs`):

- `Rdict` — main client holding:
  - `client: reqwest::Client` — HTTP client
  - `source: Box<dyn DictionarySource>` — pluggable source
  - `pool: Option<SqlitePool>` — optional SQLite cache

**Key methods**:
- `Rdict::new(source, cache_db_path)` — constructs the client with a source
- `Rdict::get_results(input_text)` — checks cache → fetches HTML → parses → caches → returns
- `Rdict::fetch_source_html(text)` — HTTP GET to `{source.fetch_url(text)}`

**Parsing** (`parse.rs`):
- Youdao: `to_chinese()` and `to_english()` — scraper CSS class selectors for youdao DOM
- German: `WoerterNetSource::parse()` — uses schema.org microdata selectors + `<i>` elements

**Error handling** (`lib.rs`):
- `Error` enum with variants: `InvalidDatabasePath`, `NoTranslationResults`, `Parse`, `Http`, `Database`, `Serialize`, `Io`

**Cache**:
- SQLite via `sqlx`
- Single `cache_results` table: `(text TEXT PRIMARY KEY, data TEXT NOT NULL)`
- Value = serde-tagged JSON of `TranslationData` (dispatches on `"type"` tag)
- Cache is optional (skipped if `cache_db_path` is `None`)

### rdict-cli (binary)

**Entry point**: `src/main.rs` → `App::new()` → `App::run()`

**Modes**:
1. **Direct query**: CLI args provided → fetch & output
2. **Pipe mode**: stdin piped → read & output
3. **Interactive mode**: no args → rustyline REPL loop
4. **Shell completions**: `--completion <SHELL>` flag

**Output formats**: Markdown (colored or plain), JSON

**Features**:
- Spinner (indicatif) during fetch
- Pager (ratatui TUI) for long output
- Render functions in `rdict::rdict`: all take `colored: bool` param

**Args** (`args.rs`):
- `input_text: Vec<String>` — positional words
- `--source <SOURCE>` — dictionary source (youdao/woerter-net), via `clap::ValueEnum`
- `--no-cache` — disable SQLite cache
- `--json` — JSON output
- `--completion <SHELL>` — generate shell completions

### rdict-telegram (binary)

**Entry point**: `src/main.rs`

**Source selection**: reads `RDICT_SOURCE` env var

**Commands**:
- `/help` — show help
- `/translate <text>` — translate and reply

**Features**:
- Uses `teloxide` framework
- No caching (passes `None` as db_path)
- Outputs plain markdown wrapped in HTML `<pre>` tags

---

## Data Flow

```
User input (text)
    │
    ▼
Rdict::get_results(text)
    │
    ├── Check SQLite cache
    │   ├── hit  → return cached FetchedResult
    │   └── miss → proceed
    │
    ├── fetch_source_html(text)
    │   └── HTTP GET {source.fetch_url(text)}
    │
    ├── source.parse(text, html) 
    │   ├── YoudaoSource: CJK detection → to_english() or to_chinese()
    │   └── WoerterNetSource: German dictionary parsing
    │
    ├── Store in cache (if enabled)
    │
    └── Return FetchedResult { data, is_cached }
```

---

## Adding a New Dictionary Source

1. Define a new data struct (if fields differ from existing variants)
2. Add a variant to `TranslationData` enum
3. Implement `DictionarySource` trait — `name()`, `fetch_url()`, `parse()`
4. Add source variant to `clap::ValueEnum` in `rdict-cli/src/args.rs`
5. Handle the new source in `Rdict::new()` or config
6. Add a render function for the new data variant in `rdict.rs`

The existing infrastructure (reqwest, scraper, sqlx, serde) is fully reusable.

---

## Multi-Language Pipeline (Future Direction)

The `DictionarySource` trait is designed with future multi-language translation pipelines in mind:

- Each source encapsulates its own language pair
- Future: `TranslationPipeline` can chain multiple sources (A→B→C)
- Example: German → English (WoerterNetSource) → Chinese (YoudaoSource)
- See `doc/woerter-net-research.md` for details

---

## Dependencies

| Crate | Key Dependencies |
|-------|-----------------|
| rdict-core | reqwest, scraper, serde, serde_json, sqlx, thiserror, tokio, owo-colors |
| rdict-cli | rdict_core, clap, ratatui, rustyline, crossterm, indicatif, anyhow, console |
| rdict-telegram | rdict_core, teloxide, anyhow, html-escape |

---

## Code Quality Notes

- `#![forbid(unsafe_code)]` in all crates — good safety practice
- Uses `thiserror` for error types — good Rust pattern
- Remaining `unwrap()` calls acknowledged: ProgressStyle template, writeln! macro
- HTML selectors are source-specific and must be updated if upstream sites change their DOM
