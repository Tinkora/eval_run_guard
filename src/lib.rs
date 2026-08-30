use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const MAX_AUXILIARY_BYTES: u64 = 1024 * 1024;
const MAX_TRACKED_SAMPLES: usize = 250_000;
const MAX_IDENTIFIER_BYTES: usize = 4096;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub sample_id: String,
    pub status: String,
    pub score: String,
    #[serde(default)]
    pub score_min: Option<f64>,
    #[serde(default)]
    pub score_max: Option<f64>,
    #[serde(default)]
    pub summary: Option<SummaryMapping>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummaryMapping {
    pub total: Option<String>,
    pub completed: Option<String>,
    pub errored: Option<String>,
    pub scored: Option<String>,
    pub mean: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub line: Option<u64>,
    pub message: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub total: u64,
    pub completed: u64,
    pub errored: u64,
    pub scored: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub input: String,
    pub counts: Counts,
    pub mean: Option<f64>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Sarif,
}

pub fn load_mapping(path: &Path) -> Result<Mapping> {
    validate_regular_file(path, "mapping")?;
    validate_file_size(path, "mapping", MAX_AUXILIARY_BYTES)?;
    let bytes = fs::read(path).context("could not read mapping file")?;
    let mapping: Mapping = serde_json::from_slice(&bytes).context("mapping is not valid JSON")?;
    validate_mapping(&mapping)?;
    Ok(mapping)
}

pub fn audit(input: &Path, mapping: &Mapping, summary: Option<&Path>) -> Result<Report> {
    validate_mapping(mapping)?;
    validate_regular_file(input, "input")?;
    let file = File::open(input).context("could not open input file")?;
    let mut reader = BufReader::new(file);
    let mut report = Report {
        input: safe_basename(input),
        counts: Counts::default(),
        mean: None,
        findings: Vec::new(),
    };
    let mut line = 0_u64;
    let mut states: HashMap<String, String> = HashMap::new();
    let mut score_sum = 0.0;

    loop {
        line = line.saturating_add(1);
        let Some(record) = read_bounded_line(&mut reader)? else {
            break;
        };
        report.counts.total = report.counts.total.saturating_add(1);
        let bytes = match record {
            BoundedLine::Complete(bytes) if !bytes.iter().all(u8::is_ascii_whitespace) => bytes,
            BoundedLine::Complete(_) | BoundedLine::Oversized => {
                finding(
                    &mut report,
                    "ERG001",
                    Severity::Error,
                    Some(line),
                    "JSONL record is malformed, empty, truncated, or too large",
                );
                continue;
            }
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(Value::Object(map)) => Value::Object(map),
            _ => {
                finding(
                    &mut report,
                    "ERG001",
                    Severity::Error,
                    Some(line),
                    "JSONL record is malformed, empty, truncated, or too large",
                );
                continue;
            }
        };
        inspect_record(
            &value,
            mapping,
            line,
            &mut report,
            &mut states,
            &mut score_sum,
        );
    }
    if report.counts.scored > 0 {
        report.mean = Some(score_sum / report.counts.scored as f64);
    }
    if let Some(summary_path) = summary {
        reconcile_summary(summary_path, mapping, &mut report)?;
    }
    Ok(report)
}

fn inspect_record(
    value: &Value,
    mapping: &Mapping,
    line: u64,
    report: &mut Report,
    states: &mut HashMap<String, String>,
    score_sum: &mut f64,
) {
    let id = lookup(value, &mapping.sample_id)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= MAX_IDENTIFIER_BYTES);
    let status = lookup(value, &mapping.status)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    if id.is_none() || status.is_none() {
        finding(
            report,
            "ERG002",
            Severity::Error,
            Some(line),
            "A declared field is missing or has the wrong type",
        );
    }
    if let (Some(id), Some(status)) = (id, status) {
        let normalized = status.to_ascii_lowercase();
        match normalized.as_str() {
            "completed" | "succeeded" => {
                report.counts.completed = report.counts.completed.saturating_add(1)
            }
            "failed" | "errored" => report.counts.errored = report.counts.errored.saturating_add(1),
            _ => {}
        }
        if let Some(previous) = states.get_mut(id) {
            finding(
                report,
                "ERG003",
                Severity::Warning,
                Some(line),
                "Sample identifier is duplicated",
            );
            if let (Some(previous_category), Some(current_category)) =
                (terminal_category(previous), terminal_category(&normalized))
            {
                if previous_category != current_category {
                    finding(
                        report,
                        "ERG004",
                        Severity::Error,
                        Some(line),
                        "A sample has conflicting terminal statuses",
                    );
                }
            }
            *previous = normalized;
        } else if states.len() < MAX_TRACKED_SAMPLES {
            states.insert(id.to_owned(), normalized);
        } else if !report.findings.iter().any(|item| item.code == "ERG007") {
            finding(
                report,
                "ERG007",
                Severity::Warning,
                Some(line),
                "Duplicate tracking capacity was reached; later identities are not compared",
            );
        }
    }
    match lookup(value, &mapping.score) {
        None | Some(Value::Null) => {}
        Some(score_value) => match score_value.as_f64() {
            Some(score)
                if score.is_finite()
                    && mapping.score_min.is_none_or(|min| score >= min)
                    && mapping.score_max.is_none_or(|max| score <= max) =>
            {
                report.counts.scored = report.counts.scored.saturating_add(1);
                *score_sum += score;
            }
            _ => finding(
                report,
                "ERG005",
                Severity::Error,
                Some(line),
                "Score is non-finite, wrongly typed, or outside its declared range",
            ),
        },
    }
}

