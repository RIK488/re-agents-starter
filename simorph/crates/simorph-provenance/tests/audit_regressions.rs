//! Regression tests for defects found by the engine audit.

use simorph_provenance::*;
use std::io::Write;
use std::time::Instant;

const KEY: &[u8] = b"test-ledger-key-kept-outside-the-ledger";

fn lines(p: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(p).unwrap().lines().map(String::from).collect()
}

fn write_lines(p: &std::path::Path, l: &[String]) {
    let mut f = std::fs::File::create(p).unwrap();
    for x in l {
        writeln!(f, "{x}").unwrap();
    }
}

/// Edit a record's label and recompute every hash of the chain from there,
/// as an attacker with write access (but no key) would.
fn edit_and_rehash(p: &std::path::Path, index: usize) {
    let mut recs: Vec<ProvenanceRecord> =
        lines(p).iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    recs[index].label = "forged".into();
    for i in index..recs.len() {
        if i > 0 {
            recs[i].previous_hash = Some(recs[i - 1].content_hash.clone());
        }
        recs[i].content_hash = recs[i].compute_hash();
    }
    write_lines(p, &recs.iter().map(|r| serde_json::to_string(r).unwrap()).collect::<Vec<_>>());
}

fn build(path: &std::path::Path, keyed: bool, n: usize) {
    let store = if keyed {
        JsonlStore::open_keyed(path, KEY).unwrap()
    } else {
        JsonlStore::open(path).unwrap()
    }
    .with_sync(false);
    let l = if keyed { Ledger::with_key(Box::new(store), KEY) } else { Ledger::new(Box::new(store)) };
    for i in 0..n {
        l.append(ProvenanceKind::Run, format!("run {i}"), serde_json::json!({ "i": i }), &[]).unwrap();
    }
    l.verify_chain().unwrap();
}

#[test]
fn audit_unkeyed_ledger_cannot_detect_edit_and_rehash_but_keyed_can() {
    let dir = tempfile::tempdir().unwrap();
    // unkeyed: documented limit — a consistent re-hash is accepted
    let p = dir.path().join("plain.jsonl");
    build(&p, false, 5);
    std::fs::remove_file(JsonlStore::head_path(&p)).unwrap();
    edit_and_rehash(&p, 2);
    assert!(JsonlStore::open(&p).is_ok(), "unkeyed mode only detects accidental edits");
    // keyed: the forged records have no valid MAC
    let p = dir.path().join("keyed.jsonl");
    build(&p, true, 5);
    edit_and_rehash(&p, 2);
    let e = JsonlStore::open_keyed(&p, KEY).unwrap_err().to_string();
    assert!(e.contains("MAC"), "{e}");
    // the wrong key is refused too
    let p2 = dir.path().join("keyed2.jsonl");
    build(&p2, true, 3);
    assert!(JsonlStore::open_keyed(&p2, b"another key").is_err());
    assert!(JsonlStore::open_keyed(&p2, KEY).is_ok());
}

#[test]
fn audit_truncation_is_detected_by_the_head_anchor() {
    let dir = tempfile::tempdir().unwrap();
    for keyed in [false, true] {
        let p = dir.path().join(format!("t{keyed}.jsonl"));
        build(&p, keyed, 6);
        let l = lines(&p);
        write_lines(&p, &l[..4]); // drop the tail: the remaining chain is valid
        let r = if keyed { JsonlStore::open_keyed(&p, KEY) } else { JsonlStore::open(&p) };
        let e = r.unwrap_err().to_string();
        assert!(e.contains("truncated"), "keyed={keyed}: {e}");
    }
    // keyed: deleting the anchor is detected as well
    let p = dir.path().join("nohead.jsonl");
    build(&p, true, 2);
    std::fs::remove_file(JsonlStore::head_path(&p)).unwrap();
    assert!(JsonlStore::open_keyed(&p, KEY).is_err());
}

#[test]
fn audit_sequence_numbers_are_checked_on_load() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("seq.jsonl");
    build(&p, false, 3);
    std::fs::remove_file(JsonlStore::head_path(&p)).unwrap();
    let mut recs: Vec<ProvenanceRecord> =
        lines(&p).iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    // renumber consistently (hashes recomputed): chain linkage still holds
    recs[1].sequence = 7;
    recs[1].content_hash = recs[1].compute_hash();
    recs[2].previous_hash = Some(recs[1].content_hash.clone());
    recs[2].content_hash = recs[2].compute_hash();
    write_lines(&p, &recs.iter().map(|r| serde_json::to_string(r).unwrap()).collect::<Vec<_>>());
    let e = JsonlStore::open(&p).unwrap_err().to_string();
    assert!(e.contains("sequence"), "{e}");
}

#[test]
fn audit_append_is_constant_time() {
    // O(n²) before: `last()` cloned the whole store on every append
    let l = Ledger::in_memory();
    let t = Instant::now();
    for i in 0..10_000 {
        l.append(ProvenanceKind::Run, "r", serde_json::json!(i), &[]).unwrap();
    }
    let mem = t.elapsed();
    assert!(mem.as_secs_f64() < 2.0, "10 000 in-memory appends took {mem:?}");
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("big.jsonl");
    let t = Instant::now();
    build(&p, true, 10_000);
    let file = t.elapsed();
    assert!(file.as_secs_f64() < 6.0, "10 000 keyed JSONL appends (no fsync) took {file:?}");
    let t = Instant::now();
    let s = JsonlStore::open_keyed(&p, KEY).unwrap();
    assert_eq!(s.len(), 10_000);
    eprintln!("10k appends: memory {mem:?}, keyed jsonl {file:?}; reload {:?}", t.elapsed());
}

#[test]
fn audit_environment_records_build_compiler_and_caller() {
    let e = EnvironmentCapture::capture_for("my-app", "1.2.3");
    assert!(e.build_rustc_version.as_deref().unwrap_or("").starts_with("rustc "), "{e:?}");
    assert_eq!(e.caller, Some(("my-app".into(), "1.2.3".into())));
    // old records without the new fields still deserialise
    let mut v = serde_json::to_value(&e).unwrap();
    for k in ["build_rustc_version", "build_profile", "caller"] {
        v.as_object_mut().unwrap().remove(k);
    }
    let back: EnvironmentCapture = serde_json::from_value(v).unwrap();
    assert!(back.build_rustc_version.is_none());
}
