# N1a 合同与候选分支接收

> **性质**：实际接收与验证记录 · **状态**：current · **读取时机**：核对2026-10-10合同接入与历史证据边界时 · **唯一真源**：本页与所链接的实际收据

## 接收身份与边界

- 接收工作分支：`fix/spec-0290`，文档开始基线 `c91fdcb`。
- 0289 文档来源：`970e90a116bbaac9893e4d842845d2e6f9306988`；补齐已批准的 Guide v0.43、ADR-0030、active 0289 与架构事实。
- 0291 独立草案来源：`9669d3d0e120a09158ed0aa4555587f93dbac148`，不包含0289 Guide；只接收为 v0.44 draft。
- 接收时 active 为0289/0290；旧立即消费0290改为全局空闲0292并更新引用，仍在 v0.43 准备目录保持未启用。
- consume、Clone、N1b 与任意用户扩展未启用；0291历史批准引用不构成当前语义授权。

## v0.42 完整保全

从实际 main `ef60f2fcd07f07dc92d8a204a9c7dd2e40494c3c` 的 `docs/guide/`
提取16页，存入 archive，仅重算 Markdown 相对链接。
[逐页 SHA-256 与行数收据](evidence/range-carrier-0289/v042-main-integration-snapshot.json)
记录真实 Git 来源、目标字节和接收基线。既有快照与本次重建字节一致，未丢失原文。
旧 `v042-snapshot.json` 与0289各阶段 `/tmp` 日志仅保留历史记载；本次未核验、未伪造，
不能作为当前分支复跑成功的证据。

## 本次验证

- `python3 scripts/check_docs.py`：608 Markdown，结构门禁通过；不证明语义等价。
- `python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py'`：37 passed。
- DAG 新增跨 `PYTHONHASHSEED=0..15` 的四份产物字节一致性回归：旧排序实际失败，
  修复后 1 passed；[红日志](evidence/range-carrier-0289/dag-red.log)与
  [绿日志](evidence/range-carrier-0289/dag-green.log)保全同一16-seed选择。早期4-seed
  选择未触发问题，未将它记作红测。原实现按分区rank排序，同rank的v0.43/v0.44
  保留set随机顺序；修复仅添加分区名作为稳定次序，不改依赖或验收语义。
- `python3 scripts/gen_spec_dag.py`：当前4个live节点与275个archive，完整279份。
- `git diff --check`：通过。
不运行 Rust/Cargo、native、故障注入或校准；不改变CI、生产Rust或consume能力。
规范接收不表示0289实现完成、双宿主通过或PR/merge交付完成。
