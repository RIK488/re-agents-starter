//! Final session report.

use crate::context::AgentContext;
use crate::message::*;
use crate::supervisor::SessionEnd;
use simorph_security::AuditLog;

pub fn render(
    ctx: &AgentContext,
    trace: &[AgentMessage],
    end: &SessionEnd,
    audit: &AuditLog,
) -> String {
    let b = &ctx.board;
    let mut s = String::from("# Rapport de session multi-agents SIMORPH\n\n");
    s.push_str(&format!("**Issue** : {end:?}\n\n"));
    if let Some(t) = &b.text {
        s.push_str(&format!("## Énoncé\n\n> {}\n\n", t.replace('\n', "\n> ")));
    }
    if let Some(u) = &b.understanding {
        s.push_str(&u.to_markdown());
        s.push('\n');
    }
    if let Some(p) = &b.plan_markdown {
        s.push_str("## Planification (agent Planner)\n\n");
        s.push_str(p);
        s.push('\n');
    }
    if let Some(m) = &b.assembly_markdown {
        s.push_str("## Modélisation (agent Modeler)\n\n");
        s.push_str(m);
        s.push('\n');
    }
    if let Some(g) = &b.generation {
        s.push_str("## Simulateur généré\n\n");
        s.push_str(&g.markdown());
        s.push('\n');
    }
    for (i, c) in b.campaigns.iter().enumerate() {
        s.push_str(&format!(
            "## Campagne {i} — {}\n\n",
            b.campaign_reasons.get(i).cloned().unwrap_or_default()
        ));
        s.push_str("| Variable | Valeur | Unité | Niveau |\n|---|---|---|---|\n");
        for k in &c.conclusions {
            s.push_str(&format!(
                "| {} | {:.6e} | {} | {} |\n",
                k.variable,
                k.value,
                k.unit,
                k.level.label_fr()
            ));
        }
        if let Some(v) = ctx.verdicts().iter().find(|v| v.campaign == i) {
            s.push_str(&format!("\n**Verdict du validateur indépendant** : {}\n\n| Contrôle | Résultat | Détails |\n|---|---|---|\n", if v.passed { "validé" } else { "REJETÉ" }));
            for ch in &v.checks {
                s.push_str(&format!(
                    "| {} | {} | {} |\n",
                    ch.name,
                    if ch.passed { "✓" } else { "✗" },
                    ch.details.replace('|', "\\|")
                ));
            }
        }
        if let Some(cr) = b.critiques.iter().find(|x| x.campaign == i) {
            s.push_str("\n**Critique**\n\n");
            for f in &cr.findings {
                s.push_str(&format!("- [{}] {}\n", f.kind, f.message));
            }
            if let Some(r) = &cr.proposal_rationale {
                s.push_str(&format!("- **Reformulation exécutée** : {r}\n"));
            }
        }
        s.push('\n');
    }
    if let Some((n, e, p)) = &b.archive {
        s.push_str(&format!("## Mémoire scientifique\n\n{n} nœuds et {e} arêtes ajoutés (`{p}`, synthèse `memory.md`).\n\n"));
    }
    if let Some(r) = &b.blocked {
        s.push_str(&format!("## Blocage\n\n{r}\n\n"));
    }
    let refused: Vec<_> = audit.entries().into_iter().filter(|e| !e.granted).collect();
    if !refused.is_empty() {
        s.push_str("## Actions refusées (journal d'audit)\n\n");
        for e in refused {
            s.push_str(&format!("- `{}` : {}\n", e.principal, e.reason));
        }
        s.push('\n');
    }
    s.push_str("## Trace des messages\n\n| # | De | Vers | Message |\n|---|---|---|---|\n");
    for m in trace {
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            m.seq,
            m.from.name(),
            m.to.name(),
            m.body.tag()
        ));
    }
    s
}
