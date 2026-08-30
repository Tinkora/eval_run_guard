# eval_run_guard

[English](README.md) · [产品规格](docs/PRODUCT_SPEC.zh-CN.md) · [贡献指南](CONTRIBUTING.zh-CN.md) · [安全策略](SECURITY.zh-CN.md)

一个隐私优先、完全离线的 Agent 评测 JSONL 完整性检查工具。

评测流水线可能正常结束，但留下损坏记录、重复样本 ID、互相冲突的终态、非法分数或过期的汇总数据。`eval_run_guard` 检查这些明确的失败模式，不猜测框架 schema，也不暴露评测内容。

## 安装

下载 Release 归档，或使用 Rust 1.85 及以上版本构建：

```console
cargo install --path . --locked
```

## 使用

创建一个小型映射文件：

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

审计 JSONL，并可选择与独立汇总文件对账：

```console
eval_run_guard audit run.jsonl --config mapping.json --summary summary.json
eval_run_guard audit run.jsonl --config mapping.json --format json
eval_run_guard audit run.jsonl --config mapping.json --format sarif > results.sarif
```

退出码 `0` 表示无发现，`1` 表示发现问题，`2` 表示输入或调用无法完成审计。

## 保证与边界

- 只读取明确指定且稳定的本地普通文件。符号链接拒绝是尽力而为的验证，不是针对恶意并发文件系统的安全边界。
- 流式读取 JSONL，并显式限制记录、字段、跟踪内存和发现数量。
- 只报告固定说明、清理后的文件名和行号，不输出样本 ID、字段值、内容或绝对路径。
- 只重算声明的计数与算术平均值。
- 不猜测 schema、不运行评测、不修复文件，也不访问网络。

准确的发现代码和语义请参阅[产品规格](docs/PRODUCT_SPEC.zh-CN.md)。

## 支持 Tinkora

如果这个工具为你节省了时间，可以通过 [Ko-fi](https://ko-fi.com/tinkora) 支持持续维护。

## 许可证

MIT
