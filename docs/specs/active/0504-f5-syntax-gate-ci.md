# SPEC-0504：F5 表面语法增量门禁路径触发轻量 CI 接入

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F5 表面语法增量门禁自动化 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 表面语法增量门禁（F5） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0502-f6-stops-lattice-proof@7ff523d8` |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [SPEC-0480](0480-surface-syntax-incremental-gate.md) 与 [SPEC-0494](../../archive/specs/0494-f5-syntax-gate-script.md)，
新增独立路径触发轻量 workflow [.github/workflows/parser-formal-f5.yml](../../../.github/workflows/parser-formal-f5.yml)，
将 `scripts/check_syntax_gate.py --self-check-template`、`scripts/tests/test_check_syntax_gate.py` 单测与 PR 变更提案/Spec 增量门禁校验接入 GitHub Actions（先 advisory，对无关 PR 零开销）。
不修改生产 Rust 代码、主 `ci.yml` 或语言规范。

## 1. 交付合同

1. **路径触发轻量 CI**：[.github/workflows/parser-formal-f5.yml](../../../.github/workflows/parser-formal-f5.yml) 仅在 `docs/proposals/**`、`docs/specs/TEMPLATE.md`、`docs/specs/active/**`、`scripts/check_syntax_gate.py`、`scripts/tests/test_check_syntax_gate.py` 或自身变更时触发（并支持 `workflow_dispatch`）。
2. **模板自检与单测全量执行**：每次触发均执行 `python3 scripts/check_syntax_gate.py --self-check-template` 与 `python3 -m unittest discover -s scripts/tests -p 'test_check_syntax_gate.py' -v`。
3. **PR 变更提案/Spec 增量校验**：在 `pull_request` 事件下自动收集本次 PR 新增或修改的 `docs/proposals/**/*.md` 与 `docs/specs/active/*.md`，传给 `check_syntax_gate.py --check-file` 校验 6 项必填字段与未决占位符（无表面语法门禁章节或合法 `N/A` 的非语法 Spec 自动放行）。

## 2. 检查点

- [x] `check_syntax_gate.py --self-check-template` 与 `test_check_syntax_gate.py`（9/9）本机通过
- [x] `gen_spec_dag.py` 与 `check_docs.py` 本机通过
- [ ] PR CI 中 `Parser Formal F5 (Surface Syntax Gate)` 实跑验收与归档
