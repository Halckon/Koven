# ADR-0007: LLVM 工具链与首个目标平台

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-24 依据当前持续 Goal 的站立授权接受；接受前已实际安装 Homebrew LLVM 21.1.8，
用 Inkwell 0.10.0 / llvm-sys 211.0.1 完成编译、动态链接、AArch64 target、合法模块与无效模块
verifier smoke matrix。

## 背景

SPEC-0033 已在 `lang-codegen` 建立 target-independent typed SSA 与独立 verifier。SPEC-0034
需要把标量和控制流先 lower 到该 SSA，再映射为 LLVM IR，因此必须固定一组可复现的 LLVM
major、Rust wrapper、Cargo feature、发现方式和首个目标。若仅搜索任意 `llvm-config`，开发机、
CI 和发布构建可能静默选择不同 C API；若直接使用 Rustc 自带 LLVM，也没有可供 `llvm-sys`
消费的受支持开发库与 `llvm-config` 边界。

2026-08-24 的主机审计为 `aarch64-apple-darwin`：Rust 1.96.0 自身使用 LLVM 22.1.2，Xcode
提供 Apple Clang 21，但系统最初没有 Homebrew LLVM 或可用的 `llvm-config`。Inkwell 0.10.0
是当前 crates.io release，支持 LLVM 11–22；Homebrew 同时提供 keg-only 的 `llvm@21` 21.1.x
bottle。采用 LLVM 21 与 Inkwell 0.10.0 均无需 Git 依赖，并允许通过显式 prefix 与宿主工具链
共存。

## 决策

- 首个受支持 codegen host/target 固定为 `aarch64-apple-darwin`。SPEC-0034 只验收该 host 上的
  LLVM IR；跨编译、Linux、x86_64 和多 target 初始化留给后续 CI/target Spec。
- 固定 LLVM 21.1.x C API 与 Inkwell `0.10.0`，根 workspace 集中声明 Inkwell，`Cargo.lock`
  固定实际 Rust 依赖版本。不得跟随 Inkwell `master`、未固定 branch 或任意更高 LLVM major。
- Inkwell 禁用默认 `target-all`，只启用 `llvm21-1-prefer-dynamic`、`target-aarch64` 和
  `no-libffi-linking`。当前 AOT 路径不使用 JIT/ExecutionEngine；不为未使用目标和 libffi 扩大
  native 链接面。
- 构建通过 `LLVM_SYS_211_PREFIX` 指向包含 `bin/llvm-config` 的 LLVM 21 prefix；macOS 基线为
  Homebrew `llvm@21` 的稳定 keg。不得修改全局 PATH 来遮蔽 Apple Clang，也不得回退到 Rustc
  私有 LLVM。
- 兼容门禁必须验证 `llvm-config --version` 为 21.1.x、Inkwell/llvm-sys 能编译和动态链接、
  LLVM module verifier 能拒绝无效 IR，并在首个 target 上生成确定的标量模块。该门禁已于
  2026-08-24 通过；升级 LLVM/Inkwell 或新增 target 时必须重新执行。
- Inkwell 和 LLVM 只能位于 `lang-codegen` 的 LLVM adapter 内；`lang-frontend`、自建 SSA
  model/verifier 与外围 crate 不得暴露 LLVM context/type/value。LLVM 错误收敛为 codegen
  内部错误，不占用 frontend `Lxxxx`。
- `prefer-dynamic` 是开发与首个目标基线，不承诺最终分发形式。是否静态链接、随工具链捆绑
  LLVM dylib、目标文件格式和 linker 驱动由后续发布/linker ADR 决定。

## 依赖准入检查

- **适配度**：Inkwell 提供 LLVM C API 的强类型安全封装，直接覆盖 module/type/builder/
  verifier/target 边界；Koven 仍掌控自建 SSA 与可观察语义。
- **兼容性**：Inkwell 0.10.0 要求 Rust 1.85+ 并提供 `llvm21-1` feature；项目 Rust 1.96.0
  满足。Homebrew LLVM 21.1.8 已实际构建并通过 smoke，不以版本表替代验证。
