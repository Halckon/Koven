# SPEC-0239：Linux CI 与双宿主定向回归门禁

> **性质**：实施与验证 Spec · **状态**：in-progress · **读取时机**：维护 Linux/macOS CI 或核验本切片交付时 · **唯一真源**：本 Spec 的范围、验收与交付限制

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-CI-0239` |
| 所属 Phase | 工程验证，覆盖现有 Phase 1–4 实现 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户要求继续计划，并讨论 CI 增加 Linux clippy/test；本阶段按 Guide 门禁后的 Linux CI 切片准备本地改动 |
| 前置 Spec / ADR | 不新增语言或架构决定；复用现有工具链与已接受的 [ADR-0026](../../adr/accepted/0026-linux-x86-64-native-host.md) |
| 关联 Spec | SPEC-0228、SPEC-0237、SPEC-0238（仍按各自实际交付状态维护） |
| 基线 | `aafccaa`，包含未合并的整合与 Guide Litmus 切片；对应 main 审查基线 `3be83b5` |
| 分支 | `feature/spec-0239-linux-ci`，从指定整合基线建立隔离 worktree；不改 main 或既有 worktree |
| 影响范围 | CI workflow、LLVM 安装、CI 汇总策略及测试、验证文档 |
| 非目标 | 不修复 frontend 已知语义缺口、不新增平台/交叉编译、不升级第三方 Actions、不推送或申请 OAuth 权限 |
| 阻塞项 | workflow 发布需用户批准相应凭据权限；远端 Ubuntu/macOS 运行尚未验证，不得归档为 done |

## 1. 合同

- macOS 14 AArch64 与 Ubuntu 24.04 x86_64 各运行 workspace check、严格 clippy、核心测试、
  阶段整合脚本与 Guide Litmus；fmt 单次。矩阵 `fail-fast: false` 保留另一平台的结果。
- Rust 固定 1.96.0；Cargo 依赖使用 `--locked`。Linux 从 LLVM 官方签名源安装固定
  21.1.8 完整 Debian 包版本，验证签名 key 指纹；缺包或版本不符立即失败，不回退 latest。
- macOS 保持 Homebrew `llvm@21`；两平台验证 LLVM/Clang/DWARF 工具及宿主 C driver。
  cache 按 OS/架构、Rust、实际 LLVM、Cargo.lock 与安装脚本区分，不复用跨平台产物。
- 保留 docs-only PR 跳过 Rust 与 feature/fix push 仅运行轻量门禁的现有策略；Guide10/11/13/15
  被 Litmus 测试直接读取，因此一并纳入 Rust filter。CI 脚本和 LLVM action 变更也触发 Rust。
- 手动触发与 main 均执行完整配置；`CI Passed` 要求 changes 成功，并根据事件/路径判断
  每项必需 job，不能把失败检测或意外跳过视为通过。仅 changes job 增加 PR 列表读取权限。
- 保留完整 `lang-codegen` 与 CLI 测试，不只筛选 native_tests；由此纳入 Linux ELF machine、
  triple/DataLayout、DWARF 行表、真实链接/执行、String/Box/Rc 分配释放与 OOM 边界。
- 两个现有 frontend 脚本原样复用。Guide known-gap 检查不是功能完成；frontend 全量不在范围，
  既有 8 项基线失败仍按 SPEC-0237 记录，不通过 ignore/skip 排除新增回归。

## 2. 验收账本

本地环境：Debian 13 x86_64 + glibc，Rust 1.96.0、LLVM/Clang 21.1.8，统一 Cargo target 串行。
全部 Cargo 命令通过 `source /workspace/shared/rust-dev/activate.sh` 启用工具，设置
`CARGO_TARGET_DIR=/workspace/shared/koven/target`、`CARGO_NET_OFFLINE=true`；因此本地不重取依赖。
以上定向测试全部 0 failed / 0 ignored / 0 filtered，重叠套件按各条单独计数，不汇成唯一测试总数。

Ubuntu 24.04 的 apt 安装流程及 GitHub Actions 的真实调度必须在获准发布后另验；本地 Linux
测试不等同于 hosted Ubuntu 或 macOS 通过。

| 验收/命令 | 实际结果 | 边界 |
|---|---|---|
| 原汇总器：changes 失败、四项依赖 skipped | 红测：错误返回0；回归断言失败 | 修改前实际运行旧 inline Python |
| `python3 -m unittest discover -s scripts/tests -p test_check_ci_results.py -v` | 8 tests passed | 覆盖事件策略、失败/取消/缺失、意外跳过、实际 workflow 输入过滤与矩阵 |
| LLVM 官方 Noble 21 amd64 Packages.gz | 核验目标包完整版本及依赖 | 只读核验，不代表 Ubuntu 安装成功 |
| `cargo fmt --all -- --check` | passed，退出0 | 最终 Rust 内容未改动 |
| `cargo check --locked --workspace --all-targets` | passed，退出0 | 下游及全部 target 编译 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | passed，退出0 | 未加 lint 豁免 |
| `cargo test --locked -p lang-frontend --lib` | 180 passed | 不是 frontend 全量 |
| `cargo test --locked -p lang-codegen` | 510 passed + 4 doctests passed | 包含 Linux ELF/DWARF、真实 native 和内存计数 |
| `cargo test --locked -p lang-cli` | 66 passed（48 + 3 + 9 + 6） | 含真实 native build/run |
| `cargo test --locked -p lang-lsp` | 26 passed | 模块测试 |
| `bash scripts/check_stage_integration.sh` | 50 targets / 629 passed | SPEC-0238 两项 nested-loan 测试使原627增至629；无失败/ignore/filtered |
| `bash scripts/check_guide_litmus.sh` | 443 Markdown；124 passed（21 + 31 + 72） | 两诊断缺口及 typed 延后事实继续精确检查，不宣称功能完成 |
| `python3 scripts/check_docs.py` | 443 Markdown passed | 结构不证明语义等价 |
| `python3 -m unittest discover -s scripts/tests -v` | 45 passed（37 docs + 8 CI） | 汇总策略与实际 workflow 契约 |
| actionlint 1.7.12 / YAML parse / `bash -n scripts/*.sh` / inline shell syntax | passed，退出0 | actionlint 本次未接 shellcheck；不等同 hosted 调度 |
| `bash scripts/install_ci_llvm.sh`（Debian 13） | 按预期以1拒绝不支持宿主，未修改系统 | Ubuntu 实际安装未运行 |
| `git diff --check` | passed，退出0 | 最终提交前再次检查 |
| 独立只读复核 | 最终无阻塞项 | Guide 输入触发与 keyring 边界建议已修正；复核未冒称独立运行 Cargo |
| 远端双平台 CI / 发布 / 合并 | 未运行 | 等待明确授权及后续远端验证 |

## 3. 交付条件

- [x] 本地 Linux 命令与静态/策略门禁全部完成并记录
- [ ] 获准发布 workflow 改动
- [ ] 精确提交的远端 Ubuntu/macOS 门禁全部通过
- [ ] 按真实结果完成生命周期与合并闭环

本地提交不是远端 CI 完成；保留 in-progress，不修改其他 Spec 的验收状态。

## 4. 安装与工具来源

- [LLVM 官方 APT 说明](https://apt.llvm.org/)：Noble 21 源、签名 key 指纹与版本化包名。
- [Noble 21 amd64 包索引](https://apt.llvm.org/noble/dists/llvm-toolchain-noble-21/main/binary-amd64/Packages.gz)：
  核验 `llvm-21-dev` / `clang-21` / `libclang-cpp21` 的版本为
  `1:21.1.8~++20251221032922+2078da43e25a-1~exp1~20251221153059.70`。
- [paths-filter 官方支持工作流](https://github.com/dorny/paths-filter#supported-workflows)：
  PR REST 文件列表需要 `pull-requests: read`，仅在 changes job 授予。
- [rust-cache 官方缓存说明](https://github.com/Swatinem/rust-cache#cache-details)：
  显式补充 LLVM 与宿主 key，避免依赖默认环境过滤。
