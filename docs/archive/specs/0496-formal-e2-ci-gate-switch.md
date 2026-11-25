# SPEC-0496: 正式 CI 切换 E2 Replay 与 E2 两源 Unit 为唯一产物锁多核执行器

> **性质**：CI 基础设施变更合同 · **状态**：done（历史归档） · **读取时机**：回溯或回退正式 `ci.yml` 中 E2 两套门禁执行路径切换时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-CI-0496` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-25 用户在 [SPEC-0495](./0495-e2-two-source-ci-shadow-pilot.md) 双宿主冷/热影子闭环与 PR #463 合并后明确授权推进第 2 步：正式切换 `ci.yml` 中 E2 门禁 |
| 前置 Spec | [SPEC-0495](./0495-e2-two-source-ci-shadow-pilot.md) |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无（已完成双宿主正式 `ci.yml` 实测全绿并归档） |
| 影响范围 | `.github/workflows/ci.yml`、`scripts/e2_two_source_shadow_pilot.py`、`scripts/tests/test_e2_two_source_shadow_pilot.py`、`docs/development/testing.md` |
| 语言语义变更 | 否 |

## 1. Goal

在保持现行 `scripts/check_e2_replay_ci.sh`（2 配置 / 14 条目）与 `scripts/check_e2_two_source_unit_ci.sh`（5 配置 / 179 条目）原脚本作为真源与一键回退路径不变的前提下，将 `.github/workflows/ci.yml` `Targeted Tests` 中的两步正式 E2 门禁（`Experimental E2 replay faults (CI only)` 与 `Verify opt-in two-source Unit capability and atomicity`）切换为经过 [SPEC-0495](../../archive/specs/0495-e2-two-source-ci-shadow-pilot.md) 双宿主冷/热实证与 8 项 P2 审阅加固的 `scripts/e2_two_source_shadow_pilot.py run --mode candidate --candidate-jobs default`，消除 379 次冗余 Cargo 包装调用与 `-j1` 单核编译瓶颈，同时保留唯一产物锁定、跨配置异构 feature 防混用锁、真实 Cargo 启动环境探针、动态执行分支一致性校验与防漏跑硬校验。

## 2. 背景

- [SPEC-0495](../../archive/specs/0495-e2-two-source-ci-shadow-pilot.md)（PR #463，已合入 `main` `c299a360`）已在 `ubuntu-26.04`（4c / 15.6 GiB）与 `macos-15`（3c / 7.0 GiB）上对 `e2-two-source-unit`（4 组 8 轮）与 `e2-replay`（4 组 8 轮）完成全量冷/热与默认多核 A/B 实证，全部 `100%` 等价通过、`0` OOM、`0` 重试，且完成 8 项 P2 审阅加固。
- 在 PR #463 同 SHA 正式 `ci.yml`（[run 45529296052](https://github.com/Halckon/Koven/actions/runs/45529296052)）单次运行中，两步旧 `-j1` E2 步骤合计消耗：Linux `378s + 562s = 940s`（占整 job `2,456s` 的 `38.3%`）、macOS `571s + 968s = 1,538s`（占整 job `4,122s` 的 `37.3%`）。

## 3. 范围与需求

1. **正式 `ci.yml` 精确切换两步 E2 门禁（启用 `--formal-gate`）**：
   - `Experimental E2 replay faults (CI only)` 改为调用 `python3 scripts/e2_two_source_shadow_pilot.py run --manifest scripts/e2_replay_manifest.json --mode candidate --candidate-jobs default --formal-gate --artifacts "$RUNNER_TEMP/koven-e2-replay"`。
   - `Verify opt-in two-source Unit capability and atomicity` 改为调用 `python3 scripts/e2_two_source_shadow_pilot.py run --manifest scripts/e2_two_source_unit_manifest.json --mode candidate --candidate-jobs default --formal-gate --artifacts "$RUNNER_TEMP/koven-e2-two-source-unit"`。
   - `--formal-gate` 强约束：禁止 `--list-only`、禁止清单包含任何 `expected_failure` 条目、强制完成真实动态执行分支对账；测试断言失败或编译失败时将 Rust panic / Cargo `compiler-message` 诊断片段直接输出到 Actions 日志。
2. **保留原 shell 脚本真源与 fail-closed 对齐校验**：
   - `scripts/check_e2_replay_ci.sh` 与 `scripts/check_e2_two_source_unit_ci.sh` 保持原样不改；每次正式 CI 执行候选路径前自动执行静态与动态执行分支 1:1 校验，任何漂移立即失败。
3. **保留既有 `ci.yml` 缓存与门禁合同**：
   - 不改动 `Swatinem/rust-cache@v2`（`v1-test-debug1` / `v0-rust`）、Clippy、Preview 独立 consumer、D10、`check_generated_owners.py`、Parser Lean/Kani 与差分测试及 `ci-e2-shadow-pilot.yml` 独立影子入口。

## 4. 非目标

- 不修改 `scripts/check_e2_replay_ci.sh`、`scripts/check_e2_two_source_unit_ci.sh` 或任何 Rust 编译器源码。
- 不修改 `check_d10_catalog_identity_ci.sh`、Clippy、Preview 或 `Swatinem/rust-cache@v2` 缓存前缀。

## 5. 验收标准

- [x] `SPEC-0496` 登记及索引/DAG 通过 `python3 scripts/check_docs.py` 与 `scripts/check_syntax_gate.py`
- [x] `scripts/tests/test_e2_two_source_shadow_pilot.py` 新增正式 `ci.yml` 切换契约单测（先红后绿）且既有工作流策略单测全绿
- [x] PR 双宿主正式 `ci.yml`（`Targeted Tests (ubuntu-26.04)` 与 `Targeted Tests (macos-15)`）实跑全绿，记录两步新耗时与 `main` 基线对照
- [x] Architecture 确认无需更新（纯 CI 执行器切换，无语言或编译器实现变化）
- [x] 表面语法增量门禁：`N/A`（纯 CI 工作流切换，无表面语法变更）

## 6. 实施计划

1. [x] 基于合入 PR #463 后的 `main`（`c299a360`）建立独立分支 `feature/spec-0496-formal-e2-gate-switch` 并登记 `SPEC-0496`
2. [x] 更新 `scripts/tests/test_e2_two_source_shadow_pilot.py` 锁定正式 `ci.yml` E2 切换契约（TDD Red）
3. [x] 切换 `.github/workflows/ci.yml` 两步 E2 命令并同步 `docs/development/testing.md`（TDD Green）
4. [x] 运行全套工作流与影子执行器本地单测及文档门禁，推送建 PR #465 完成 PR 审阅 4 项 P2 与 PR #463 二轮 8 项 P2 加固，触发双宿主正式 `ci.yml` 全绿实测并归档

## 7. 验证记录

| 验收项 / 命令 | 结果 | 备注 |
|---|---|---|
| `python3 -m unittest`（`Pr465ReviewP2RegressionContracts` + `ShadowWorkflowStaticContracts` + `Pr463Review2P2RegressionContracts` + `test_ci_debug_profile` + `test_ubuntu26_ci` + `test_pr_ci_reuse` + `test_check_ci_results` + `test_llvm_install_bounds`） | 通过：105/105 全绿（含 PR #465 4 项 P2 与 PR #463 二轮审阅 6 项代码/工作流 P2 回归及 2 项 `SPEC-0495` 归档勘误） | 先红后绿（TDD），确认 `--formal-gate`、失败日志诊断输出、完整 argv 契约与既有 `ci.yml` 缓存/门禁合同全绿 |
| `python3 -m unittest scripts.tests.test_e2_two_source_shadow_pilot -v` | 通过：30/30 全绿 | 覆盖清单漂移、唯一产物锁、环境探针、冷热 A/B 比较与正式门禁全部回归契约 |
| `python3 scripts/check_docs.py`、`python3 scripts/check_syntax_gate.py --check-file docs/archive/specs/0496-formal-e2-ci-gate-switch.md`、`git diff --check` | 全部通过：1057 份 Markdown 与 F5 语法增量门禁通过 | 无 Rust 源码改动 |
| PR #465 双宿主正式 `ci.yml`（[run 45582820903](https://github.com/Halckon/Koven/actions/runs/45582820903)，head `b03ca8c5`）与影子 A/B（[run 45582820897](https://github.com/Halckon/Koven/actions/runs/45582820897)） | **全部通过（`CI Passed` = `success`）** | 对比同口径基线 [run 45529296052](https://github.com/Halckon/Koven/actions/runs/45529296052)：Linux `e2_replay` `378s → 207s`（`-45.2%`）、`e2_two_source_unit` `562s → 316s`（`-43.8%`）、两步合计 `940s → 523s`（`-44.4%`）、整 job `2,456s → 1,764s`（`-28.2%`）；macOS `e2_replay` `571s → 464s`（`-18.7%`）、`e2_two_source_unit` `968s → 619s`（`-36.1%`）、两步合计 `1,538s → 1,083s`（`-29.6%`）、整 job `4,122s → 3,729s`（`-9.5%`） |
