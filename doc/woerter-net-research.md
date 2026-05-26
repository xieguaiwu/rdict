# woerter.net Integration Research

## Website Overview

**woerter.net** is the **Netzverb Dictionary** — a comprehensive German language dictionary covering:
- Verbs, nouns, adjectives, adverbs, articles, pronouns, prepositions, conjunctions, particles
- Meanings, synonyms, translations, usage examples, inflection tables
- CEFR levels (A1, A2, B1, B2, C1, C2)
- Grammatical features (gender, regularity, separability, etc.)

URL: https://www.woerter.net/
Maintained by: Netzverb (also operates verbformen.com, verben.de)

---

## API Availability

**Finding: No public API exists for woerter.net.**

The site is a static HTML dictionary — there is no documented REST/JSON API endpoint. The site generates HTML pages for each word.

Alternative German dictionary APIs that DO exist:

| Service | API Type | Cost | Notes |
|---------|----------|------|-------|
| **DWDS** (dwds.de) | REST/JSON | Free | `https://www.dwds.de/api/wb/snippet?q=WORT` — returns lemma info, word type, URL |
| **PONS** | REST/JSON | Paid (1000 req/mo free) | 42 language pairs, needs registration |
| **Wortschatz Leipzig** | REST | Free | University project, word frequencies, collocations, synonyms |

---

## Implementation: verbformen.com

**woerter.net unreachable**. **verbformen.com** (same Netzverb data) is used.

URL patterns:
- Nouns: `https://www.verbformen.com/declension/nouns/{word}.htm`
- Verbs: `https://www.verbformen.com/conjugation/{word}.htm`

### Parser Approach

Text-pattern-based, scoped to context:

| Field | Pattern | Location |
|-------|---------|----------|
| CEFR | `at A1 level` | FAQ section |
| Gender | `article is "das"` | FAQ section |
| Word type | `declension of noun` | Page header |
| IPA | `/.../` patterns | First 3000 chars |
| Definitions | Sentences near word | Body text |
| Examples | Pairs after `Examples` h2 | Examples section |

### Known Limitations

- **Nouns only**: Fetches noun declension URL. Verbs/adjectives not handled.
- **Text-based extraction**: Uses DOM text rather than direct CSS selectors.
- **No synonyms/translations extraction**: Present on page but not parsed.
- Returns `Error::NoTranslationResults` on empty parse to prevent silent garbage.

## Integration Approach

### Feasibility: HIGH

The project already has **all the infrastructure needed** for HTML-scraping-based integration:

| Component | Existing | For woerter.net |
|-----------|----------|-----------------|
| HTTP client | `reqwest::Client` in `Rdict` | Reuse existing client |
| HTML parser | `scraper::Html` + `Selector` | New selectors needed |
| Caching | `sqlx::SqlitePool` | New table(s) needed |
| CLAP CLI | `args.rs` | Add `--source` flag |
| Output rendering | `render_*` functions | New render functions |
| Error handling | `thiserror::Error` | New error variants |

### Required Architecture Changes

Since the current code is tightly coupled to youdao's specific HTML and data model, a significant refactoring is needed:

#### 1. Abstract Dictionary Source (highest priority)
Currently `Rdict` has hardcoded youdao URL and parsing. A trait-based approach would enable multiple backends:

```rust
trait DictionarySource {
    fn lookup(&self, word: &str) -> Result<DictionaryEntry, Error>;
}
```

#### 2. New Data Model
German dictionary data is fundamentally different from bilingual translation:

| Feature | Current (youdao) | German (woerter.net) |
|---------|-----------------|---------------------|
| Primary data | Translations | Definitions in German |
| Pronunciation | UK/US phonetics | IPA |
| Grammar | Part of speech only | Gender, case, conjugation, declension |
| Levels | None | CEFR (A1-C2) |
| Synonyms | No | Yes |
| Inflection | No | Full conjugation/declension tables |

#### 3. HTML Structure Differences
woerter.net uses different CSS classes and page structure than youdao. A dedicated parser module would be needed.

#### 4. Proposed Implementation Plan

**Phase 1 — Architecture Refactoring:**
1. Define `DictionarySource` trait in `rdict-core`
2. Wrap current youdao logic as `YoudaoSource` implementing the trait
3. Update `Rdict` to use the trait instead of hardcoded youdao calls
4. Add `--source` CLI argument to select dictionary backend

**Phase 2 — woerter.net Integration:**
5. Define German dictionary data structures
6. Implement `WoerterNetSource` with HTML fetching + parsing
7. Add render functions for German output
8. Add cache support for German results

**Phase 3 — Polish:**
9. Interactive mode source switching
10. Tests with fixtures
11. Documentation

---

## DWDS (Alternative — Easier Integration)

If a German dictionary with a proper API is preferred, **DWDS** offers:
- Simple JSON API: `https://www.dwds.de/api/wb/snippet?q=WORT`
- Returns: `{input, wortart, lemma, url}`
- Also has word frequency, IPA pronunciation, random word APIs
- Fully free, no registration needed

This would be significantly easier to integrate than HTML scraping woerter.net.