fn reconcile_summary(path: &Path, mapping: &Mapping, report: &mut Report) -> Result<()> {
    validate_regular_file(path, "summary")?;
    validate_file_size(path, "summary", MAX_AUXILIARY_BYTES)?;
    let bytes = fs::read(path).context("could not read summary file")?;
    let value: Value = serde_json::from_slice(&bytes).context("summary is not valid JSON")?;
    let Some(paths) = &mapping.summary else {
        bail!("--summary requires a summary mapping in the config");
    };
    let mut mismatch = false;
    for (path, actual) in [
        (&paths.total, report.counts.total),
        (&paths.completed, report.counts.completed),
        (&paths.errored, report.counts.errored),
        (&paths.scored, report.counts.scored),
    ] {
        if let Some(path) = path {
            mismatch |= lookup(&value, path).and_then(Value::as_u64) != Some(actual);
        }
    }
    if let Some(path) = &paths.mean {
        let declared = lookup(&value, path)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite());
        mismatch |= match (declared, report.mean) {
            (None, None) => false,
            (Some(a), Some(b)) => (a - b).abs() > 1e-12_f64.max(b.abs() * 1e-9),
            _ => true,
        };
    }
    if mismatch {
        finding(
            report,
            "ERG006",
            Severity::Error,
            None,
            "Declared summary disagrees with recomputed counts or arithmetic mean",
        );
    }
    Ok(())
}

pub fn render(report: &Report, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Text => {
            let mut out = format!(
                "eval_run_guard: {}\nrecords: {}  completed: {}  errored: {}  scored: {}\n",
                report.input,
                report.counts.total,
                report.counts.completed,
                report.counts.errored,
                report.counts.scored
            );
            for item in &report.findings {
                let location = item.line.map(|n| format!(" line {n}")).unwrap_or_default();
                out.push_str(&format!(
                    "{} {:?}{}: {}\n",
                    item.code, item.severity, location, item.message
                ));
            }
            Ok(out)
        }
        OutputFormat::Sarif => {
            let results: Vec<Value> = report.findings.iter().map(|item| json!({
                "ruleId": item.code,
                "level": if item.severity == Severity::Error { "error" } else { "warning" },
                "message": { "text": item.message },
                "locations": item.line.map(|line| vec![json!({"physicalLocation":{"artifactLocation":{"uri":report.input},"region":{"startLine":line}}})]).unwrap_or_default()
            })).collect();
            Ok(serde_json::to_string_pretty(
                &json!({"version":"2.1.0","$schema":"https://json.schemastore.org/sarif-2.1.0.json","runs":[{"tool":{"driver":{"name":"eval_run_guard","rules":[]}},"results":results}]}),
            )?)
        }
    }
}

fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(root, |value, key| value.as_object()?.get(key))
}

fn validate_mapping(mapping: &Mapping) -> Result<()> {
    for path in [&mapping.sample_id, &mapping.status, &mapping.score] {
        if path.is_empty() || path.split('.').any(str::is_empty) {
            bail!("mapping paths must contain non-empty dot-separated object keys");
        }
    }
    if mapping.score_min.is_some_and(|v| !v.is_finite())
        || mapping.score_max.is_some_and(|v| !v.is_finite())
        || matches!((mapping.score_min, mapping.score_max), (Some(min), Some(max)) if min > max)
    {
        bail!("score range must be finite and ordered");
    }
    Ok(())
}

fn validate_regular_file(path: &Path, label: &str) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("could not inspect {label} file"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("{label} must be a regular non-symlink file");
    }
    Ok(())
}

fn validate_file_size(path: &Path, label: &str, maximum: u64) -> Result<()> {
    if fs::metadata(path)
        .with_context(|| format!("could not inspect {label} file size"))?
        .len()
        > maximum
    {
        bail!("{label} file exceeds the supported size limit");
    }
    Ok(())
}

fn safe_basename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("input.jsonl")
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn terminal_category(status: &str) -> Option<u8> {
    match status {
        "completed" | "succeeded" => Some(0),
        "failed" | "errored" => Some(1),
        "cancelled" => Some(2),
        _ => None,
    }
}

fn finding(
    report: &mut Report,
    code: &'static str,
    severity: Severity,
    line: Option<u64>,
    message: &'static str,
) {
    report.findings.push(Finding {
        code,
        severity,
        line,
        message,
    });
}

enum BoundedLine {
    Complete(Vec<u8>),
    Oversized,
}

fn read_bounded_line(reader: &mut impl BufRead) -> Result<Option<BoundedLine>> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf().context("could not read JSONL input")?;
        if available.is_empty() {
            return if bytes.is_empty() && !oversized {
                Ok(None)
            } else if oversized {
                Ok(Some(BoundedLine::Oversized))
            } else {
                Ok(Some(BoundedLine::Complete(bytes)))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !oversized {
            if bytes.len().saturating_add(consumed) > MAX_RECORD_BYTES {
                oversized = true;
                bytes.clear();
            } else {
                bytes.extend_from_slice(&available[..consumed]);
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            if !oversized && bytes.last() == Some(&b'\n') {
                bytes.pop();
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
            }
            return Ok(Some(if oversized {
                BoundedLine::Oversized
            } else {
                BoundedLine::Complete(bytes)
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_lookup_does_not_index_arrays() {
        let v = json!({"a":{"b":3},"items":[1]});
        assert_eq!(lookup(&v, "a.b"), Some(&json!(3)));
        assert!(lookup(&v, "items.0").is_none());
    }
    #[test]
    fn sanitizes_untrusted_basename() {
        assert_eq!(
            safe_basename(Path::new("bad\nname.jsonl")),
            "bad_name.jsonl"
        );
    }
}