- **维护与供应链**：直接依赖为 pre-1.0 Inkwell；其 `inkwell_internals` 过程宏和 `llvm-sys`
  `build.rs` 都是构建期执行代码，升级时必须单独审阅。native C API 与动态库被限制在
  `lang-codegen` adapter，保留以后替换版本的单一边界。
- **许可**：Inkwell 为 Apache-2.0，LLVM 为 Apache-2.0 WITH LLVM-exception。仓库仍
  `publish = false` 且尚未选定项目许可证；本决策不授权发布，进入分发前必须完成项目许可证、
  transitive license 与 NOTICE 审计。
- **成本**：LLVM 是重型 native 工具链，但属于现行 guide 已确定的 AOT 后端；禁用
  `target-all`、libffi 和 JIT 路径，使用单一动态 host backend 控制构建时间与二进制体积。
- **质量属性**：每次进入 LLVM 前运行自建 verifier，生成后运行 LLVM module verifier；
  用户源码错误不得以 Inkwell panic、LLVM assertion 或 native crash 形式暴露。

## 替代方案

### LLVM 22 + Inkwell 0.10.0

暂不作为首个基线。它能与当前 Rustc LLVM major 和 Homebrew 最新公式对齐，且 Inkwell
0.10.0 已提供对应 feature；但 LLVM 22 支持刚进入该 release，而 LLVM 21 路径已在更早 release
中存在。首个后端优先固定已准备实测的 LLVM 21 矩阵，待 object/link 与 CI target 矩阵建立后
再用新 ADR 评估升级，不能因为 Rustc 内部 LLVM major 相同就假定 C API 或链接契约兼容。

### LLVM 21 静态链接

暂不作为首个基线。它能减少运行时 dylib 依赖，但显著增加链接输入、构建时间和产物体积；
最终分发模型尚未决定。后续发布 ADR 可以在 CI 证明后改为 force-static。

### 直接使用 `llvm-sys`

不采用。它减少一层 wrapper，却把大量 unsafe C API、字符串生命周期和 builder 错误扩散到
项目代码。Inkwell 的类型封装更符合当前最小实现与安全 Rust 边界。

### 使用 Rustc 私有 LLVM 或 Apple Clang

拒绝。Rustc 不提供受支持的 `llvm-config`/开发库契约；Apple Clang 的品牌版本也不等同于
上游 LLVM C API major，无法满足 llvm-sys 的版本发现与可复现链接要求。

## 后果

收益：

- LLVM C API、Rust wrapper、feature、prefix 与首个 target 均有单一可验证基线；
- 系统 Apple Clang、Rustc LLVM 与 Koven codegen LLVM 不会通过 PATH 偶然混用；
- frontend/SSA 保持 LLVM 无关，未来升级或替换只影响 codegen adapter 与构建矩阵。

代价与风险：

- 开发与 CI 必须额外安装 LLVM 21，并设置显式 prefix；
- 动态链接产物暂时依赖 LLVM dylib，不能据此宣称可独立分发；
- Inkwell pre-1.0 API、过程宏、llvm-sys build script 和 native C API 增加供应链与升级审计成本；
- 首个目标不代表跨平台支持，新增 target 必须补充 feature、工具链和真实 object/run 验收。

## 关联

- 相关 Spec：SPEC-0034–SPEC-0040
- 相关 ADR：[ADR-0002](./0002-bootstrap-workspace-layout.md)、
  [ADR-0006](./0006-typed-ssa-block-parameters.md)
- 上游依据：[Inkwell 0.10.0 文档](https://docs.rs/crate/inkwell/0.10.0)、
  [llvm-sys discovery/compatibility](https://github.com/tari/llvm-sys.rs/blob/main/README.md)、
  [Homebrew llvm@21](https://formulae.brew.sh/formula/llvm@21)
- 取代的 ADR：无
- 被以下 ADR 取代：无

- 被以下 ADR 局部扩展：[ADR-0026](0026-linux-x86-64-native-host.md) 新增 Linux x86_64 + glibc
  本机目标及其 feature/object/link driver 范围；本 ADR 的其他决定继续生效。
