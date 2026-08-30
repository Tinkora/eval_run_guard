# eval_run_guard

[中文](README.zh-CN.md) · [Product specification](docs/PRODUCT_SPEC.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

Privacy-first offline integrity checks for agent evaluation JSONL runs.

Evaluation pipelines often finish successfully while leaving malformed records, duplicate sample IDs, conflicting terminal states, invalid scores, or stale aggregate summaries. `eval_run_guard` checks those concrete failure modes without learning a framework-specific schema or exposing evaluation content.

## Install

Download a release archive, or build with Rust 1.85 or newer:

```console
cargo install --path . --locked
```

## Use

Create a small mapping file:

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

Audit a JSONL run, optionally reconciling a separate summary:

```console
eval_run_guard audit run.jsonl --config mapping.json --summary summary.json
eval_run_guard audit run.jsonl --config mapping.json --format json
eval_run_guard audit run.jsonl --config mapping.json --format sarif > results.sarif
```

Exit code `0` means no findings, `1` means findings were reported, and `2` means the input or invocation could not be audited.

## Guarantees and limits

- Reads explicit, stable local regular files only. Symlink rejection is best-effort validation, not a hostile concurrent-filesystem security boundary.
- Streams JSONL with explicit limits for records, fields, tracking memory, and findings.
- Reports fixed explanations, sanitized basenames, and line numbers—not sample IDs, values, content, or absolute paths.
- Recomputes only declared counts and an arithmetic mean.
- Does not infer schemas, run evaluations, repair files, or access the network.

See the [product specification](docs/PRODUCT_SPEC.md) for exact finding codes and semantics.

## Support Tinkora

If this tool saves you time, you can support continued maintenance on [Ko-fi](https://ko-fi.com/tinkora).

## License

MIT
