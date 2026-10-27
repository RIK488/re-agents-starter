//! Text normalisation and tokenisation (French and English).

/// Fold accents and lowercase.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let r = match c {
            'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
            'ç' | 'Ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ÿ' => 'y',
            'œ' | 'Œ' => {
                out.push('o');
                'e'
            }
            'æ' | 'Æ' => {
                out.push('a');
                'e'
            }
            c => c.to_lowercase().next().unwrap_or(c),
        };
        out.push(r);
    }
    out
}

/// Light stemmer shared by phrases and lexicon entries: fold, drop plural
/// `s`/`x`, drop a final mute `e`.  It only needs to be consistent, not
/// linguistically exact.
pub fn stem(word: &str) -> String {
    let folded = fold(word);
    let mut w = folded.clone();
    w.retain(|c| c.is_alphanumeric() || c == '-');
    if w.is_empty() {
        // pure symbols (≤, <, %) are kept as written
        return folded;
    }
    if w.chars().count() > 3 && (w.ends_with('s') || w.ends_with('x')) {
        w.pop();
    }
    if w.chars().count() > 4 && w.ends_with('e') {
        w.pop();
    }
    if w.chars().count() > 4 && w.ends_with('e') {
        // "consommee" → "consomm"
        w.pop();
    }
    w
}

/// Replace superscripts and typographic symbols so that unit expressions
/// can be parsed by `simorph_units` (`m³` → `m^3`, `s⁻¹` → `s^-1`, `·` kept).
pub fn normalise_unit(raw: &str) -> String {
    let mut out = String::new();
    let mut in_sup = false;
    for c in raw.chars() {
        let sup = match c {
            '⁰' => Some('0'),
            '¹' => Some('1'),
            '²' => Some('2'),
            '³' => Some('3'),
            '⁴' => Some('4'),
            '⁵' => Some('5'),
            '⁶' => Some('6'),
            '⁷' => Some('7'),
            '⁸' => Some('8'),
            '⁹' => Some('9'),
            '⁻' => Some('-'),
            _ => None,
        };
        match sup {
            Some(d) => {
                if !in_sup {
                    out.push('^');
                    in_sup = true;
                }
                out.push(d);
            }
            None => {
                in_sup = false;
                match c {
                    '−' => out.push('-'),
                    '×' | '·' | '⋅' => out.push('*'),
                    c => out.push(c),
                }
            }
        }
    }
    out
}

/// Parse a number written in French or English style: `2,5`, `2.5`,
/// `1,5e3`, `2.1×10^11`, `2,1·10¹¹`, `−40`.  Thousands separators are handled
/// by the tokenizer (separate chunks).
pub fn parse_number(chunk: &str) -> Option<f64> {
    let s = normalise_unit(chunk).replace('−', "-");
    // split mantissa × 10^exp
    let (mant, exp10) = match s
        .find("*10^")
        .or_else(|| s.find("x10^"))
        .or_else(|| s.find("·10^"))
    {
        Some(p) => {
            let sep_len = if s[p..].starts_with('·') {
                '·'.len_utf8() + 3
            } else {
                4
            };
            let e: i32 = s[p + sep_len..].parse().ok()?;
            (&s[..p], e)
        }
        None => (s.as_str(), 0),
    };
    let mut m = mant.to_string();
    // decimal comma only when between digits
    if let Some(p) = m.find(',') {
        let b = m.as_bytes();
        if p > 0
            && p + 1 < b.len()
            && b[p - 1].is_ascii_digit()
            && b[p + 1].is_ascii_digit()
            && m.matches(',').count() == 1
        {
            m.replace_range(p..p + 1, ".");
        } else {
            return None;
        }
    }
    let first = m.chars().next()?;
    if !(first.is_ascii_digit() || ((first == '-' || first == '+') && m.len() > 1)) {
        return None;
    }
    let mut seen_digit = false;
    for c in m.chars() {
        if c.is_ascii_digit() {
            seen_digit = true;
        } else if !matches!(c, '.' | '-' | '+' | 'e' | 'E') {
            return None;
        }
    }
    if !seen_digit {
        return None;
    }
    let v: f64 = m.parse().ok()?;
    Some(v * 10f64.powi(exp10))
}

/// A token of a sentence.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    /// A word or symbol chunk: original text and stemmed form.
    Word {
        raw: String,
        norm: String,
    },
    PlusMinus,
    RangeDash,
    /// Clause separator: `,` `;` `:` `(` `)`.
    Sep(char),
}

