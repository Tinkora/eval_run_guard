# Eval Run Guard Product Specification

## Objective

`eval_run_guard` is a privacy-first offline CLI for detecting structural and accounting defects in agent-evaluation JSONL exports. It helps evaluation authors catch corrupted lines, duplicate or conflicting samples, invalid scores, and summaries that disagree with their declared records before results are compared or published.

## Interface

```text
eval_run_guard audit <INPUT.jsonl> --config <MAPPING.json> [--summary <SUMMARY.json>] [--format text|json|sarif]
```

The mapping is deliberately small and explicit:

```json
{
  "sample_id": "sample.id",
  "status": "result.status",
  "score": "result.score",
  "score_min": 0.0,
  "score_max": 1.0,
  "summary": {
    "total": "counts.total",
    "completed": "counts.completed",
    "errored": "counts.errored",
    "scored": "counts.scored",
    "mean": "score.mean"
  }
}
```

Paths are dot-separated object keys. Array indexing, expressions, coercion, and schema inference are out of scope. `summary` mappings are used only when `--summary` is supplied.

Exit status is `0` when no findings exist, `1` when findings exist, and `2` for invocation or I/O failures.

## Checks

- `ERG001`: malformed, truncated, empty, or over-limit JSONL record.
- `ERG002`: missing or wrongly typed declared field.
- `ERG003`: duplicate sample identifier.
- `ERG004`: conflicting terminal statuses for one sample identifier.
- `ERG005`: non-finite or out-of-range score.
- `ERG006`: declared summary count or arithmetic mean disagrees with recomputed data.
- `ERG007`: duplicate-tracking capacity was reached, so later identities cannot be compared.

Terminal statuses are `completed`, `succeeded`, `failed`, `errored`, and `cancelled` (case-insensitive). Completed means `completed` or `succeeded`; errored means `failed` or `errored`. The arithmetic mean uses finite declared scores only. Duplicate records remain part of input counts but are findings, rather than being silently discarded.

## Privacy and Resource Boundaries

- Read only explicitly named local regular files; reject symlink inputs.
- Stream JSONL with a 16 MiB per-record limit and continue after draining an oversized record. Limit mapping and summary files to 1 MiB, identifiers to 4 KiB, and duplicate tracking to 250,000 samples.
- Never emit sample content, field values, absolute paths, environment variables, or credentials.
- Reports identify only a sanitized input basename, line number, finding code, and fixed explanatory text.
- Make no network requests and never mutate or repair the audited inputs.

## Output

Text is optimized for terminals. JSON is a stable machine-readable report. SARIF 2.1.0 supports GitHub code scanning and uses basename-only artifact locations. Every format represents the same findings and aggregate counters.

## Testing Strategy

Unit tests cover path lookup, terminal classification, mean comparison, and sanitization. Integration tests cover valid input, malformed and oversized records, duplicate/conflicting samples, invalid scores, summary disagreement, output privacy, and symlink rejection. Required checks are `cargo fmt --all -- --check`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## Non-goals

- Running evaluations or judging model quality.
- Inferring framework-specific schemas or metrics.
- Recomputing arbitrary formulas, weighted statistics, confidence intervals, or rankings.
- Repairing, uploading, or storing evaluation data.

## Acceptance Criteria

The CLI deterministically reports every listed defect, reconciles only explicitly declared simple totals and arithmetic mean, produces equivalent privacy-safe text/JSON/SARIF reports, remains bounded on hostile lines, and passes all required checks without network access at runtime.
