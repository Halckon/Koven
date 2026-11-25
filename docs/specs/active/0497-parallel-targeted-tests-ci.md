# SPEC-0497: 正式 CI `Targeted Tests` 按独立套件拆分为并行矩阵作业

> **性质**：CI 基础设施变更合同 · **状态**：in-progress · **读取时机**：维护、扩展或回退正式 `ci.yml` 中 `Targeted Tests` 并行作业拆分时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-CI-0497` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-25 用户在 [SPEC-0496](../../archive/specs/0496-formal-e2-ci-gate-switch.md)（PR #465）合入 `main` 后授权推进方案 A：将 `Targeted Tests` 互不依赖的重型套件拆分为并行作业以缩短关键路径 |
| 前置 Spec | [SPEC-0496](../../archive/specs/0496-formal-e2-ci-gate-switch.md) |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 待本 PR 双宿主正式 `ci.yml`（`test`、`test-d10`、`test-e2` 共 6 组矩阵作业）全绿实测并记录墙钟对比后归档合并 |
| 影响范围 | `.github/workflows/ci.yml`、`.github/workflows/ci-toolchain-image.yml`、`scripts/check_ci_results.py`、`scripts/pr_ci_reuse.py`、`scripts/e2_two_source_shadow_pilot.py`、`scripts/tests/test_*.py` |
| 语言语义变更 | 否 |

## 1. Goal

在不删减、不跳过、不放宽任何现有正式 CI 检查步的前提下，将 `.github/workflows/ci.yml` 中原本单机串行执行 62 分钟（`macos-15`）/ 29 分钟（`ubuntu-26.04`）的 `Targeted Tests` 拆分为 3 个共享相同双宿主矩阵（`macos-15`、`ubuntu-26.04`）、相同 Linux 工具链镜像、相同 `Swatinem/rust-cache@v2` 缓存前缀与相同 `CARGO_PROFILE_*_DEBUG` 配置的并行作业：

1. `test`（`Targeted Tests (${{ matrix.os }})`）：核心组合与 tutorial（`check_integration.sh`）、Parser 差分（SPEC-0478/0481-0489）、借用故障门禁、词频、Conformance、Linux Native Sanitizer 与 Generated Owners。
2. `test-d10`（`Targeted Tests D10 & Receipts (${{ matrix.os }})`）：Unit root-set、D10 全量目录/拆分/并发发布门禁、parent receipt / child cycle 与 scalar owner 独立故障门禁。
3. `test-e2`（`Targeted Tests E2 (${{ matrix.os }})`）：`Experimental E2 replay faults (CI only)` 与 `Verify opt-in two-source Unit capability and atomicity`（均保持 `--formal-gate`）。

同步将 `test-d10` 与 `test-e2` 纳入 `code-evidence`、`ci-passed`、`scripts/check_ci_results.py`、`scripts/pr_ci_reuse.py` 及 `.github/workflows/ci-toolchain-image.yml` 的 fail-closed 必需作业集合。

## 2. 背景

- 在 PR #465 实测（[run 45582820903](https://github.com/Halckon/Koven/actions/runs/45582820903)）中，切换 E2 多核执行器使 E2 两步在 Linux 提速 `44.4%`、在 macOS 提速 `29.6%`，但 `Targeted Tests (macos-15)` 单机串行执行仍需 `62m09s`（`3,729s`），其中 `Run bounded composition including tutorial`（`22m08s`）、`D10 + receipts + scalar owner`（`~20m`）与 `E2`（`18m03s`）三块互不依赖却严格串行。
- 将三块拆为并行矩阵作业后，`macos-15` 关键路径预计由 `62m09s` 降至 `~25m` 左右，`ubuntu-26.04` 关键路径预计由 `29m24s` 降至 `~12m` 左右。

## 3. 范围与需求

1. **三路并行矩阵作业（零步骤删减）**：
   - `test`、`test-d10`、`test-e2` 均使用 `needs: [changes, docs, fmt]`、相同的 `if:` 触发条件、`fail-fast: false`、`os: [macos-15, ubuntu-26.04]`、相同的不可变 Linux 工具链镜像与挂载预检、`CARGO_PROFILE_*_DEBUG`（Linux `1` / macOS `2`）及 `Swatinem/rust-cache@v2`（`v1-test-debug1` / `v0-rust`）。
   - 所有既有步骤 1:1 归属上述三作业之一，不重复也不遗漏。
2. **Fail-Closed 门禁与复用契约全覆盖**：
   - `scripts/check_ci_results.py` 要求 `test`、`test-d10`、`test-e2` 在需要完整代码验证时全部为 `success`；任一缺失、跳过、失败或取消均阻止 `CI Passed`。
   - `scripts/pr_ci_reuse.py` 的 `required_job_names` 要求双宿主全部 6 个作业名称均存在且成功。
   - `.github/workflows/ci-toolchain-image.yml` 发布前校验同步要求全部 6 个 `Targeted Tests*` 作业成功。
   - `scripts/e2_two_source_shadow_pilot.py` 的参考运行提取兼容 `Targeted Tests E2 (<host>)`。

## 4. 非目标

- 不修改任何 Rust 编译器源码、测试脚本本体或测试断言。
- 不修改 macOS `CARGO_PROFILE_TEST_DEBUG=2` 或 `Swatinem/rust-cache@v2` 缓存前缀。
- 不修改 D10 脚本执行方式或 `check_integration.sh` 内部逻辑（留后续独立 Spec）。

## 5. 验收标准

- [x] `SPEC-0497` 登记及索引/DAG 通过 `python3 scripts/check_docs.py` 与 `scripts/check_syntax_gate.py`
- [ ] 全套 `scripts/tests/test_*.py` 契约单测（含新增 `test-d10`/`test-e2` fail-closed 与矩阵守卫单测，并在 Linux `Docs & Spec Structural Gate` 覆盖 `sha256sum` 校验）在最终 head 上全通过
- [ ] PR 双宿主正式 `ci.yml`（`test`、`test-d10`、`test-e2` 共 6 个矩阵作业及 `CI Passed`）在最终 head 实跑全绿，记录新旧墙钟对照
- [x] Architecture 确认无需更新（纯 CI 工作流拓扑拆分，无语言或编译器实现变化）
- [x] 表面语法增量门禁：`N/A`（纯 CI 基础设施变更，无表面语法变更）

## 6. 实施计划

1. [x] 基于合入 PR #465 后的 `main`（`7b817ce2`）建立独立分支 `feature/spec-0497-parallel-targeted-tests` 并登记 `SPEC-0497`
2. [x] 更新 `scripts/tests/` 契约单测锁定三作业并行矩阵与 fail-closed 必需检查（TDD Red）
3. [x] 拆分 `.github/workflows/ci.yml` 为 `test`、`test-d10`、`test-e2`，同步 `check_ci_results.py`、`pr_ci_reuse.py`、`ci-toolchain-image.yml` 与 `e2_two_source_shadow_pilot.py`（TDD Green）
4. [ ] 在最终 head 上完成本地定向单测、Linux 全量 `Docs & Spec Structural Gate` 与双宿主正式 `ci.yml` 实测并归档

## 7. 验证记录

| 验收项 / 命令 | 结果 | 备注 |
|---|---|---|
| 本地定向 `python3 -m unittest`（`test_ci_debug_profile`、`test_ubuntu26_ci`、`test_generated_owner_ci`、`test_pr_ci_reuse`、`test_check_ci_results`、`test_word_frequency_contracts`、`test_check_native_sanitizers`、`test_check_borrow_faults`、`test_recovery_gates`、`test_llvm_install_bounds`、`test_e2_two_source_shadow_pilot` 与 `test_ci_toolchain_image` 矩阵/发布契约） | 通过（TDD 先红后绿） | 定向验证；依赖 Linux `sha256sum` 的 `test_bad_rust_payload_cannot_be_extracted_or_executed` 及全量 `scripts/tests` 由 Linux `Docs & Spec Structural Gate` 覆盖 |
| `python3 scripts/check_docs.py`、`python3 scripts/check_syntax_gate.py --check-file docs/specs/active/0497-parallel-targeted-tests-ci.md`、`git diff --check` | 全部通过 | 1058 份 Markdown 与 F5 语法增量门禁通过 |
| PR #466 首轮双宿主正式 `ci.yml`（[run 45602893212](https://github.com/Halckon/Koven/actions/runs/45602893212)，head `3584c63f`） | 通过（`CI Passed` = `success`） | Linux 关键路径 `29m24s → 16m48s`（`-42.9%`），macOS 关键路径执行 `62m09s → 32m06s`（`-48.3%`），整轮墙钟 `77m14s → 40m45s`；最终 head 审阅修复后的完整 CI 待本轮验证 |