impl Tok {
    pub fn norm(&self) -> Option<&str> {
        match self {
            Tok::Word { norm, .. } => Some(norm),
            _ => None,
        }
    }
    pub fn raw(&self) -> Option<&str> {
        match self {
            Tok::Word { raw, .. } => Some(raw),
            _ => None,
        }
    }
}

fn word(raw: &str) -> Tok {
    Tok::Word {
        raw: raw.to_string(),
        norm: stem(raw),
    }
}

fn is_number_chunk(s: &str) -> bool {
    parse_number(s).is_some()
}

/// Split text into sentences (on `.`, `!`, `?`, newlines) keeping decimal
/// points inside numbers.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let end = match c {
            '!' | '?' | '\n' => true,
            '.' => {
                let next = chars.get(i + 1).copied();
                let prev = if i > 0 { Some(chars[i - 1]) } else { None };
                let decimal = matches!((prev, next), (Some(p), Some(n)) if p.is_ascii_digit() && n.is_ascii_digit());
                !decimal && next.map(|n| n.is_whitespace()).unwrap_or(true)
            }
            _ => false,
        };
        if end {
            if c == '?' {
                cur.push(' ');
                cur.push('?');
            }
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            cur.clear();
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Tokenise one sentence.
pub fn tokenize(sentence: &str) -> Vec<Tok> {
    let mut raw_chunks: Vec<String> = vec![];
    for chunk in sentence.split_whitespace() {
        raw_chunks.push(chunk.replace('\u{a0}', " ").replace('\u{202f}', " "));
    }
    // re-split on narrow/non-breaking spaces converted above
    let chunks: Vec<String> = raw_chunks
        .iter()
        .flat_map(|c| c.split(' ').map(|s| s.to_string()).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .collect();
    let mut toks: Vec<Tok> = vec![];
    for chunk in chunks {
        let mut c = chunk.as_str();
        let mut leading = vec![];
        while let Some(f) = c.chars().next() {
            if matches!(f, '(' | '"' | '«' | '“' | '[') {
                leading.push(f);
                c = &c[f.len_utf8()..];
            } else {
                break;
            }
        }
        for l in leading {
            if l == '(' {
                toks.push(Tok::Sep('('));
            }
        }
        let mut trailing = vec![];
        loop {
            let Some(l) = c.chars().last() else { break };
            let strip = match l {
                ',' | ';' | ':' | '.' | '!' | '?' | '"' | '»' | '”' | ']' => true,
                ')' => c.matches('(').count() < c.matches(')').count(),
                _ => false,
            };
            if strip {
                trailing.push(l);
                c = &c[..c.len() - l.len_utf8()];
            } else {
                break;
            }
        }
        if !c.is_empty() {
            push_chunk(&mut toks, c);
        }
        for t in trailing.into_iter().rev() {
            if matches!(t, ',' | ';' | ':' | ')') {
                toks.push(Tok::Sep(t));
            }
        }
    }
    toks
}

fn push_chunk(toks: &mut Vec<Tok>, c: &str) {
    if c == "±" || c == "+/-" || c == "+-" {
        toks.push(Tok::PlusMinus);
        return;
    }
    if c == "–" || c == "—" || c == "-" {
        toks.push(Tok::RangeDash);
        return;
    }
    if matches!(c, "≤" | "<=" | "<" | "≥" | ">=" | ">" | "=") {
        toks.push(word(c));
        return;
    }
    // elision: l'état, d'éclipse, qu'il, Young's
    for apo in ['\'', '’'] {
        if let Some(p) = c.find(apo) {
            let (a, b) = (&c[..p], &c[p + apo.len_utf8()..]);
            if !a.is_empty() && !b.is_empty() && a.chars().all(|x| x.is_alphabetic()) {
                toks.push(word(a));
                push_chunk(toks, b);
                return;
            }
        }
    }
    if is_number_chunk(c) {
        toks.push(Tok::Num(parse_number(c).unwrap()));
        return;
    }
    // 14±2, 300–1200, 300-1200
    for sep in ["±", "–", "—"] {
        if let Some(p) = c.find(sep) {
            let (a, b) = (&c[..p], &c[p + sep.len()..]);
            if is_number_chunk(a) {
                toks.push(Tok::Num(parse_number(a).unwrap()));
                toks.push(if sep == "±" {
                    Tok::PlusMinus
                } else {
                    Tok::RangeDash
                });
                if !b.is_empty() {
                    push_chunk(toks, b);
                }
                return;
            }
        }
    }
    let skip = c.chars().next().map(|x| x.len_utf8()).unwrap_or(0);
    if let Some(p) = c[skip..].find('-').map(|p| p + skip) {
        let (a, b) = (&c[..p], &c[p + 1..]);
        if is_number_chunk(a)
            && b.chars()
                .next()
                .map(|x| x.is_ascii_digit())
                .unwrap_or(false)
        {
            toks.push(Tok::Num(parse_number(a).unwrap()));
            toks.push(Tok::RangeDash);
            push_chunk(toks, b);
            return;
        }
    }
    // glued number + unit: 500km, 20mm (only if the rest parses as a unit)
    let digits_end = c
        .char_indices()
        .take_while(|(i, ch)| {
            ch.is_ascii_digit()
                || *ch == '.'
                || *ch == ','
                || (*i == 0 && (*ch == '-' || *ch == '−'))
        })
        .map(|(i, ch)| i + ch.len_utf8())
        .last();
    if let Some(e) = digits_end {
        let (a, b) = (&c[..e], &c[e..]);
        if is_number_chunk(a)
            && !b.is_empty()
            && simorph_units::parse_unit(&normalise_unit(b)).is_ok()
            && b.chars()
                .next()
                .map(|x| x.is_alphabetic() || x == '°' || x == '%')
                .unwrap_or(false)
        {
            toks.push(Tok::Num(parse_number(a).unwrap()));
            toks.push(word(b));
            return;
        }
    }
    // hyphenated words: sous-système → sous, système (units like s^-1 keep their hyphen)
    if c.contains('-')
        && c.split('-')
            .all(|p| !p.is_empty() && p.chars().all(|x| x.is_alphabetic()))
    {
        for p in c.split('-') {
            toks.push(word(p));
        }
        return;
    }
    toks.push(word(c));
}

/// Tokenize a sentence, first merging thousands separators exactly
/// (`1 000`, `10 000 000`, with normal, non-breaking or narrow spaces).
pub fn tokenize_sentence(sentence: &str) -> Vec<Tok> {
    let chars: Vec<char> = sentence.chars().collect();
    let is_sep = |c: char| c == ' ' || c == '\u{a0}' || c == '\u{202f}';
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let at_run_start = c.is_ascii_digit()
            && (i == 0
                || !(chars[i - 1].is_ascii_digit() || chars[i - 1] == ',' || chars[i - 1] == '.'));
        if at_run_start {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let run: String = chars[i..j].iter().collect();
            out.push_str(&run);
            if run.len() <= 3 {
                loop {
                    if j < chars.len()
                        && is_sep(chars[j])
                        && j + 4 <= chars.len()
                        && chars[j + 1..j + 4].iter().all(|d| d.is_ascii_digit())
                        && !chars
                            .get(j + 4)
                            .map(|d| d.is_ascii_digit())
                            .unwrap_or(false)
                    {
                        out.extend(chars[j + 1..j + 4].iter());
                        j += 4;
                    } else {
                        break;
                    }
                }
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    tokenize(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("2,5"), Some(2.5));
        assert_eq!(parse_number("1,5e3"), Some(1500.0));
        assert_eq!(parse_number("2.1×10^11"), Some(2.1e11));
        assert_eq!(parse_number("2,1×10¹¹"), Some(2.1e11));
        assert_eq!(parse_number("−40"), Some(-40.0));
        assert_eq!(parse_number("3U"), None);
        assert_eq!(parse_number("1,"), None);
    }

    #[test]
    fn tokens_and_thousands() {
        let t = tokenize_sentence("Une masse de 1 000 kg, et 2,5 m/s.");
        assert!(t.contains(&Tok::Num(1000.0)), "{t:?}");
        assert!(t.contains(&Tok::Num(2.5)));
        let t = tokenize_sentence("de 300–1200 cm² et 14±2 W");
        assert!(
            t.contains(&Tok::RangeDash) && t.contains(&Tok::PlusMinus),
            "{t:?}"
        );
        let t = tokenize_sentence("l'état de charge");
        assert_eq!(t[1].norm(), Some("etat"));
    }

    #[test]
    fn sentence_split_keeps_decimals() {
        let s = sentences("Le rendement vaut 0.28. Choisir la surface ? Fin");
        assert_eq!(s.len(), 3, "{s:?}");
        assert!(s[0].contains("0.28"));
    }

    #[test]
    fn units_normalised() {
        assert_eq!(normalise_unit("kg/m³"), "kg/m^3");
        assert_eq!(normalise_unit("s⁻¹"), "s^-1");
        assert!(simorph_units::parse_unit(&normalise_unit("mPa·s")).is_ok());
        assert!(simorph_units::parse_unit(&normalise_unit("cm⁴")).is_ok());
    }
}
