//! Quantity extraction: values, ranges, uncertainties, units.

use crate::lexicon::Lexicon;
use crate::text::{normalise_unit, stem, Tok};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QKind {
    /// A single value, optionally `± sd` (sd absolute, or relative when `sd_relative`).
    Value {
        value: f64,
        sd: Option<f64>,
        sd_relative: bool,
    },
    Range {
        low: f64,
        high: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedQuantity {
    pub sentence: usize,
    /// Token span `[start, end)` in the sentence.
    pub start: usize,
    pub end: usize,
    pub kind: QKind,
    /// Normalised unit expression accepted by `simorph_units` (None = no unit written).
    pub unit: Option<String>,
    /// Word right after the quantity when it is not a unit (`1 orbite`, `400 simulations`).
    pub counted_noun: Option<String>,
    /// Text of the quantity as written.
    pub text: String,
}

/// Words never read as units even though `simorph_units` would accept them
/// (`au` = astronomical unit but French "au", `in` = inch but English "in").
const NOT_UNITS: &[&str] = &["au", "in", "a", "de", "et", "en", "pour", "or", "on"];

/// Return the normalised unit if token `j` is a unit.
pub fn unit_at(toks: &[Tok], j: usize) -> Option<String> {
    let raw = toks.get(j)?.raw()?;
    // case-sensitive: "a" is French, "A" is the ampere
    if NOT_UNITS.contains(&raw) {
        return None;
    }
    let n = normalise_unit(raw);
    if n.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    simorph_units::parse_unit(&n).ok().map(|_| n)
}

fn num_at(toks: &[Tok], j: usize) -> Option<f64> {
    match toks.get(j) {
        Some(Tok::Num(v)) => Some(*v),
        _ => None,
    }
}

fn norm_at(toks: &[Tok], j: usize) -> Option<&str> {
    toks.get(j).and_then(|t| t.norm())
}

fn convert_or(v: f64, from: &Option<String>, to: &Option<String>) -> f64 {
    match (from, to) {
        (Some(f), Some(t)) if f != t => simorph_units::convert(v, f, t).unwrap_or(v),
        _ => v,
    }
}

fn span_text(toks: &[Tok], a: usize, b: usize) -> String {
    toks[a..b]
        .iter()
        .map(|t| match t {
            Tok::Num(v) => format!("{v}"),
            Tok::Word { raw, .. } => raw.clone(),
            Tok::PlusMinus => "±".into(),
            Tok::RangeDash => "–".into(),
            Tok::Sep(c) => c.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An issue found while extracting.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionIssue {
    pub token: usize,
    pub word: String,
}

/// Extract all quantities of a tokenised sentence.  Returns the quantities
/// and the words that follow a number but are neither units nor known words
/// (possible unknown units — reported, never guessed).
pub fn extract(
    toks: &[Tok],
    sentence: usize,
    lex: &Lexicon,
) -> (Vec<ExtractedQuantity>, Vec<ExtractionIssue>) {
    let mut out = vec![];
    let mut issues = vec![];
    let mut i = 0;
    while i < toks.len() {
        // entre/between N1 [U1] et/and N2 [U2]   |   de/from N1 [U1] à/to N2 [U2]
        if let (Some(w), Some(n1)) = (norm_at(toks, i), num_at(toks, i + 1)) {
            let pair = if w == stem("entre") || w == "between" {
                Some(["et", "and"].as_slice())
            } else if w == "de" || w == "from" || w == "du" {
                Some(["a", "à", "to", "jusqu"].as_slice())
            } else {
                None
            };
            if let Some(seps) = pair {
                let mut j = i + 2;
                let u1 = unit_at(toks, j);
                if u1.is_some() {
                    j += 1;
                }
                if let (Some(sep), Some(n2)) = (norm_at(toks, j), num_at(toks, j + 1)) {
                    if seps.iter().any(|s| stem(s) == sep) {
                        let mut k = j + 2;
                        let u2 = unit_at(toks, k);
                        if u2.is_some() {
                            k += 1;
                        }
                        let unit = u2.clone().or(u1.clone());
                        let lo = convert_or(n1, &u1, &unit);
                        let (low, high) = if lo <= n2 { (lo, n2) } else { (n2, lo) };
                        let counted = counted_noun(toks, k, lex, &unit, &mut issues);
                        out.push(ExtractedQuantity {
                            sentence,
                            start: i,
                            end: k,
                            kind: QKind::Range { low, high },
                            unit,
                            counted_noun: counted,
                            text: span_text(toks, i, k),
                        });
                        i = k;
                        continue;
                    }
                }
            }
        }
        if let Some(n1) = num_at(toks, i) {
            let mut j = i + 1;
            let u1 = unit_at(toks, j);
            if u1.is_some() {
                j += 1;
            }
            // N1 [U1] – N2 U2   or   N1 à/to N2 U2 (second unit required for "à")
            let dash = matches!(toks.get(j), Some(Tok::RangeDash));
            let a_sep = matches!(norm_at(toks, j), Some("a") | Some("à") | Some("to"));
            if (dash || a_sep) && num_at(toks, j + 1).is_some() {
                let n2 = num_at(toks, j + 1).unwrap();
                let mut k = j + 2;
                let u2 = unit_at(toks, k);
                if u2.is_some() {
                    k += 1;
                }
                if dash || u2.is_some() {
                    let unit = u2.clone().or(u1.clone());
                    let lo = convert_or(n1, &u1, &unit);
                    let (low, high) = if lo <= n2 { (lo, n2) } else { (n2, lo) };
                    let counted = counted_noun(toks, k, lex, &unit, &mut issues);
                    out.push(ExtractedQuantity {
                        sentence,
                        start: i,
                        end: k,
                        kind: QKind::Range { low, high },
                        unit,
                        counted_noun: counted,
                        text: span_text(toks, i, k),
                    });
                    i = k;
                    continue;
                }
            }
            // N1 [U1] ± N2 [U2|%]
            if matches!(toks.get(j), Some(Tok::PlusMinus)) {
                if let Some(n2) = num_at(toks, j + 1) {
                    let mut k = j + 2;
                    let u2 = unit_at(toks, k);
                    if u2.is_some() {
                        k += 1;
                    }
                    let (unit, sd, rel) = match (&u1, &u2) {
                        (_, Some(p)) if p == "%" && u1.as_deref() != Some("%") => {
                            (u1.clone(), n2 / 100.0, true)
                        }
                        (None, Some(_)) => (u2.clone(), n2, false),
                        (Some(_), Some(_)) => (u1.clone(), convert_or(n2, &u2, &u1), false),
                        _ => (u1.clone(), n2, false),
                    };
                    let counted = counted_noun(toks, k, lex, &unit, &mut issues);
                    out.push(ExtractedQuantity {
                        sentence,
                        start: i,
                        end: k,
                        kind: QKind::Value {
                            value: n1,
                            sd: Some(sd),
                            sd_relative: rel,
                        },
                        unit,
                        counted_noun: counted,
                        text: span_text(toks, i, k),
                    });
                    i = k;
                    continue;
                }
            }
            // calendar years ("en 2025", "in 2030") are not quantities
            let year_marker = i > 0
                && matches!(
                    norm_at(toks, i - 1),
                    Some("en") | Some("in") | Some("annee") | Some("year")
                );
            if u1.is_none() && year_marker && n1.fract() == 0.0 && (1800.0..=2200.0).contains(&n1) {
                i = j;
                continue;
            }
            let counted = counted_noun(toks, j, lex, &u1, &mut issues);
            out.push(ExtractedQuantity {
                sentence,
                start: i,
                end: j,
                kind: QKind::Value {
                    value: n1,
                    sd: None,
                    sd_relative: false,
                },
                unit: u1,
                counted_noun: counted,
                text: span_text(toks, i, j),
            });
            i = j;
            continue;
        }
        i += 1;
    }
    (out, issues)
}

fn counted_noun(
    toks: &[Tok],
    k: usize,
    lex: &Lexicon,
    unit: &Option<String>,
    issues: &mut Vec<ExtractionIssue>,
) -> Option<String> {
    let Some(Tok::Word { raw, norm }) = toks.get(k) else {
        return None;
    };
    if unit.is_some() {
        return None;
    }
    if crate::lexicon::contains_word(crate::lexicon::BUDGET_NOUNS, norm) {
        return Some(norm.clone());
    }
    if lex.known_words.contains(norm) {
        if lex.is_stop(norm) {
            return None;
        }
        return Some(norm.clone());
    }
    // An unknown word glued to a number is reported as a possible unknown unit.
    issues.push(ExtractionIssue {
        token: k,
        word: raw.clone(),
    });
    None
}
