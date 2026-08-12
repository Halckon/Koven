# SPEC-0001: 建立可检查的 Cargo workspace

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P0-001` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) |
| 前置 Spec | 无 |
| 前置 ADR | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 必须为 `accepted` |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | workspace 根、五个 workspace member、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，仓库具有五个名称准确、target 有效、依赖方向合法且可执行标准 Cargo 基线的
workspace member。

## 2. 背景

本 Spec 启动前，仓库没有根 `Cargo.toml` 或 Rust target，任何后续实现和测试都无法由 Cargo
验证。本 Spec 只建立最小工程骨架，不以占位实现模拟 lexer、parser、runtime 或 codegen。

## 3. 范围与需求

- 按 accepted ADR 建立 virtual workspace，并且恰好包含 `lang-frontend`、`lang-codegen`、
  `lang-cli`、`lang-lsp`、`lang-std` 五个 member。
- 每个 member 都有 Cargo 能识别的真实 target；不得只创建 manifest。
- 在根统一 edition、MSRV、lint、共享依赖和发布 / license 策略，并提交 `Cargo.lock` 与
  明确的 Rust toolchain 配置；license 未选定期间保持不可发布且不虚构许可证标识。
- 建立最小合法依赖方向：frontend 不依赖 LLVM；外围 member 不反向污染核心层。
- target 只包含能证明边界和构建成立的最小代码，并为预期公共边界提供必要 rustdoc。
- 把实际目录、target 类型和依赖图同步到 Architecture。

## 4. 非目标

- 不实现 source / `Span`、AST、诊断、fixture、lexer、parser 或 CLI 参数协议。
- 不引入 LLVM / `inkwell`、LSP 框架、异步 runtime 或第六个 workspace member。
- 不决定目标语言 runtime ABI、包管理或跨平台发布矩阵。

## 5. 验收标准

- [x] `cargo metadata --no-deps` 只列出五个规定 member，且每个 package 至少有一个 target。
- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo check --workspace --all-targets` 通过。
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- [x] `cargo test --workspace --all-targets` 通过，且没有用 ignored 测试伪造覆盖。
- [x] `cargo build -p lang-cli` 通过。
- [x] `Cargo.lock`、toolchain、不可发布策略与共享 package / lint 配置已生成，并纳入本 Spec
      的 staged diff / 提交范围。
- [x] Architecture 已改为实际 workspace 快照并明确仍未实现的编译阶段。

## 6. 技术方案与边界

按已接受的 ADR-0002 采用根 virtual workspace + `crates/` 布局；frontend / codegen 为
library，CLI / LSP 为 binary，`lang-std` 用最小 Rust library 提供目标语言源码包与测试入口。

骨架代码不得建立未被后续 Spec 需要的通用 trait 或配置层。CLI target 可返回成功退出码，
但不得声称已经能编译 `.ko` 文件。

## 7. 实施计划

1. [x] 按已接受的 ADR-0002 建立根配置与五个最小 target → 验证：`cargo metadata --no-deps`、`cargo check --workspace --all-targets`
2. [x] 集中 lint / package 配置并补最小测试 → 验证：fmt、Clippy、workspace test、CLI build
3. [x] 更新 Architecture 和本 Spec 验收记录 → 验证：文档只描述实际落地事实

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 根 workspace、五个 target、锁文件、测试及 Architecture | `chore(workspace): bootstrap Koven workspace (SPEC-0001)` |

ADR-0002 单独提交，不与本 Spec 的实现提交混合。

## 9. 未决问题

- runtime / ABI 的实现位置不在本 Spec 决定；本轮不得因该问题新增 runtime crate。
- 正式发行 license 后续由用户决定；Phase 0 明确保持不可发布，不因此伪造许可证。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo metadata --no-deps --format-version 1 --locked --offline` | 通过 | 恰好五个 member、五个 package、五个 target；edition、MSRV、不可发布策略与依赖方向符合 ADR-0002 |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 member 的全部 target 均可检查 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 1 passed；0 failed；0 ignored；0 measured；0 filtered out |
| `cargo build -p lang-cli --locked --offline` | 通过 | `kovenc` binary 构建成功 |
| `git diff --check`、`git diff --cached --check` | 通过 | 无空白错误；staged diff 仅包含 SPEC-0001 范围 |
