use eval_run_guard::{audit, render, Mapping, OutputFormat, Severity};
use std::fs;
use tempfile::tempdir;

fn mapping() -> Mapping {
    serde_json::from_str(
        r#"{"sample_id":"sample.id","status":"result.status","score":"result.score","score_min":0.0,"score_max":1.0,"summary":{"total":"counts.total","completed":"counts.completed","errored":"counts.errored","scored":"counts.scored","mean":"score.mean"}}"#,
    )
    .unwrap()
}

#[test]
fn valid_run_reconciles_declared_summary() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("run.jsonl");
    let summary = dir.path().join("summary.json");
    fs::write(&input, "{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"completed\",\"score\":0.5}}\n{\"sample\":{\"id\":\"b\"},\"result\":{\"status\":\"errored\"}}\n").unwrap();
    fs::write(
        &summary,
        r#"{"counts":{"total":2,"completed":1,"errored":1,"scored":1},"score":{"mean":0.5}}"#,
    )
    .unwrap();

    let report = audit(&input, &mapping(), Some(&summary)).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.counts.total, 2);
}

#[test]
fn reports_structural_score_identity_and_summary_defects() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("secret-run.jsonl");
    let summary = dir.path().join("summary.json");
    fs::write(&input, "{bad json\n{\"sample\":{\"id\":\"TOP_SECRET\"},\"result\":{\"status\":\"completed\",\"score\":2.0}}\n{\"sample\":{\"id\":\"TOP_SECRET\"},\"result\":{\"status\":\"failed\",\"score\":\"NaN\"}}\n{\"sample\":{},\"result\":{\"status\":\"completed\"}}\n").unwrap();
    fs::write(
        &summary,
        r#"{"counts":{"total":99,"completed":0,"errored":0,"scored":0},"score":{"mean":0.0}}"#,
    )
    .unwrap();

    let report = audit(&input, &mapping(), Some(&summary)).unwrap();
    for code in ["ERG001", "ERG002", "ERG003", "ERG004", "ERG005", "ERG006"] {
        assert!(
            report.findings.iter().any(|finding| finding.code == code),
            "missing {code}"
        );
    }
    let output = render(&report, OutputFormat::Json).unwrap();
    assert!(!output.contains("TOP_SECRET"));
    assert!(!output.contains(dir.path().to_str().unwrap()));
}

#[test]
fn drains_oversized_record_and_continues_with_next_record() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("large.jsonl");
    let mut bytes = vec![b'x'; 16 * 1024 * 1024 + 1];
    bytes.extend_from_slice(
        b"\n{\"sample\":{\"id\":\"ok\"},\"result\":{\"status\":\"completed\",\"score\":0.25}}\n",
    );
    fs::write(&input, bytes).unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
    assert_eq!(report.counts.total, 2);
    assert_eq!(report.counts.completed, 1);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "ERG001");
}

#[test]
fn compares_terminal_categories_after_non_terminal_updates() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("states.jsonl");
    fs::write(&input, "{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"running\"}}\n{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"completed\"}}\n{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"succeeded\"}}\n{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"failed\"}}\n").unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "ERG003")
            .count(),
        3
    );
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "ERG004")
            .count(),
        1
    );
}

#[test]
fn json_and_sarif_share_finding_count_without_content() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("run\nprivate.jsonl");
    fs::write(&input, "{bad\n").unwrap();
    let report = audit(&input, &mapping(), None).unwrap();
    assert_eq!(report.findings[0].severity, Severity::Error);
    let json = render(&report, OutputFormat::Json).unwrap();
    let sarif = render(&report, OutputFormat::Sarif).unwrap();
    assert!(json.contains("ERG001"));
    assert!(sarif.contains("2.1.0"));
    assert!(!sarif.contains("{bad"));
    assert!(sarif.contains("run_private.jsonl"));
    assert!(!sarif.contains("run\\nprivate.jsonl"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_inputs() {
    use std::os::unix::fs::symlink;
    let dir = tempdir().unwrap();
    let real = dir.path().join("real.jsonl");
    let link = dir.path().join("link.jsonl");
    fs::write(&real, "{}\n").unwrap();
    symlink(&real, &link).unwrap();
    assert!(audit(&link, &mapping(), None).is_err());
}
