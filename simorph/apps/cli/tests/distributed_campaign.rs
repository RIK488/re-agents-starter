//! A campaign executed on worker processes gives the same conclusions as a
//! local run, and its runs are really spread over the workers.

use simorph_cli::*;
use simorph_core::*;

#[test]
fn beam_campaign_on_two_workers_matches_local_run() {
    std::env::set_var("SIMORPH_BIN", env!("CARGO_BIN_EXE_simorph"));
    std::env::set_var("SIMORPH_SCIENTIFIC", "0");
    let dir = std::env::temp_dir().join(format!("simorph-dist-{}", RunId::new()));
    let ws = Workspace::open(&dir).unwrap();
    let local = run_campaign(&ws, examples::beam(), &ExecutionControl::default()).unwrap();
    let (remote, metrics) =
        run_campaign_distributed(&ws, examples::beam(), 2, 2, &ExecutionControl::default())
            .unwrap();
    let v = |c: &simorph_svc_scientific_director::Campaign| {
        c.conclusions
            .iter()
            .find(|k| k.variable == "tip_deflection")
            .unwrap()
            .value
    };
    assert!(
        (v(&local) - v(&remote)).abs() <= 1e-12 * v(&local).abs(),
        "{} vs {}",
        v(&local),
        v(&remote)
    );
    assert_eq!(local.stop_reason, remote.stop_reason);
    // Identical requests (reproducibility re-runs, points revisited by the
    // optimiser) are answered once by the coordinator (de-duplication).
    assert!(metrics.completed + metrics.dedup_hits >= 40, "{metrics:?}");
    assert_eq!(metrics.failed, 0);
    assert!(
        metrics.workers.iter().filter(|w| w.completed > 0).count() == 2,
        "both workers must receive tasks: {:?}",
        metrics.workers
    );
    let _ = std::fs::remove_dir_all(dir);
}
