//! Deterministic natural-language → `ProblemSpec` pipeline.

use crate::catalog::{Catalog, PortInfo};
use crate::lexicon::*;
use crate::quantity::{extract, ExtractedQuantity, QKind};
use crate::text::{sentences, stem, tokenize_sentence, Tok};
use serde::{Deserialize, Serialize};
use simorph_core::*;
use simorph_units::Dimension;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityKind {
    UnknownUnit,
    UnmappedConcept,
    MultipleCandidates,
    RangeRoleUnclear,
    NominalFromRange,
    DuplicateQuantity,
    ConstraintOnInput,
    NoObjectiveOrUnknown,
    LowConfidence,
    InvalidProposal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ambiguity {
    pub kind: AmbiguityKind,
    /// Sentence index (0-based), when applicable.
    pub sentence: Option<usize>,
    pub text: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MentionRole {
    Known,
    DesignVariable,
    Uncertainty,
    Objective,
    Constraint,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptMapping {
    pub sentence: usize,
    pub phrase: String,
    pub role: MentionRole,
    pub port: String,
    pub port_unit: String,
    pub components: Vec<String>,
    /// 0–1: weighted coverage of the phrase by a synonym, plus component coherence.
    pub confidence: f64,
    /// Next best (port, confidence) pairs.
    pub alternatives: Vec<(String, f64)>,
    /// `deterministic` or `llm`.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderstandingResult {
    pub spec: ProblemSpec,
    pub mappings: Vec<ConceptMapping>,
    pub ambiguities: Vec<Ambiguity>,
    pub unsupported_phrases: Vec<String>,
    pub quantities: Vec<ExtractedQuantity>,
}

impl UnderstandingResult {
    pub fn has(&self, k: AmbiguityKind) -> bool {
        self.ambiguities.iter().any(|a| a.kind == k)
    }

    pub fn to_markdown(&self) -> String {
        let mut s = format!("## Compréhension du problème — « {} »\n\n", self.spec.title);
        s.push_str("| Phrase | Rôle | Port | Confiance | Alternatives |\n|---|---|---|---|---|\n");
        for m in &self.mappings {
            s.push_str(&format!(
                "| {} | {:?} | `{}` [{}] | {:.2} | {} |\n",
                m.phrase.replace('|', "\\|"),
                m.role,
                m.port,
                m.port_unit,
                m.confidence,
                m.alternatives
                    .iter()
                    .map(|(p, c)| format!("{p} ({c:.2})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.ambiguities.is_empty() {
            s.push_str("\n### Ambiguïtés (non résolues automatiquement)\n");
            for a in &self.ambiguities {
                s.push_str(&format!(
                    "- **{:?}** : {} — « {} »\n",
                    a.kind, a.message, a.text
                ));
            }
        }
        if !self.unsupported_phrases.is_empty() {
            s.push_str("\n### Passages non pris en charge\n");
            for p in &self.unsupported_phrases {
                s.push_str(&format!("- {p}\n"));
            }
        }
        s
    }
}

/// Minimum confidence for a concept mapping to be used in the spec.
pub const MAPPING_THRESHOLD: f64 = 0.3;

/// A concept mentioned in the text that must be mapped to a port.
#[derive(Debug, Clone)]
struct Mention {
    sentence: usize,
    role: MentionRole,
    /// Content tokens, nearest to the anchor first.
    before: Vec<String>,
    /// Content tokens after the anchor (forward order), used as fallback.
    after: Vec<String>,
    counted: Option<String>,
    input: bool,
    dim: Option<Dimension>,
    phrase: String,
    quantity: Option<ExtractedQuantity>,
    direction: Option<Direction>,
    constraint: Option<char>,
    hard: bool,
    /// "qui ne doit pas dépasser…": refers to the previous output concept.
    pronoun: bool,
    /// Resolved port (filled in later).
    port: Option<String>,
    confidence: f64,
    alternatives: Vec<(String, f64)>,
    components: Vec<String>,
    /// Score difference between the chosen port and the runner-up (uncapped).
    margin: f64,
}

fn weighted_coverage(tokens: &[String], syn: &[String]) -> f64 {
    if tokens.is_empty() || syn.is_empty() {
        return 0.0;
    }
    let k = tokens.len().min(5);
    let toks = &tokens[..k];
    let tok_match = |a: &str, b: &str| {
        a == b || (a.len() >= 5 && b.len() >= 5 && (a.starts_with(b) || b.starts_with(a)))
    };
    if !syn.iter().all(|s| toks.iter().any(|t| tok_match(t, s))) {
        return 0.0;
    }
    let mut total = 0.0;
    let mut covered = 0.0;
    for (i, t) in toks.iter().enumerate() {
        let w = 0.6f64.powi(i as i32);
        total += w;
        if syn.iter().any(|s| tok_match(t, s)) {
            covered += w;
        }
    }
    let specificity = (syn.len() as f64 / 2.0).min(1.0);
    covered / total * (0.75 + 0.25 * specificity)
}

fn score_port(
    lex: &Lexicon,
    port: &PortInfo,
    before: &[String],
    after: &[String],
    counted: &Option<String>,
) -> f64 {
    let syns = lex.synonyms_of(&port.name);
    let mut best: f64 = 0.0;
    for s in &syns {
        if let Some(c) = counted {
            best = best.max(weighted_coverage(std::slice::from_ref(c), s));
        }
        best = best.max(weighted_coverage(before, s));
        best = best.max(0.95 * weighted_coverage(after, s));
    }
    best
}

fn content(toks: &[Tok], lex: &Lexicon, skip: &dyn Fn(&str) -> bool) -> Vec<String> {
    toks.iter()
        .filter_map(|t| t.norm())
        .filter(|n| !lex.is_stop(n) && !skip(n) && n.chars().any(|c| c.is_alphabetic()))
        .map(|s| s.to_string())
        .collect()
}

fn find_seq(toks: &[Tok], seq: &[&str]) -> Option<usize> {
    let stems: Vec<String> = seq.iter().map(|w| stem(w)).collect();
    if stems.is_empty() || toks.len() < stems.len() {
        return None;
    }
    (0..=toks.len() - stems.len()).find(|&i| {
        stems
            .iter()
            .enumerate()
            .all(|(k, s)| toks[i + k].norm() == Some(s.as_str()))
    })
}

fn keyword_words() -> std::collections::HashSet<String> {
    let mut s = std::collections::HashSet::new();
    for list in [
        DESIGN_VERBS,
        UNCERTAINTY_WORDS,
        SOFT_WORDS,
        BUDGET_WORDS,
        MIN_VERBS,
        MAX_VERBS,
        UNKNOWN_VERBS,
    ] {
        for w in list {
            s.insert(stem(w));
        }
    }
    for w in [
        "rester", "être", "vaut", "fixer", "couvre", "est", "sont", "valeur", "mesure", "mesurer",
        "évolue", "carrying", "porte",
    ] {
        s.insert(stem(w));
    }
    s
}

fn unit_dim(unit: &Option<String>) -> Dimension {
    unit.as_ref()
        .and_then(|u| simorph_units::parse_unit(u).ok())
        .map(|u| u.dimension)
        .unwrap_or(Dimension::NONE)
}

fn clause_start(toks: &[Tok], before: usize) -> usize {
    let mut k = before;
    while k > 0 {
        if matches!(
            toks[k - 1],
            Tok::Sep(';') | Tok::Sep(',') | Tok::Sep(':') | Tok::Sep('(')
        ) {
            return k;
        }
        k -= 1;
    }
    0
}

fn clause_end(toks: &[Tok], from: usize) -> usize {
    let stops: Vec<String> = [
        "qui", "which", "tout", "while", "pour", "en", "subject", "afin", "et", "and", "to", "so",
    ]
    .iter()
    .map(|w| stem(w))
    .collect();
    let mut k = from;
    while k < toks.len() {
        match &toks[k] {
            Tok::Sep(_) | Tok::Num(_) => return k,
            Tok::Word { norm, .. } if stops.contains(norm) && k > from + 1 => return k,
            _ => {}
        }
        k += 1;
    }
    k
}

/// The deterministic interpreter.
pub fn understand(text: &str, manifests: &[ComponentManifest]) -> UnderstandingResult {
    let lex = Lexicon::load();
    let catalog = Catalog::from_manifests(manifests);
    let kw = keyword_words();
    let is_kw = |n: &str| kw.contains(n);
    let sents = sentences(text);
    let mut ambiguities: Vec<Ambiguity> = vec![];
    let mut unsupported: Vec<String> = vec![];
    let mut mentions: Vec<Mention> = vec![];
    let mut all_q: Vec<ExtractedQuantity> = vec![];
    let mut spec = ProblemSpec::new(title_of(&sents), text.trim());
    let folded_text = crate::text::fold(text);

    for (si, sentence) in sents.iter().enumerate() {
        let toks = tokenize_sentence(sentence);
        let (qs, issues) = extract(&toks, si, &lex);
        let has = |list: &[&str]| {
            toks.iter()
                .filter_map(|t| t.norm())
                .any(|n| contains_word(list, n))
        };
        let design = has(DESIGN_VERBS) || find_seq(&toks, &["à", "choisir"]).is_some();
        let uncertain = has(UNCERTAINTY_WORDS);
        let soft = has(SOFT_WORDS);

        // --- objectives and unknowns (concepts after a verb)
        for (k, t) in toks.iter().enumerate() {
            let Some(n) = t.norm() else { continue };
            let dir = if contains_word(MIN_VERBS, n) {
                Some(Direction::Minimize)
            } else if contains_word(MAX_VERBS, n) {
                Some(Direction::Maximize)
            } else {
                None
            };
            let unknown = contains_word(UNKNOWN_VERBS, n);
            if dir.is_none() && !unknown {
                continue;
            }
            let end = clause_end(&toks, k + 1);
            // split the clause into several concepts on "et"/"and"/"ainsi que"
            let mut groups: Vec<Vec<Tok>> = vec![vec![]];
            for t2 in &toks[k + 1..end] {
                if matches!(t2.norm(), Some("et") | Some("and")) {
                    groups.push(vec![]);
                } else {
                    groups.last_mut().unwrap().push(t2.clone());
                }
            }
            // "et" ends the clause in clause_end only after the first word; collect following "et X" groups for unknowns
            if unknown {
                let mut j = end;
                while j < toks.len()
                    && (matches!(toks[j].norm(), Some("et") | Some("and"))
                        || matches!(toks[j], Tok::Sep(',')))
                {
                    let mut k2 = j + 1;
                    if k2 < toks.len() && matches!(toks[k2].norm(), Some("et") | Some("and")) {
                        k2 += 1;
                    }
                    let e2 = clause_end(&toks, k2);
                    if e2 == k2 {
                        break;
                    }
                    groups.push(toks[k2..e2].to_vec());
                    j = e2;
                }
            }
            for g in groups {
                let c = content(&g, &lex, &is_kw);
                if c.is_empty() {
                    continue;
                }
                let phrase = g
                    .iter()
                    .filter_map(|t| t.raw())
                    .collect::<Vec<_>>()
                    .join(" ");
                mentions.push(Mention {
                    sentence: si,
                    role: if dir.is_some() {
                        MentionRole::Objective
                    } else {
                        MentionRole::Unknown
                    },
                    before: c.clone(),
                    after: vec![],
                    counted: None,
                    input: false,
                    dim: None,
                    phrase,
                    quantity: None,
                    direction: dir,
                    constraint: None,
                    hard: true,
                    pronoun: false,
                    port: None,
                    confidence: 0.0,
                    alternatives: vec![],
                    components: vec![],
                    margin: 0.0,
                });
            }
        }
        // superlatives ("le plus léger possible")
        for (seq, minimise, implied) in SUPERLATIVES {
            if let Some(p) = find_seq(&toks, seq) {
                let start = clause_start(&toks, p);
                let mut c: Vec<String> = implied.iter().map(|w| stem(w)).collect();
                let mut ctx = content(&toks[start..p], &lex, &is_kw);
                ctx.reverse();
                c.extend(ctx.into_iter().take(2));
                mentions.push(Mention {
                    sentence: si,
                    role: MentionRole::Objective,
                    before: c,
                    after: vec![],
                    counted: None,
                    input: false,
                    dim: None,
                    phrase: seq.join(" "),
                    quantity: None,
                    direction: Some(if *minimise {
                        Direction::Minimize
                    } else {
                        Direction::Maximize
                    }),
                    constraint: None,
                    hard: true,
                    pronoun: false,
                    port: None,
                    confidence: 0.0,
                    alternatives: vec![],
                    components: vec![],
                    margin: 0.0,
                });
            }
        }

        // --- quantities
        let mut prev_end = 0usize;
        for q in qs {
            // unknown unit right after the number → do not use the quantity
            if let Some(issue) = issues.iter().find(|x| x.token == q.end) {
                ambiguities.push(Ambiguity {
                    kind: AmbiguityKind::UnknownUnit,
                    sentence: Some(si),
                    text: format!("{} {}", q.text, issue.word),
                    message: format!("« {} » n'est ni une unité reconnue ni un mot connu : la valeur n'est pas utilisée", issue.word),
                });
                unsupported.push(format!("{} {}", q.text, issue.word));
                prev_end = q.end + 1;
                continue;
            }
            let anchor = prev_end.max(clause_start(&toks, q.start));
            let window = &toks[anchor..q.start];
            let after_end = clause_end(&toks, q.end);
            let after_toks = if after_end > q.end {
                &toks[q.end..after_end.min(q.end + 4)]
            } else {
                &toks[q.end..q.end]
            };
            prev_end = q.end;
            all_q.push(q.clone());
            // budget
            let budget_noun = q
                .counted_noun
                .as_deref()
                .map(|n| contains_word(BUDGET_NOUNS, n))
                .unwrap_or(false);
            let in_budget_sentence = toks.iter().filter_map(|t| t.norm()).any(|n| n == "budget");
            if let QKind::Value { value, .. } = q.kind {
                if budget_noun {
                    spec.budget.max_runs = value.max(1.0) as u64;
                    continue;
                }
                if in_budget_sentence && unit_dim(&q.unit) == Dimension::TIME {
                    if let Ok(s) = simorph_units::convert(value, q.unit.as_deref().unwrap(), "s") {
                        spec.budget.max_wall_time_s = s;
                        continue;
                    }
                }
            }
            // constraint keyword inside the window
            let mut ckind: Option<char> = None;
            let mut kw_span: Option<(usize, usize)> = None;
            for (seq, kind) in CONSTRAINT_KEYWORDS {
                if let Some(p) = find_seq(window, seq) {
                    ckind = Some(*kind);
                    kw_span = Some((p, p + seq.len()));
                    break;
                }
            }
            let window_wo_kw: Vec<Tok> = window
                .iter()
                .enumerate()
                .filter(|(k, _)| kw_span.map(|(a, b)| *k < a || *k >= b).unwrap_or(true))
                .map(|(_, t)| t.clone())
                .collect();
            let mut before = content(&window_wo_kw, &lex, &is_kw);
            before.reverse();
            let after = content(after_toks, &lex, &is_kw);
            let dim = Some(unit_dim(&q.unit));
            let phrase = window
                .iter()
                .chain(toks[q.start..q.end].iter())
                .filter_map(|t| t.raw())
                .collect::<Vec<_>>()
                .join(" ")
                + &format!(" [{}]", q.text);
            let (role, input) = match (&q.kind, ckind) {
                (QKind::Range { .. }, _) if design => (MentionRole::DesignVariable, true),
                (QKind::Range { .. }, _) if uncertain => (MentionRole::Uncertainty, true),
                (QKind::Range { .. }, _) => {
                    ambiguities.push(Ambiguity {
                        kind: AmbiguityKind::RangeRoleUnclear,
                        sentence: Some(si),
                        text: q.text.clone(),
                        message: "intervalle sans verbe de conception ni mot d'incertitude : interprété comme variable de conception (à confirmer)".into(),
                    });
                    (MentionRole::DesignVariable, true)
                }
                (QKind::Value { .. }, Some(_)) => (MentionRole::Constraint, false),
                (QKind::Value { .. }, None) => (MentionRole::Known, true),
            };
            let pronoun = before.is_empty()
                && window
                    .iter()
                    .filter_map(|t| t.norm())
                    .any(|n| n == "qui" || n == "which" || n == "it" || n == "elle");
            mentions.push(Mention {
                sentence: si,
                role,
                before,
                after,
                counted: q.counted_noun.clone(),
                input,
                dim,
                phrase,
                quantity: Some(q),
                direction: None,
                constraint: ckind,
                hard: !soft,
                pronoun,
                port: None,
                confidence: 0.0,
                alternatives: vec![],
                components: vec![],
                margin: 0.0,
            });
        }
    }

    resolve(&mut mentions, &catalog, &lex);
    build_spec(
        &mut spec,
        &mut mentions,
        &catalog,
        &mut ambiguities,
        &mut unsupported,
    );
    spec.domains = DOMAINS
        .iter()
        .filter(|(_, ws)| {
            ws.iter()
                .any(|w| folded_text.contains(&crate::text::fold(w)))
        })
        .map(|(d, _)| d.to_string())
        .collect();
    if spec.objectives.is_empty() && spec.unknowns.is_empty() {
        ambiguities.push(Ambiguity {
            kind: AmbiguityKind::NoObjectiveOrUnknown,
            sentence: None,
            text: String::new(),
            message: "aucun objectif ni aucune grandeur à déterminer n'a été reconnu".into(),
        });
    }
    let mappings = mentions
        .iter()
        .filter(|m| m.port.is_some())
        .map(|m| {
            let p = m.port.clone().unwrap();
            let unit = catalog
                .ports
                .iter()
                .find(|x| x.name == p)
                .map(|x| x.unit.clone())
                .unwrap_or_default();
            ConceptMapping {
                sentence: m.sentence,
                phrase: m.phrase.clone(),
                role: m.role,
                port: p,
                port_unit: unit,
                components: m.components.clone(),
                confidence: m.confidence,
                alternatives: m.alternatives.clone(),
                source: "deterministic".into(),
            }
        })
        .collect();
    UnderstandingResult {
        spec,
        mappings,
        ambiguities,
        unsupported_phrases: unsupported,
        quantities: all_q,
    }
}

fn title_of(sents: &[String]) -> String {
    let first = sents.first().cloned().unwrap_or_else(|| "Problème".into());
    if first.chars().count() <= 90 {
        return first;
    }
    let mut t: String = first.chars().take(90).collect();
    if let Some(p) = t.rfind(' ') {
        t.truncate(p);
    }
    t + "…"
}

/// Score candidates, apply component coherence, choose ports.
fn resolve(mentions: &mut [Mention], catalog: &Catalog, lex: &Lexicon) {
    // pass 1: raw scores
    let mut raw: Vec<Vec<(usize, f64)>> = vec![];
    for m in mentions.iter() {
        let mut cands: Vec<(usize, f64)> = vec![];
        let roles: Vec<bool> = match m.role {
            MentionRole::Constraint => vec![false, true],
            MentionRole::Objective | MentionRole::Unknown => vec![false],
            _ => vec![true],
        };
        for (ri, input) in roles.iter().enumerate() {
            for (pi, p) in catalog.ports.iter().enumerate() {
                if p.input != *input {
                    continue;
                }
                if let (Some(d), Some(pd)) = (m.dim, p.dimension) {
                    if d != pd {
                        continue;
                    }
                }
                let s = score_port(lex, p, &m.before, &m.after, &m.counted)
                    * if ri == 0 { 1.0 } else { 0.9 };
                if s > 0.0 {
                    cands.push((pi, s));
                }
            }
        }
        cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        raw.push(cands);
    }
    // pass 2: component support from confident mentions
    let mut support: BTreeMap<String, f64> = BTreeMap::new();
    for cands in &raw {
        let Some(best) = cands.first().map(|c| c.1) else {
            continue;
        };
        if best < MAPPING_THRESHOLD {
            continue;
        }
        let mut comps: Vec<&String> = cands
            .iter()
            .filter(|c| c.1 >= best - 0.05)
            .flat_map(|c| catalog.ports[c.0].components.iter())
            .collect();
        comps.sort();
        comps.dedup();
        let w = 1.0 / comps.len() as f64;
        for c in comps {
            *support.entry(c.clone()).or_default() += w;
        }
    }
    let max_support = support.values().cloned().fold(0.0, f64::max).max(1e-9);
    for (m, cands) in mentions.iter_mut().zip(raw) {
        let mut rescored: Vec<(usize, f64)> = cands
            .iter()
            .map(|(pi, s)| {
                let sup = catalog.ports[*pi]
                    .components
                    .iter()
                    .map(|c| support.get(c).cloned().unwrap_or(0.0))
                    .fold(0.0, f64::max);
                let rs = if *s >= MAPPING_THRESHOLD {
                    s + 0.15 * sup / max_support
                } else {
                    *s
                };
                (*pi, rs)
            })
            .collect();
        rescored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        // merge entries of the same port name (input/output twin) keeping the best
        let mut seen: Vec<String> = vec![];
        rescored.retain(|(pi, _)| {
            let n = catalog.ports[*pi].name.clone();
            if seen.contains(&n) {
                false
            } else {
                seen.push(n);
                true
            }
        });
        if let Some(&(pi, s)) = rescored.first() {
            // ranking uses the uncapped score (coherence bonus); reported confidences are capped at 1
            m.confidence = s.min(1.0);
            m.alternatives = rescored
                .iter()
                .skip(1)
                .take(3)
                .map(|(p, s2)| (catalog.ports[*p].name.clone(), s2.min(1.0)))
                .collect();
            m.margin = rescored.get(1).map(|(_, s2)| s - s2).unwrap_or(1.0);
            if s >= MAPPING_THRESHOLD {
                m.port = Some(catalog.ports[pi].name.clone());
                m.components = catalog.ports[pi].components.clone();
                m.input = catalog.ports[pi].input;
            }
        }
    }
}

fn build_spec(
    spec: &mut ProblemSpec,
    mentions: &mut [Mention],
    catalog: &Catalog,
    amb: &mut Vec<Ambiguity>,
    unsupported: &mut Vec<String>,
) {
    // pronoun constraints: take the previous output mention of the same sentence
    for i in 0..mentions.len() {
        if mentions[i].role == MentionRole::Constraint
            && mentions[i].pronoun
            && mentions[i].confidence < MAPPING_THRESHOLD + 0.2
        {
            let prev = (0..i).rev().find(|&j| {
                mentions[j].sentence == mentions[i].sentence
                    && !mentions[j].input
                    && mentions[j].port.is_some()
                    && matches!(
                        mentions[j].role,
                        MentionRole::Objective | MentionRole::Unknown | MentionRole::Constraint
                    )
            });
            if let Some(j) = prev {
                mentions[i].port = mentions[j].port.clone();
                mentions[i].components = mentions[j].components.clone();
                mentions[i].input = false;
                mentions[i].confidence = mentions[j].confidence * 0.9;
            }
        }
    }
    // canonical unit of each output variable: first unit written in the text
    let mut canonical: BTreeMap<String, String> = BTreeMap::new();
    for m in mentions.iter() {
        if let (Some(p), false, Some(q)) = (&m.port, m.input, &m.quantity) {
            if let Some(u) = &q.unit {
                canonical.entry(p.clone()).or_insert_with(|| u.clone());
            }
        }
    }
    let port_unit = |name: &str, input: bool| {
        catalog
            .port(name, input)
            .or_else(|| catalog.port(name, !input))
            .map(|p| p.unit.clone())
            .unwrap_or_else(|| "1".into())
    };
    let mut last_known_in_sentence: BTreeMap<usize, usize> = BTreeMap::new();
    for m in mentions.iter() {
        let Some(port) = m.port.clone() else {
            let msg = match m.role {
                MentionRole::Objective | MentionRole::Unknown => {
                    "concept non reconnu parmi les sorties des composants disponibles"
                }
                _ => "concept non reconnu parmi les ports des composants disponibles",
            };
            if m.role == MentionRole::Uncertainty {
                // handled below: may attach to the previous known
            } else {
                amb.push(Ambiguity {
                    kind: AmbiguityKind::UnmappedConcept,
                    sentence: Some(m.sentence),
                    text: m.phrase.clone(),
                    message: format!(
                        "{msg} (meilleure confiance {:.2}{})",
                        m.confidence,
                        if m.alternatives.is_empty() {
                            String::new()
                        } else {
                            format!(
                                ", candidats : {}",
                                m.alternatives
                                    .iter()
                                    .map(|a| a.0.clone())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    ),
                });
                unsupported.push(m.phrase.clone());
                continue;
            }
            // uncertainty with no own concept
            if let (Some(&ki), Some(q)) = (last_known_in_sentence.get(&m.sentence), &m.quantity) {
                if let QKind::Range { low, high } = q.kind {
                    let known = &mut spec.knowns[ki];
                    let (lo, hi) = match &q.unit {
                        Some(u) if u != &known.unit => (
                            simorph_units::convert(low, u, &known.unit).unwrap_or(low),
                            simorph_units::convert(high, u, &known.unit).unwrap_or(high),
                        ),
                        _ => (low, high),
                    };
                    known.uncertainty = Uncertainty::Uniform { low: lo, high: hi };
                }
            } else {
                amb.push(Ambiguity {
                    kind: AmbiguityKind::UnmappedConcept,
                    sentence: Some(m.sentence),
                    text: m.phrase.clone(),
                    message: "incertitude sans grandeur associée".into(),
                });
                unsupported.push(m.phrase.clone());
            }
            continue;
        };
        if let Some(second) = m.alternatives.first() {
            if m.margin < 0.03 && second.0 != port {
                amb.push(Ambiguity {
                    kind: AmbiguityKind::MultipleCandidates,
                    sentence: Some(m.sentence),
                    text: m.phrase.clone(),
                    message: format!(
                        "`{port}` retenu ({:.2}) mais `{}` est presque aussi probable ({:.2})",
                        m.confidence, second.0, second.1
                    ),
                });
            }
        }
        if m.confidence < 0.45 {
            amb.push(Ambiguity {
                kind: AmbiguityKind::LowConfidence,
                sentence: Some(m.sentence),
                text: m.phrase.clone(),
                message: format!(
                    "correspondance `{port}` de faible confiance ({:.2})",
                    m.confidence
                ),
            });
        }
        match m.role {
            MentionRole::Known
            | MentionRole::Uncertainty
            | MentionRole::DesignVariable
            | MentionRole::Constraint
                if m.input =>
            {
                let q = m.quantity.as_ref().unwrap();
                let unit = q.unit.clone().unwrap_or_else(|| "1".into());
                let is_design = m.role == MentionRole::DesignVariable;
                if m.role == MentionRole::Constraint {
                    // a bound written on an input
                    if let (Some(dv), QKind::Value { value, .. }) = (
                        spec.design_variables.iter_mut().find(|d| d.name == port),
                        &q.kind,
                    ) {
                        let v = simorph_units::convert(*value, &unit, &dv.unit).unwrap_or(*value);
                        match m.constraint {
                            Some('U') => dv.high = dv.high.min(v),
                            Some('L') => dv.low = dv.low.max(v),
                            _ => {}
                        }
                        continue;
                    }
                    if m.constraint == Some('E') {
                        // "égal à" on an input is a known value
                    } else {
                        amb.push(Ambiguity { kind: AmbiguityKind::ConstraintOnInput, sentence: Some(m.sentence), text: m.phrase.clone(), message: format!("borne sur l'entrée `{port}` qui n'est pas une variable de conception : non utilisée") });
                        continue;
                    }
                }
                if spec.knowns.iter().any(|k| k.name == port)
                    || spec.design_variables.iter().any(|d| d.name == port)
                {
                    if m.role == MentionRole::Uncertainty {
                        if let (Some(k), QKind::Range { low, high }) =
                            (spec.knowns.iter_mut().find(|k| k.name == port), &q.kind)
                        {
                            let lo = simorph_units::convert(*low, &unit, &k.unit).unwrap_or(*low);
                            let hi = simorph_units::convert(*high, &unit, &k.unit).unwrap_or(*high);
                            k.uncertainty = Uncertainty::Uniform { low: lo, high: hi };
                            continue;
                        }
                    }
                    amb.push(Ambiguity {
                        kind: AmbiguityKind::DuplicateQuantity,
                        sentence: Some(m.sentence),
                        text: m.phrase.clone(),
                        message: format!("`{port}` déjà défini : seconde mention ignorée"),
                    });
                    continue;
                }
                match &q.kind {
                    QKind::Range { low, high } if is_design => {
                        spec.design_variables.push(DesignVariable {
                            name: port.clone(),
                            unit,
                            low: *low,
                            high: *high,
                            integer: false,
                            initial: None,
                        })
                    }
                    QKind::Range { low, high } => {
                        amb.push(Ambiguity { kind: AmbiguityKind::NominalFromRange, sentence: Some(m.sentence), text: m.phrase.clone(), message: format!("`{port}` incertain sans valeur nominale : milieu de l'intervalle retenu") });
                        spec.knowns.push(
                            Quantity::new(&port, 0.5 * (low + high), &unit).with_uncertainty(
                                Uncertainty::Uniform {
                                    low: *low,
                                    high: *high,
                                },
                            ),
                        );
                        last_known_in_sentence.insert(m.sentence, spec.knowns.len() - 1);
                    }
                    QKind::Value {
                        value,
                        sd,
                        sd_relative,
                    } => {
                        let unc = match sd {
                            Some(s) if *sd_relative => Uncertainty::Normal {
                                std_dev: s * value.abs(),
                            },
                            Some(s) => Uncertainty::Normal { std_dev: *s },
                            None => Uncertainty::Undeclared,
                        };
                        spec.knowns
                            .push(Quantity::new(&port, *value, &unit).with_uncertainty(unc));
                        last_known_in_sentence.insert(m.sentence, spec.knowns.len() - 1);
                    }
                }
            }
            MentionRole::Objective => {
                if spec.objectives.iter().any(|o| o.variable == port) {
                    continue;
                }
                let unit = canonical
                    .get(&port)
                    .cloned()
                    .unwrap_or_else(|| port_unit(&port, false));
                spec.objectives.push(Objective {
                    name: m.phrase.clone(),
                    direction: m.direction.unwrap_or(Direction::Minimize),
                    variable: port,
                    unit,
                    target: None,
                    weight: 1.0,
                });
            }
            MentionRole::Unknown => {
                if spec.unknowns.iter().any(|u| u.name == port) {
                    continue;
                }
                let unit = canonical
                    .get(&port)
                    .cloned()
                    .unwrap_or_else(|| port_unit(&port, false));
                spec.unknowns.push(Unknown {
                    name: port,
                    unit,
                    prior: None,
                    description: Some(m.phrase.clone()),
                });
            }
            MentionRole::Constraint => {
                let q = m.quantity.as_ref().unwrap();
                let QKind::Value { value, .. } = q.kind else {
                    continue;
                };
                let unit_written = q.unit.clone().unwrap_or_else(|| "1".into());
                let unit = canonical
                    .get(&port)
                    .cloned()
                    .unwrap_or_else(|| unit_written.clone());
                let bound = simorph_units::convert(value, &unit_written, &unit).unwrap_or(value);
                let kind = match m.constraint {
                    Some('L') => ConstraintKind::LowerBound,
                    Some('E') => ConstraintKind::Equality,
                    _ => ConstraintKind::UpperBound,
                };
                spec.constraints.push(Constraint {
                    name: m.phrase.clone(),
                    variable: port,
                    kind,
                    bound,
                    unit,
                    tolerance: 0.0,
                    hard: m.hard,
                });
            }
            _ => {
                // known/design mapped to an output port: not usable as an input
                amb.push(Ambiguity {
                    kind: AmbiguityKind::UnmappedConcept,
                    sentence: Some(m.sentence),
                    text: m.phrase.clone(),
                    message: format!(
                        "`{port}` est une sortie : une valeur ne peut pas lui être imposée"
                    ),
                });
            }
        }
    }
    // Align objective/unknown units with constraint units of the same variable.
    let cons_units: BTreeMap<String, String> = spec
        .constraints
        .iter()
        .map(|c| (c.variable.clone(), c.unit.clone()))
        .collect();
    for o in spec.objectives.iter_mut() {
        if let Some(u) = cons_units.get(&o.variable) {
            o.unit = u.clone();
        }
    }
    for u in spec.unknowns.iter_mut() {
        if let Some(cu) = cons_units.get(&u.name) {
            u.unit = cu.clone();
        }
    }
}
