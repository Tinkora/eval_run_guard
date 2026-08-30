# Eval Run Guard 产品规格

## 目标

`eval_run_guard` 是一个隐私优先、完全离线的 CLI，用于发现 Agent 评测 JSONL 导出中的结构和对账缺陷。它帮助评测作者在比较或发布结果前发现损坏行、重复或冲突样本、非法分数，以及与记录不一致的汇总数据。

## 接口

```text
eval_run_guard audit <INPUT.jsonl> --config <MAPPING.json> [--summary <SUMMARY.json>] [--format text|json|sarif]
```

映射配置刻意保持小而明确：

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

路径是以点分隔的对象键。不支持数组索引、表达式、类型强制转换和 schema 猜测。只有同时提供 `--summary` 时才使用 `summary` 映射。

无发现时退出码为 `0`，存在发现时为 `1`，调用或 I/O 失败时为 `2`。

## 检查项

- `ERG001`：格式错误、截断、空白或超过上限的 JSONL 记录。
- `ERG002`：声明字段缺失或类型错误。
- `ERG003`：样本标识符重复。
- `ERG004`：同一样本标识符出现互相冲突的终态。
- `ERG005`：非有限值或超出范围的分数。
- `ERG006`：声明的汇总计数或算术平均值与重算结果不一致。
- `ERG007`：审计容量已满，后续发现或标识符将被省略。

终态为 `completed`、`succeeded`、`failed`、`errored` 和 `cancelled`（不区分大小写）。`completed` 与 `succeeded` 计入完成，`failed` 与 `errored` 计入错误。算术平均值只使用有限的已声明分数。重复记录仍计入输入计数并产生发现，不会被静默丢弃。

## 隐私和资源边界

- 只读取明确指定的本地普通文件。符号链接拒绝只是尽力而为的输入验证，不能防御被恶意并发修改的文件系统；调用方必须提供稳定的本地文件。
- 流式读取 JSONL，单条上限 16 MiB；超限后排空该记录并继续。映射和汇总文件上限为 1 MiB，标识符上限为 1 KiB，状态上限为 128 字节，重复跟踪存储上限为 16 MiB，发现上限为 10,000 条。
- 不输出样本内容、字段值、绝对路径、环境变量或凭据。
- 报告只包含清理后的输入文件名、行号、发现代码和固定说明。
- 不发起网络请求，也不修改或修复输入。

## 输出

文本格式适合终端，JSON 是稳定的机器可读报告，SARIF 2.1.0 可用于 GitHub code scanning，并只使用文件名作为位置。三种格式表达相同的发现和汇总计数。

## 测试策略

单元测试覆盖路径查找、终态分类、均值比较和名称清理。集成测试覆盖正常输入、损坏及超大记录、重复/冲突样本、非法分数、汇总不一致、输出隐私和符号链接拒绝。必须通过 `cargo fmt --all -- --check`、`cargo test --workspace --locked` 和 `cargo clippy --workspace --all-targets --locked -- -D warnings`。

## 非目标

- 运行评测或判断模型质量。
- 猜测特定框架的 schema 或指标。
- 重算任意公式、加权统计、置信区间或排名。
- 修复、上传或保存评测数据。

## 验收条件

CLI 能确定性报告上述缺陷，只对明确声明的简单计数和算术平均值对账，生成等价且保护隐私的 text/JSON/SARIF 报告，在恶意超长行下保持资源边界，并且运行时无需网络即可通过所有质量检查。
