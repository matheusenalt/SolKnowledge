use crate::{
    models::*,
    storage::{Store, StoreResult},
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

pub(crate) fn normalize(value: &str) -> String {
    value
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn words(value: &str) -> Vec<String> {
    normalize(value)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}
fn query_words(value: &str) -> Vec<String> {
    // A single character is a prefix, even when it is normally a stop word.
    if normalize(value).chars().count() == 1 {
        return words(value);
    }
    const STOP: &[&str] = &[
        "a", "as", "o", "os", "e", "de", "da", "das", "do", "dos", "em", "na", "nas", "no", "nos",
        "um", "uma", "para", "por", "com", "que", "eu", "me", "como", "ver",
    ];
    let mut seen = HashSet::new();
    words(value)
        .into_iter()
        .filter(|w| !STOP.contains(&w.as_str()) && seen.insert(w.clone()))
        .take(24)
        .collect()
}
struct Candidate {
    result: SearchResult,
    payload: Value,
    body: String,
}

impl Store {
    pub fn search(&mut self, request: SearchRequest) -> StoreResult<Vec<SearchResult>> {
        let query = request.query.trim();
        if query.chars().count() > 256 || query.chars().any(char::is_control) {
            return Err("A pesquisa aceita até 256 caracteres, sem controles".into());
        }
        let limit = request.limit.unwrap_or(30).clamp(1, 100) as usize;
        let tokens = query_words(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let normalized = normalize(query);
        let allowed = request
            .filters
            .as_ref()
            .map(|f| self.matching_entry_ids(f))
            .transpose()?;
        // FTS operators are not user syntax: only quoted letters/numbers reach MATCH.
        let fts_query = tokens
            .iter()
            .map(|t| {
                if tokens.len() == 1 && t.chars().count() == 1 {
                    format!("\"{t}\"*")
                } else {
                    format!("\"{t}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        let fts: HashMap<String, (f64, String)> = {
            let mut stmt = self.conn.prepare("SELECT entry_id,bm25(entries_fts,0,10,5,1),snippet(entries_fts,3,'','',' … ',24) FROM entries_fts WHERE entries_fts MATCH ?1")?;
            let rows = stmt.query_map([fts_query], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let aliases: HashSet<String> = {
            let mut stmt = self.conn.prepare("SELECT target_entry_id FROM synonyms WHERE normalized=?1 UNION SELECT target_entry_id FROM personal_vocabulary WHERE normalized=?1")?;
            let rows = stmt.query_map([&normalized], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        // Exact option-prefix matches and fuzzy complement scan persisted text locally.
        let candidates = {
            let mut stmt = self.conn.prepare("SELECT e.id,e.title,e.kind,a.id,a.name,m.id,m.name,e.payload_json,f.body,e.learning_status,EXISTS(SELECT 1 FROM favorites WHERE entry_id=e.id),EXISTS(SELECT 1 FROM blocks b WHERE b.entry_id=e.id AND b.kind IN ('diagram','topology','image','attachment')) FROM entries e JOIN modules m ON m.id=e.module_id JOIN areas a ON a.id=m.area_id JOIN entries_fts f ON f.entry_id=e.id")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    SearchResult {
                        visual: r.get(11)?,
                        id: r.get(0)?,
                        title: r.get(1)?,
                        kind: r.get(2)?,
                        area_id: r.get(3)?,
                        area_name: r.get(4)?,
                        module_id: r.get(5)?,
                        module_name: r.get(6)?,
                        snippet: String::new(),
                        match_kind: MatchKind::Approximate,
                        learning_status: r.get(9)?,
                        favorite: r.get(10)?,
                    },
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                ))
            })?;
            rows.map(|row| {
                let (result, payload, body) = row?;
                Ok(Candidate {
                    result,
                    payload: serde_json::from_str(&payload)?,
                    body,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?
        };
        let mut ranked = Vec::new();
        for mut c in candidates {
            if allowed
                .as_ref()
                .is_some_and(|ids| !ids.contains(&c.result.id))
            {
                continue;
            }
            let option = exact_option(&c.payload, query);
            let title_exact = if c.result.kind == "command" {
                c.result.title == query
            } else {
                normalize(&c.result.title) == normalized
            };
            let full_name_exact = c.payload["full_name"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && normalize(s) == normalized);
            let (kind, score) = if title_exact || full_name_exact || option.is_some() {
                (
                    MatchKind::Exact,
                    if title_exact {
                        -2.0
                    } else if full_name_exact {
                        -1.0
                    } else {
                        0.0
                    },
                )
            } else if let Some((bm25, _)) = fts.get(&c.result.id) {
                (MatchKind::Textual, *bm25)
            } else if aliases.contains(&c.result.id) {
                (MatchKind::Textual, -0.5)
            } else if let Some(distance) = fuzzy_distance(&tokens, &c.result.title, &c.body) {
                (MatchKind::Approximate, distance)
            } else {
                continue;
            };
            c.result.match_kind = kind;
            c.result.snippet = if let Some(option) = option {
                format!(
                    "{} — {}",
                    option["example"].as_str().unwrap_or(""),
                    option["description"].as_str().unwrap_or("")
                )
            } else if let Some((_, snippet)) = fts.get(&c.result.id) {
                snippet.clone()
            } else {
                c.payload["technical_description"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&c.result.title)
                    .chars()
                    .take(220)
                    .collect()
            };
            ranked.push((c.result, score));
        }
        ranked.sort_by(|(a, sa), (b, sb)| {
            a.match_kind
                .cmp(&b.match_kind)
                .then_with(|| sa.total_cmp(sb))
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        ranked.truncate(limit);
        self.conn
            .execute("INSERT INTO search_history(query) VALUES(?1)", [query])?;
        Ok(ranked.into_iter().map(|(r, _)| r).collect())
    }
}
fn exact_option<'a>(payload: &'a Value, query: &str) -> Option<&'a Value> {
    payload["options"].as_array()?.iter().find(|option| {
        option["example"].as_str().is_some_and(|example| {
            example == query
                || example
                    .strip_prefix(query)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        })
    })
}
fn fuzzy_distance(tokens: &[String], title: &str, body: &str) -> Option<f64> {
    let title_words = words(title);
    let body_words: HashSet<String> = words(body).into_iter().collect();
    let mut distance = 0.0;
    let mut had_typo = false;
    for token in tokens {
        if title_words.contains(token) {
            continue;
        }
        if body_words.contains(token) {
            distance += 0.5;
            continue;
        }
        let length = token.chars().count();
        if length < 4 {
            return None;
        }
        let threshold = if length >= 8 { 2 } else { 1 };
        let best = body_words
            .iter()
            .filter(|w| w.chars().count().abs_diff(length) <= threshold)
            .map(|word| {
                (
                    strsim::damerau_levenshtein(token, word),
                    !title_words.contains(word),
                )
            })
            .min()?;
        if best.0 > threshold {
            return None;
        }
        had_typo = true;
        distance += best.0 as f64 + if best.1 { 0.5 } else { 0.0 };
    }
    had_typo.then_some(distance)
}
