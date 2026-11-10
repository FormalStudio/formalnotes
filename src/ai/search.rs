//! A lexical ranker over the library, for the model's `search_notes`: titles weigh most, exact
//! phrases beat scattered words, rare words beat common ones, and a note that holds every word
//! of the query beats one that holds a few of them many times.

use std::collections::{HashMap, HashSet};

use crate::library::{Library, Note, NoteId};

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub id: NoteId,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "did", "do", "does", "for", "from", "how", "i", "in", "is", "it",
    "me", "my", "of", "on", "or", "so", "that", "the", "to", "was", "were", "what", "when", "where", "which", "who",
    "why", "with", "you",
];

const SNIPPET_CHARS: usize = 180;

/// The words of `text`, lowercased, split on anything that is not a letter or a digit.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase)
}

/// A word with a plural or possessive `s` taken off, so "recipes" finds "recipe".
fn stem(word: &str) -> &str {
    match word.strip_suffix('s') {
        Some(rest) if rest.chars().count() >= 3 && !rest.ends_with('s') => rest,
        _ => word,
    }
}

fn terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let all: Vec<String> = words(query).collect();
    let kept: Vec<String> = all.iter().filter(|w| !STOP_WORDS.contains(&w.as_str())).cloned().collect();
    let chosen = if kept.is_empty() { all } else { kept };
    chosen.into_iter().map(|w| stem(&w).to_owned()).filter(|w| seen.insert(w.clone())).collect()
}

struct Doc<'a> {
    note: &'a Note,
    title: HashMap<String, usize>,
    body: HashMap<String, usize>,
    folder: HashSet<String>,
}

fn counts(text: &str) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for word in words(text) {
        *out.entry(stem(&word).to_owned()).or_default() += 1;
    }
    out
}

/// The best `limit` notes for `query`, best first. Notes that match nothing are left out.
pub fn search(library: &Library, query: &str, limit: usize) -> Vec<Hit> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let phrase = words(query).collect::<Vec<_>>().join(" ");
    let docs: Vec<Doc> = library
        .notes
        .values()
        .map(|note| Doc {
            note,
            title: counts(&note.title),
            body: counts(&note.text),
            folder: note.id.folder().0.split('/').flat_map(words).map(|w| stem(&w).to_owned()).collect(),
        })
        .collect();
    let total = docs.len().max(1) as f32;
    let idf: HashMap<&str, f32> = terms
        .iter()
        .map(|t| {
            let df = docs.iter().filter(|d| d.title.contains_key(t) || d.body.contains_key(t)).count() as f32;
            (t.as_str(), (1. + total / (1. + df)).ln())
        })
        .collect();
    let mut hits: Vec<Hit> = docs
        .iter()
        .filter_map(|doc| {
            let mut score = 0.;
            let mut matched = 0;
            for term in &terms {
                let weight = idf[term.as_str()];
                let in_title = doc.title.contains_key(term);
                let in_body = doc.body.get(term).copied().unwrap_or(0);
                if in_title {
                    score += 4. * weight;
                }
                if in_body > 0 {
                    score += (1. + (in_body as f32).ln()) * weight;
                }
                if doc.folder.contains(term) {
                    score += weight;
                }
                if in_title || in_body > 0 || doc.folder.contains(term) {
                    matched += 1;
                }
            }
            if matched == 0 {
                return None;
            }
            let coverage = matched as f32 / terms.len() as f32;
            score *= 0.5 + coverage * coverage;
            if phrase.contains(' ') {
                if normalized(&doc.note.title).contains(&phrase) {
                    score += 12.;
                } else if normalized(&doc.note.text).contains(&phrase) {
                    score += 6.;
                }
            }
            Some(Hit {
                id: doc.note.id.clone(),
                title: doc.note.title.clone(),
                snippet: snippet(&doc.note.text, &phrase, &terms),
                score,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    hits.truncate(limit);
    hits
}

/// The text as words joined by single spaces, so a phrase matches across punctuation.
fn normalized(text: &str) -> String {
    words(text).collect::<Vec<_>>().join(" ")
}

/// The note's text around the phrase, else around its first matching word, else its opening,
/// on one line.
pub fn snippet(text: &str, phrase: &str, terms: &[String]) -> String {
    let body = crate::library::model::split(text).body;
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = flat.to_lowercase();
    let at = std::iter::once(phrase)
        .chain(terms.iter().map(String::as_str))
        .filter(|t| !t.is_empty())
        .find_map(|t| lower.find(t))
        .filter(|_| lower.len() == flat.len());
    let start_chars = at.map_or(0, |at| lower[..at].chars().count().saturating_sub(SNIPPET_CHARS / 3));
    let out: String = flat.chars().skip(start_chars).take(SNIPPET_CHARS).collect();
    let more = flat.chars().count() > start_chars + SNIPPET_CHARS;
    format!("{}{}{}", if start_chars > 0 { "…" } else { "" }, out, if more { "…" } else { "" })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::Timestamp;

    use super::*;

    fn library(notes: &[(&str, &str)]) -> Library {
        let mut library = Library::default();
        for (path, text) in notes {
            let id = NoteId((*path).to_owned());
            library.insert_note(Note::new(id, Arc::from(*text), Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH));
        }
        library
    }

    fn top(library: &Library, query: &str) -> Vec<String> {
        search(library, query, 5).into_iter().map(|h| h.id.0).collect()
    }

    #[test]
    fn a_title_beats_a_mention_in_passing() {
        let lib = library(&[
            ("Groceries.md", "# Groceries\n\nBuy bread and lentils."),
            ("Lentil soup.md", "# Lentil soup\n\nRed lentils, onion, stock."),
        ]);
        assert_eq!(top(&lib, "lentil")[0], "Lentil soup.md");
    }

    #[test]
    fn an_exact_phrase_beats_the_same_words_scattered() {
        let lib = library(&[
            ("a.md", "# Notes\n\nThe release went out. Later, a post about the date."),
            ("b.md", "# Plan\n\nWe write the release post on Friday."),
        ]);
        assert_eq!(top(&lib, "release post")[0], "b.md");
    }

    #[test]
    fn every_word_beats_one_word_many_times() {
        let lib = library(&[
            ("a.md", "# A\n\nbeta beta beta beta beta beta"),
            ("b.md", "# B\n\nThe beta ships on the 14th."),
        ]);
        assert_eq!(top(&lib, "when does the beta ship")[0], "b.md");
    }

    #[test]
    fn plurals_stop_words_and_case_do_not_matter() {
        let lib = library(&[("Recipes/Pancakes.md", "# Pancakes\n\nFlour, eggs, milk.")]);
        assert_eq!(top(&lib, "What are the PANCAKE recipes?"), ["Recipes/Pancakes.md"]);
        assert!(top(&lib, "the of and").is_empty(), "stop words alone find nothing here");
        assert!(top(&lib, "").is_empty());
        assert!(top(&lib, "submarine").is_empty());
    }

    #[test]
    fn snippets_open_at_the_match() {
        let text = format!("# T\n\n{} the needle sits here", "filler words ".repeat(40));
        let hit = snippet(&text, "needle", &["needle".into()]);
        assert!(hit.starts_with('…') && hit.contains("needle"), "{hit}");
        assert_eq!(snippet("# T\n\nShort body.", "x", &[]), "Short body.");
    }
}
