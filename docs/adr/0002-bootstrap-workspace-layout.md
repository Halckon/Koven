# ADR-0002: Phase 0 workspace 布局与 bootstrap 边界

## 状态

accepted

## 背景

现行 guide 要求 Phase 0 建立 `lang-frontend`、`lang-codegen`、`lang-cli`、`lang-lsp`、
`lang-std` 五个 Cargo member，但没有确定物理布局或 target 类型。特别是 `lang-std` 必须从
第一天保存 Koven 源码，同时又不能成为没有 Cargo target 的占位 package。

该选择决定整个仓库的依赖方向、构建入口和 bootstrap 演进方式，影响后续所有 Phase，属于
长期架构决策。这里不决定尚未定义的 runtime ABI 或标准库实现。

## 决策

采用以下 Phase 0 基线：

```text
Koven/
├── Cargo.toml                 # virtual workspace，resolver = "3"
├── Cargo.lock
├── rust-toolchain.toml
└── crates/
    ├── lang-frontend/         # Rust library
    ├── lang-codegen/          # Rust library
    ├── lang-cli/              # Rust binary: kovenc
    ├── lang-lsp/              # Rust binary
    └── lang-std/              # 最小 Rust library + koven/**/*.ko
```

- 使用 Rust edition 2024。首次骨架使用当前仓库已验证可用的 Rust `1.96.0` 作为 pin 和初始
  MSRV；未来降低或提升 MSRV 必须有 CI 证据并作为独立变更处理。
- 根 manifest 集中 workspace package、lint 和共享依赖版本；所有 member 使用 workspace
  inheritance。仓库在发布策略确定前设置为不可发布，不用虚构 license 标识。
- `lang-frontend` 不依赖项目内其他 crate；`lang-codegen` 依赖 frontend；CLI 依赖 frontend
  和 codegen；LSP 只依赖 frontend；`lang-std` 不反向依赖编译器 crate。
- `lang-std` 的 Rust library 仅使源码包、构建元数据和 Cargo 测试入口具有真实 target；标准
  库公共实现仍以 `koven/**/*.ko` 为唯一真源，不在 Rust target 中重写一套标准库。
- Phase 0 不新增 `lang-runtime`。若 Phase 4/5 证明需要独立 runtime artifact，先用新 ADR
  说明 ABI、发布单元和为何现有边界不足，再决定是否修改 guide 的五 member 约束。
- CLI / LSP 在 Phase 0 只证明 target 可构建，不承诺尚未实现的命令行或协议行为。

## 替代方案

### 五个 package 平铺在仓库根

不推荐。短期少一层目录，但编译器源码、文档、fixtures、发行资产和未来工具配置会混杂；
`crates/` 能清楚表达 Rust workspace 的物理边界而不改变 package 名。

### 让 `lang-std` 只有 `.ko` 文件和 `Cargo.toml`

拒绝。Cargo 会把它视为没有 target 的无效 package，直接破坏 Phase 0 的 workspace 基线。

### 现在新增独立 `lang-runtime` crate

拒绝。runtime / ABI 仍未定义，新增第六个 member 会同时违反现行 guide 和最小实现原则。

### 把标准库功能先用 Rust 实现

拒绝。这样会产生两套标准库真源，并违背“标准库从第一天用目标语言自身编写”的约束。

## 后果

收益：

- 五个 member 从第一天都能被 Cargo 检查和测试；
- Rust 工程、目标语言标准库源码和外围工具的边界清晰；
- 不需要为尚未定义的 ABI 提前增加 runtime crate；
- 后续 Spec 可以沿稳定目录和依赖方向增量实现。

代价与风险：

- `lang-std` 会包含一个功能极小的 Rust target，必须持续避免把标准库逻辑迁入该 target；
- 初始 MSRV 直接等于已验证 toolchain，若以后需要更老 Rust，需要另行做兼容验证；
- license 尚未选择时只能保持不可发布，正式分发前必须由用户确定授权方案；
- runtime 边界延后决定，Phase 4 前仍需新的 ABI ADR。

## 关联

- 相关 Spec：[SPEC-0001](../specs/0001-bootstrap-cargo-workspace.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
