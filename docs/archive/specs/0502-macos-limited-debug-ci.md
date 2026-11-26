# SPEC-0502: 双宿主 `Targeted Tests` 统一启用有限调试符号（`debug=1`）与独立缓存前缀

> **性质**：CI 基础设施变更合同 · **状态**：done（历史归档） · **读取时机**：回溯、维护或回退正式 `ci.yml` 中 macOS / Linux `CARGO_PROFILE_*_DEBUG=1` 测试编译配置时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-CI-0502` |
| 所属 Phase | 工程与 CI 基础设施；不改变语言 Phase |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-25 用户在 [SPEC-0497](./0497-parallel-targeted-tests-ci.md)（PR #466）合入 `main` 后授权推进方案 B1：将 macOS 三路 `Targeted Tests*` 作业同步切至与 Linux 一致的 `debug=1` 有限调试符号配置 |
| 前置 Spec | [SPEC-0497](./0497-parallel-targeted-tests-ci.md) |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无（已完成双宿主正式 `ci.yml` 全绿实测并归档） |
| 影响范围 | `.github/workflows/ci.yml`、`scripts/tests/test_ci_debug_profile.py` |
| 语言语义变更 | 否 |

## 1. Goal

将 `.github/workflows/ci.yml` 中 `test`、`test-d10`、`test-e2` 三个双宿主矩阵作业的 `CARGO_PROFILE_DEV_DEBUG` 与 `CARGO_PROFILE_TEST_DEBUG` 从“仅 `ubuntu-26.04` 为 `1`、`macos-15` 为 `2`”统一为双宿主均为 `'1'`（`limited`，保留完整行号与 panic 回溯），并将 `Swatinem/rust-cache@v2` 的 `prefix-key` 统一为 `v1-test-debug1` 以将 macOS 新的 `debug=1` 编译缓存与旧 `v0-rust`（`debug=2`）缓存隔离。

## 2. 背景

- 在 [SPEC-0497](../../archive/specs/0497-parallel-targeted-tests-ci.md)（PR #466，runs `45602893212` / `45611921974`）实测日志剖析中，`macos-15` 的 `lang-codegen` 单次编译耗时高达 `8m04s`（`484s`），为 `ubuntu-26.04`（`2m11s` / `131s`）的 3.7 倍；`test-d10 (macos-15)`（`28m21s`）与 `test-e2 (macos-15)`（`25m55s`）的多 feature 重编译同样受 `debug=2` 完整 DWARF 生成拖累。
- 仓库全部测试（含 `crates/lang-codegen/src/llvm/debug_tests.rs` 测 Koven 产物 Mach-O 行号表）均不依赖 Rust 测试二进制自身的 `debug=2` DWARF；Linux 已长期稳定运行在 `CARGO_PROFILE_*_DEBUG=1` 下。

## 3. 范围与需求

1. **三路测试作业统一 `debug=1`（不污染 `Cargo.toml` 或 `clippy`/`preview`）**：
   - 仅在 `.github/workflows/ci.yml` 的 `test`、`test-d10`、`test-e2` 三个作业的 `env:` 中设置 `CARGO_PROFILE_DEV_DEBUG: '1'` 与 `CARGO_PROFILE_TEST_DEBUG: '1'`。
   - 不在 `Cargo.toml` 写入 `[profile.*]`，不启用 `CARGO_INCREMENTAL`。
2. **缓存隔离**：
   - 将上述三个作业的 `Swatinem/rust-cache@v2` `prefix-key` 统一为 `v1-test-debug1`，避免与旧 macOS `v0-rust`（`debug=2`）缓存混用。
3. **契约单测锁定**：
   - 更新 `scripts/tests/test_ci_debug_profile.py`（先红后绿）锁定三路测试作业双宿主均使用 `'1'` 与 `v1-test-debug1`，且 `Cargo.toml` 与其他非测试作业保持不变。

## 4. 非目标

- 不修改任何 Rust 编译器源码、测试脚本或 `Cargo.toml`。
- 不修改 `clippy`、`preview-*` 或 `ci-e2-shadow-pilot.yml`。
- 不在本 Spec 修改 D10 `-j1` 脚本（留待紧随其后的方案 B2 独立 Spec）。

## 5. 验收标准

- [x] `SPEC-0502` 登记及索引/DAG 通过 `python3 scripts/check_docs.py` 与 `scripts/check_syntax_gate.py`
- [x] `scripts/tests/test_ci_debug_profile.py` 等工作流契约单测先红后绿全通过，且 Linux `Docs & Spec Structural Gate` 全量 `scripts/tests` 通过
- [x] PR 双宿主正式 `ci.yml`（`test`、`test-d10`、`test-e2` 共 6 组矩阵作业及 `CI Passed`）实跑全绿，记录 `macos-15` 编译与整 job 耗时变化
- [x] Architecture 确认无需更新（纯 CI 编译调试级别配置调整，无语言或编译器实现变化）
- [x] 表面语法增量门禁：`N/A`（纯 CI 基础设施变更，无表面语法变更）

## 6. 实施计划

1. [x] 基于合入 PR #466 后的 `main`（`a6d2d51d`）建立独立分支并登记 `SPEC-0502`（顺延避开并行 PR #470 占用的 `0498..0501`）
2. [x] 更新 `scripts/tests/test_ci_debug_profile.py` 锁定双宿主 `debug=1` 与 `v1-test-debug1` 缓存隔离（TDD Red）
3. [x] 更新 `.github/workflows/ci.yml` 中 `test`、`test-d10`、`test-e2` 的 `CARGO_PROFILE_*_DEBUG` 与 `prefix-key`（TDD Green）
4. [x] 运行本地单测与文档/语法门禁，推送建 PR #468 完成双宿主正式 `ci.yml` 全绿实测并归档

## 7. 验证记录

| 验收项 / 命令 | 结果 | 备注 |
|---|---|---|
| 本地定向 `python3 -m unittest`（`test_ci_debug_profile`、`test_ubuntu26_ci`、`test_generated_owner_ci`、`test_pr_ci_reuse`、`test_check_ci_results`、`ShadowWorkflowStaticContracts`） | 通过：99/99 全绿（TDD 先红后绿） | 全量 `scripts/tests` 由 Linux `Docs & Spec Structural Gate` 实跑覆盖 |
| `python3 scripts/check_docs.py`、`python3 scripts/check_syntax_gate.py --check-file docs/archive/specs/0502-macos-limited-debug-ci.md`、`git diff --check` | 全部通过 | 全量 Markdown 与 F5 语法增量门禁通过 |
| PR #468 双宿主正式 `ci.yml`（[run 45628362539](https://github.com/Halckon/Koven/actions/runs/45628362539)，head `6948fd29`） | **全绿通过（`CI Passed` = `success`）** | `Docs & Spec Structural Gate` `1m21s` 通过；macOS 首次 `v1-test-debug1` 冷缓存实测：`Targeted Tests` `19m29s`（较 PR #466 冷缓存 `debug=2` `32m06s` 提速 `-39.3%` / `-12m37s`）、`D10 & Receipts` `26m50s`、`E2` `22m48s`；Linux 三路全绿：`Targeted Tests` `15m34s`、`D10 & Receipts` `12m24s`、`E2` `10m54s` |
