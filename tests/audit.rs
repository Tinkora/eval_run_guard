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
fn non_terminal_update_does_not_erase_terminal_outcome() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("states.jsonl");
    fs::write(&input, "{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"completed\"}}\n{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"running\"}}\n{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"failed\"}}\n").unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
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
fn caps_findings_and_emits_one_saturation_notice() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("many-bad-lines.jsonl");
    fs::write(&input, "{bad\n".repeat(10_100)).unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
    assert!(report.findings.len() <= 10_000);
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "ERG007")
            .count(),
        1
    );
}

#[test]
fn rejects_oversized_status_without_echoing_it() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("status.jsonl");
    let secret = "S".repeat(129);
    fs::write(
        &input,
        format!("{{\"sample\":{{\"id\":\"a\"}},\"result\":{{\"status\":\"{secret}\"}}}}\n"),
    )
    .unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
    assert!(report.findings.iter().any(|f| f.code == "ERG002"));
    assert!(!render(&report, OutputFormat::Json)
        .unwrap()
        .contains(&secret));
}

#[test]
fn caps_duplicate_tracking_by_estimated_bytes() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("many-identities.jsonl");
    let suffix = "x".repeat(980);
    let mut records = String::new();
    for index in 0..17_000 {
        records.push_str(&format!("{{\"sample\":{{\"id\":\"{index:05}{suffix}\"}},\"result\":{{\"status\":\"running\"}}}}\n"));
    }
    fs::write(&input, records).unwrap();

    let report = audit(&input, &mapping(), None).unwrap();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "ERG007")
            .count(),
        1
    );
}

#[test]
fn online_mean_stays_finite_for_extreme_finite_scores() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("scores.jsonl");
    let mut unrestricted = mapping();
    unrestricted.score_min = None;
    unrestricted.score_max = None;
    fs::write(&input, "{\"sample\":{\"id\":\"a\"},\"result\":{\"status\":\"completed\",\"score\":1.7e308}}\n{\"sample\":{\"id\":\"b\"},\"result\":{\"status\":\"completed\",\"score\":1.7e308}}\n").unwrap();

    let report = audit(&input, &unrestricted, None).unwrap();
    assert!(report.mean.unwrap().is_finite());
    assert!(!report.findings.iter().any(|f| f.code == "ERG005"));
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
