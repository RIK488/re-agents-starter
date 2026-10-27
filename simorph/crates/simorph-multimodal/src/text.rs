//! Text: normalisation, chunking and extraction of physical quantities.
//!
//! Extraction is rule-based (regular expressions + unit validation by
//! `simorph-units`).  It recognises numbers written in French or English
//! conventions, scientific notation and unit expressions; it does not
//! understand sentence meaning (see `services/problem-understanding`).

use regex::Regex;
use serde::{Deserialize, Serialize};
use simorph_core::{Quantity, Uncertainty};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberLocale {
    /// Comma is the decimal separator, space/narrow space groups thousands.
    French,
    /// Dot is the decimal separator, comma groups thousands.
    English,
}

/// NFKC normalisation plus whitespace canonicalisation (keeps `µ`, `°`, `Ω`
/// recognisable: NFKC maps U+00B5 to U+03BC, which simorph-units accepts).
pub fn normalise(text: &str) -> String {
    let t: String = text.nfkc().collect();
    let mut out = String::with_capacity(t.len());
    let mut prev_space = false;
    for c in t.chars() {
        let c = match c {
            '\u{2009}' | '\u{202F}' | '\u{00A0}' => ' ',
            '\u{2212}' => '-',
            c => c,
        };
        if c.is_whitespace() && c != '\n' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

/// Guess the number convention from function words.
pub fn guess_locale(text: &str) -> NumberLocale {
    let lower = format!(" {} ", text.to_lowercase());
    let fr = [
        " le ", " la ", " les ", " une ", " un ", " de ", " des ", " et ", " du ", " avec ",
        " pour ",
    ]
    .iter()
    .filter(|w| lower.contains(*w))
    .count();
    let en = [
        " the ", " a ", " an ", " of ", " and ", " with ", " for ", " is ",
    ]
    .iter()
    .filter(|w| lower.contains(*w))
    .count();
    if fr >= en {
        NumberLocale::French
    } else {
        NumberLocale::English
    }
}

/// Split into chunks of at most `max_chars` (on sentence boundaries when
/// possible) with `overlap` trailing characters repeated in the next chunk.
pub fn chunk(text: &str, max_chars: usize, overlap: usize) -> Vec<String> {
    let mut sentences = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        cur.push(*c);
        let end = matches!(c, '.' | '!' | '?' | '\n')
            && chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        if end {
            sentences.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        sentences.push(cur);
    }
    let mut chunks: Vec<String> = vec![];
    let mut acc = String::new();
    for s in sentences {
        if acc.chars().count() + s.chars().count() > max_chars && !acc.is_empty() {
            chunks.push(acc.trim().to_string());
            let tail: String = acc
                .chars()
                .rev()
                .take(overlap)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            acc = tail;
        }
        if s.chars().count() > max_chars {
            for piece in s.chars().collect::<Vec<_>>().chunks(max_chars.max(1)) {
                let p: String = piece.iter().collect();
                if !acc.is_empty() {
                    chunks.push(acc.trim().to_string());
                    acc.clear();
                }
                chunks.push(p.trim().to_string());
            }
            continue;
        }
        acc.push_str(&s);
    }
    if !acc.trim().is_empty() {
        chunks.push(acc.trim().to_string());
    }
    chunks.retain(|c| !c.is_empty());
    chunks
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedQuantity {
    /// Noun preceding the number, if any (e.g. "charge").
    pub name_hint: Option<String>,
    pub value: f64,
    /// Validated unit expression (`"1"` when no unit was written).
    pub unit: String,
    /// Value converted to SI base units of its dimension.
    pub si_value: f64,
    pub dimension: String,
    /// Unit-like token that followed the number but is not a known unit.
    pub unrecognised_unit: Option<String>,
    /// Byte span in the normalised text.
    pub span: (usize, usize),
    pub text: String,
}

const STOPWORDS: &[&str] = &[
    "de",
    "d",
    "du",
    "des",
    "la",
    "le",
    "les",
    "l",
    "un",
    "une",
    "et",
    "est",
    "à",
    "a",
    "au",
    "aux",
    "en",
    "avec",
    "pour",
    "sur",
    "par",
    "environ",
    "égale",
    "égal",
    "vaut",
    "of",
    "the",
    "an",
    "and",
    "is",
    "are",
    "with",
    "for",
    "at",
    "to",
    "about",
    "approximately",
    "equal",
    "=",
    ":",
    "≈",
    "~",
    "soit",
    "fait",
    "mesure",
    "mesurant",
    "d'une",
    "d'un",
];

fn number_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?x)
            (?P<num>[-+]?(?:\d{1,3}(?:[\ ,]\d{3})+|\d+)(?:[.,]\d+)?)
            (?:\s*(?:[eE](?P<e1>[-+]?\d+)|\s*[×x\*]\s*10\^?(?P<e2>[-+]?\d+)))?
            (?:\s?(?P<unit>[A-Za-zµμΩ°%][A-Za-z0-9µμΩ°%/\^\*\.·²³\-]*))?
        ",
        )
        .expect("valid regex")
    })
}

fn parse_number(s: &str, locale: NumberLocale) -> Option<f64> {
    let t: String = match locale {
        NumberLocale::French => s.replace(' ', "").replace(',', "."),
        NumberLocale::English => s.replace([' ', ','], ""),
    };
    t.parse().ok()
}

fn clean_unit(raw: &str) -> String {
    let mut u = raw
        .trim_end_matches(['.', '-', '/', '*', '·'])
        .replace('²', "^2")
        .replace('³', "^3")
        .replace('·', "*");
    // m2 → m^2, s-1 → s^-1 (letter immediately followed by an exponent)
    let re = Regex::new(r"([A-Za-zµμΩ])(-?\d)").expect("valid");
    u = re.replace_all(&u, "$1^$2").to_string();
    u
}

