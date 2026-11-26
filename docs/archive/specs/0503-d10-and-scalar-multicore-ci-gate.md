# SPEC-0503: 正式 CI 切换 D10 目录身份与 Scalar Owner 门禁为唯一产物锁多核执行器

> **性质**：CI 基础设施变更合同 · **状态**：done（历史归档） · **读取时机**：维护或回退正式 `ci.yml` `test-d10` 中 D10 目录身份与 Scalar Owner 两步执行路径时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-CI-0503` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-26 用户在 [SPEC-0502](0502-macos-limited-debug-ci.md)（PR #468）合入 `main` 后授权推进方案 B2：优化 `test-d10` 中 `-j1` 串行脚本 |
| 前置 Spec | [SPEC-0496](0496-formal-e2-ci-gate-switch.md)、[SPEC-0502](0502-macos-limited-debug-ci.md) |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无（实现 head 的正式双宿主 CI 已完成；归档 head 的门禁另验） |
| 影响范围 | `.github/workflows/ci.yml`、`scripts/e2_two_source_shadow_pilot.py`、`scripts/d10_catalog_identity_manifest.json`、`scripts/scalar_owner_entry_manifest.json`、`scripts/tests/test_e2_two_source_shadow_pilot.py`、`scripts/tests/test_ci_debug_profile.py` |
| 语言语义变更 | 否 |

## 1. Goal

在保持现行 `scripts/check_d10_catalog_identity_ci.sh`（15 条目）与 `scripts/check_scalar_owner_entry_ci.sh`（2 条目）原脚本 100% 原样不变作为唯一真源与一键回退路径的前提下，扩展已验证的 `scripts/e2_two_source_shadow_pilot.py` 支持逗号分隔 feature、无 `--features` 默认配置及 `else echo` 静态配对块，并将 `.github/workflows/ci.yml` `test-d10` 中对应两步切换为 `--mode candidate --candidate-jobs default --formal-gate`，消除 34 次 `-j1` 串行 Cargo 包装调用。

## 2. 背景

- 在 PR #468 热缓存 `debug=1` 实测（[run 45653016181](https://github.com/Halckon/Koven/actions/runs/45653016181)）中，`Targeted Tests D10 & Receipts` 仍是双宿主重型作业之一（macOS `23m38s`、Linux `15m21s`），其中：
  - `Independent D10 catalog identity controls (CI only)`（`check_d10_catalog_identity_ci.sh`）：macOS **`9m09s`**（`549s`）、Linux **`6m27s`**（`387s`）——30 次 `-j1` Cargo 调用。
  - `Verify scalar owner entry corruption fails independently`（`check_scalar_owner_entry_ci.sh`）：macOS **`3m13s`**（`193s`）、Linux **`2m08s`**（`128s`）——4 次 `-j1` Cargo 调用。
- 两步合计占 `test-d10` 约 `52%`（macOS `12m22s` / Linux `8m35s`），且均满足 `--list-only` + CI-only `--exact --ignored` + 全 `pass` 契约，可直接复用 `SPEC-0495/0496` 的 `--formal-gate` 执行器。

## 3. 范围与需求

1. **新增两份真源对齐清单**：
   - `scripts/d10_catalog_identity_manifest.json`：按原脚本严格顺序记录 9 段配置 / 15 条目，与 `scripts/check_d10_catalog_identity_ci.sh` 1:1 动静态对账。
   - `scripts/scalar_owner_entry_manifest.json`：记录 1 段配置（`lang-codegen --lib`，无 `--features`）/ 2 条目，与 `scripts/check_scalar_owner_entry_ci.sh` 1:1 动静态对账。
2. **最小扩展 `scripts/e2_two_source_shadow_pilot.py`**：
   - 支持逗号分隔 `cargo_features_arg`（如 `d10-catalog-experiment,loan-observation-test-support`）与空 `cargo_features_arg`（省略 `--features`）。
   - 支持可选 `features_before_target` 以精确还原原脚本中 `--features ... --test ...` 的参数顺序。
   - 在 `verify_shell_script_execution_branch_static` 中兼容无 `cargo` 的 `else echo '...' fi` 提示分支（保持零未校验 `cargo` 调用）。
3. **正式 `ci.yml` `test-d10` 切换与契约锁定**：
   - 两步切换为 `python3 scripts/e2_two_source_shadow_pilot.py run --manifest ... --mode candidate --candidate-jobs default --formal-gate --artifacts ...`。
   - 契约单测同步锁定完整 argv、禁止 `continue-on-error`/`if:`，并允许按文档回退至原 shell 脚本。

## 4. 非目标

- 不修改 `scripts/check_d10_catalog_identity_ci.sh`、`scripts/check_scalar_owner_entry_ci.sh` 或任何 Rust 编译器/测试源码。
- 不修改 `check_unit_root_set_ci.sh`、`check_parent_receipt_ci.sh` 或其他 `ci.yml` 作业拓扑。

## 5. 验收标准

- [x] `SPEC-0503` 登记及索引/DAG 通过 `python3 scripts/check_docs.py` 与 `scripts/check_syntax_gate.py`
- [x] 本地定向 110 项与 Linux `Docs & Spec Structural Gate` 全量 `scripts/tests` 719 项通过（覆盖新增清单与正式工作流契约）
- [x] PR 双宿主正式 `ci.yml`（含 `test-d10` 与 `CI Passed`）实跑全绿，记录两步新耗时与 `45653016181` 基线对照
- [x] Architecture 确认无需更新（纯 CI 执行器切换，无语言或编译器实现变化）
- [x] 表面语法增量门禁：`N/A`（纯 CI 基础设施变更，无表面语法变更）

## 6. 实施计划

1. [x] 基于最新 `origin/main`（`7c422947`）建立分支 `feature/spec-0503-d10-multicore-gate` 并登记 `SPEC-0503`
2. [x] 编写 `d10_catalog_identity_manifest.json`、`scalar_owner_entry_manifest.json` 及 TDD Red 契约单测
3. [x] 最小扩展 `e2_two_source_shadow_pilot.py` 并切换 `.github/workflows/ci.yml` `test-d10` 两步（TDD Green）
4. [x] 运行本地单测与文档/语法门禁，推送建 PR 触发双宿主正式 `ci.yml` 实测并记录耗时

## 7. 验证记录

| 验收项 / 命令 | 结果 | 备注 |
|---|---|---|
| 本地定向 `python3 -m unittest`（`D10AndScalarManifestParityContracts`、`ShadowWorkflowStaticContracts`、`Pr465MergedReviewP2RegressionContracts`、`Pr465ReviewP2RegressionContracts`、`test_ci_debug_profile`、`test_ubuntu26_ci`、`test_generated_owner_ci`、`test_pr_ci_reuse`、`test_check_ci_results`） | 通过：110/110 全绿（TDD 先红后绿） | 验证 `d10_catalog_identity_manifest.json`（9 配置 / 15 条目）与 `scalar_owner_entry_manifest.json`（1 配置 / 2 条目）与原 shell 脚本 1:1 对齐；全量 `scripts/tests` 由 Linux `Docs & Spec Structural Gate` 覆盖 |
| `python3 scripts/check_docs.py`、`python3 scripts/check_syntax_gate.py --check-file docs/specs/active/0503-d10-and-scalar-multicore-ci-gate.md`、`git diff --check` | 全部通过 | 1064 份 Markdown 与 F5 语法增量门禁通过 |

| PR #476 实现 [正式 CI](https://github.com/Halckon/Koven/actions/runs/45754138857) | **双宿主通过，`CI Passed` 成功** | base `7c422947`、head `b194e0ae`、合并验证提交 `8163b924`；文档作业全量 Python 719 项通过 |
| macOS [D10 与 Scalar 门禁](https://github.com/Halckon/Koven/actions/runs/45754138857/job/139689258157) | 9 配置／15 条目及 1 配置／2 条目全部验证 | 执行器耗时分别 424.58s、156.69s；合计 581.27s，较记录的历史 742s 约少 22% |
| Linux [D10 与 Scalar 门禁](https://github.com/Halckon/Koven/actions/runs/45754138857/job/139689258108) | 9 配置／15 条目及 1 配置／2 条目全部验证 | 执行器耗时分别 274.95s、90.40s；合计 365.35s，较记录的历史 515s 约少 29% |

耗时为一次 PR 的实际执行器日志与 §2 历史作业步骤对照，非受控同宿主 A/B；不外推至整条 CI。
