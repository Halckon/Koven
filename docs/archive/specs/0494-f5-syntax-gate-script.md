# SPEC-0494：F5 表面语法增量门禁脚本化校验与单测

> **性质**：治理工具交付合同 · **状态**：done · **读取时机**：自动化核对 F5 表面语法提案/模板完整性时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 表面语法增量门禁（F5） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0493-f4b-difftest-ci@064f6092` |
| 分支 | `feature/spec-0494-f5-syntax-gate-checker` |

本 Spec 承接 [SPEC-0480](../../specs/active/0480-surface-syntax-incremental-gate.md) 与 [F5 门禁清单](../../proposals/parser-formal-verification/syntax-gate-checklist.md)，
将 F5 的 6 项必填字段与模板完整性要求脚本化为零外部依赖的 Python 校验工具 [scripts/check_syntax_gate.py](../../../scripts/check_syntax_gate.py) 与单元测试 [scripts/tests/test_check_syntax_gate.py](../../../scripts/tests/test_check_syntax_gate.py)（由既有 `docs` CI 的 `unittest discover -s scripts/tests` 自动执行）。
不修改生产代码、Guide 或主 `ci.yml`。

## 交付合同

1. **模板完整性自检（`--self-check-template`）**：核对 `docs/proposals/README.md`、`docs/specs/TEMPLATE.md` 与 `docs/proposals/parser-formal-verification/syntax-gate-checklist.md` 持续保留 SPEC-0480 规定的 6 项必填字段与门禁章节。
2. **提案/Spec 文件校验（`--check-file`）**：对含 `表面语法...门禁` 章节（或 `--strict`）的 Markdown 文件逐项检查 6 项必填字段并拒绝 `TBD`/`TODO`/`FIXME`/`[待填]`/`[待补充]` 等未决占位符，同时允许非语法 Spec 显式标注 `不适用`/`N/A`。
3. **正负对照单测**：[scripts/tests/test_check_syntax_gate.py](../../../scripts/tests/test_check_syntax_gate.py) 覆盖仓内实时模板自检、完整提案通过、`N/A` 放行、逐项缺字段拒收、占位符拒收与 CLI 往返。

## 检查点

- [x] `scripts/check_syntax_gate.py` 与 `scripts/tests/test_check_syntax_gate.py`（9/9）落地且全绿通过
- [x] `python3 scripts/check_docs.py` 与 `python3 scripts/gen_spec_dag.py` 通过
- [x] PR #461（`4d753908`）双宿主主 `CI`（run `45385947066`，含 `Docs & Spec Structural Gate`）全绿验收并归档