/// Extract every number (with or without unit) from text.
pub fn extract_quantities(text: &str, locale: Option<NumberLocale>) -> Vec<ExtractedQuantity> {
    let norm = normalise(text);
    let locale = locale.unwrap_or_else(|| guess_locale(&norm));
    let mut out = vec![];
    let mut last_end = 0usize;
    for cap in number_re().captures_iter(&norm) {
        let m = cap.get(0).expect("match");
        // skip digits that are part of a word (e.g. "H2O", "CO2")
        if m.start() > 0
            && norm[..m.start()]
                .chars()
                .last()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        {
            continue;
        }
        let Some(mut value) = parse_number(&cap["num"], locale) else {
            continue;
        };
        if let Some(e) = cap.name("e1").or(cap.name("e2")) {
            value *= 10f64.powi(e.as_str().parse::<i32>().unwrap_or(0));
        }
        let (unit, unrec, si, dim, end) = match cap.name("unit") {
            Some(u) => {
                let cu = clean_unit(u.as_str());
                match simorph_units::parse_unit(&cu) {
                    Ok(pu) => (cu, None, pu.to_si(value), pu.dimension.to_string(), m.end()),
                    Err(_) => (
                        "1".to_string(),
                        Some(u.as_str().to_string()),
                        value,
                        "1".to_string(),
                        u.start(),
                    ),
                }
            }
            None => ("1".to_string(), None, value, "1".to_string(), m.end()),
        };
        // the name hint is looked for only between the previous quantity and this one
        let before = &norm[last_end.min(m.start())..m.start()];
        let name_hint = before
            .rsplit([',', ';', '.', '(', ')', '\n'])
            .next()
            .unwrap_or("")
            .split_whitespace()
            .rev()
            .take(5)
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '\'')
                    .to_lowercase()
            })
            .map(|w| w.rsplit('\'').next().unwrap_or("").to_string())
            .find(|w| {
                !w.is_empty()
                    && !STOPWORDS.contains(&w.as_str())
                    && !w.chars().all(|c| c.is_ascii_digit())
            });
        last_end = end;
        out.push(ExtractedQuantity {
            name_hint,
            value,
            unit,
            si_value: si,
            dimension: dim,
            unrecognised_unit: unrec,
            span: (m.start(), end),
            text: norm[m.start()..end].trim().to_string(),
        });
    }
    out
}

/// Convert extractions into `Quantity` values (name = hint or `q<i>`).
pub fn to_quantities(ex: &[ExtractedQuantity]) -> Vec<Quantity> {
    ex.iter()
        .enumerate()
        .map(|(i, e)| {
            let mut q = Quantity::new(
                e.name_hint.clone().unwrap_or_else(|| format!("q{i}")),
                e.value,
                e.unit.clone(),
            )
            .with_uncertainty(Uncertainty::Undeclared);
            q.description = Some(format!("extrait du texte : « {} »", e.text));
            q
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_sentence() {
        let ex = extract_quantities("Une charge de 1,5 kN et une portée de 2 m.", None);
        assert_eq!(ex.len(), 2, "{ex:?}");
        assert_eq!(ex[0].name_hint.as_deref(), Some("charge"));
        assert_eq!(ex[0].value, 1.5);
        assert_eq!(ex[0].unit, "kN");
        assert_eq!(ex[0].si_value, 1500.0);
        assert_eq!(ex[1].name_hint.as_deref(), Some("portée"));
        assert_eq!(ex[1].unit, "m");
        assert_eq!(ex[1].value, 2.0);
    }

    #[test]
    fn english_scientific_and_compound_units() {
        let ex = extract_quantities("The satellite orbits at an altitude of 500 km with a power of 1.2e3 W and 3 satellites, density 2,700 kg/m3.", Some(NumberLocale::English));
        let alt = ex
            .iter()
            .find(|e| e.name_hint.as_deref() == Some("altitude"))
            .unwrap();
        assert_eq!(alt.si_value, 500_000.0);
        let p = ex.iter().find(|e| e.unit == "W").unwrap();
        assert_eq!(p.value, 1200.0);
        let sat = ex.iter().find(|e| e.value == 3.0).unwrap();
        assert_eq!(sat.unit, "1");
        assert_eq!(sat.unrecognised_unit.as_deref(), Some("satellites"));
        let rho = ex
            .iter()
            .find(|e| e.name_hint.as_deref() == Some("density"))
            .unwrap();
        assert_eq!(rho.value, 2700.0);
        assert_eq!(rho.unit, "kg/m^3");
    }

    #[test]
    fn temperatures_percent_and_chemical_formulae() {
        let ex = extract_quantities(
            "La température atteint 85 °C, le rendement 28 % ; le CO2 n'est pas une quantité.",
            None,
        );
        assert_eq!(ex.len(), 2, "{ex:?}");
        assert!((ex[0].si_value - 358.15).abs() < 1e-9);
        assert!((ex[1].si_value - 0.28).abs() < 1e-12);
    }

    #[test]
    fn chunking_respects_limits_and_overlap() {
        let text = "Phrase un. Phrase deux est un peu plus longue. Trois. ".repeat(10);
        let c = chunk(&text, 60, 10);
        assert!(c.len() > 5);
        assert!(c.iter().all(|x| x.chars().count() <= 70));
        assert_eq!(normalise("a\u{00A0}b\u{202F}c   d"), "a b c d");
    }
}
