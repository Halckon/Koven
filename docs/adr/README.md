# Koven 架构决策记录

ADR 只记录现行 guide 留白处、会长期影响多个 Spec 的架构选择。状态和维护规则见
[`../AGENTS.md`](../AGENTS.md)，新记录使用 [`TEMPLATE.md`](./TEMPLATE.md)。

| ADR | 状态 | 决策 |
|---|---|---|
| [ADR-0001](./0001-record-architecture-decisions.md) | accepted | 使用 ADR 保存架构决策及其理由 |
| [ADR-0002](./0002-bootstrap-workspace-layout.md) | accepted | Phase 0 workspace 布局与 `lang-std` bootstrap 边界 |
| [ADR-0003](./0003-diagnostic-architecture.md) | accepted | 结构化诊断的所有权、稳定性与展示边界 |
| [ADR-0004](./0004-source-span-position-model.md) | accepted | 源码身份、半开字节范围与展示位置模型 |
| [ADR-0005](./0005-package-source-root-mapping.md) | accepted | 显式 source root、逻辑路径与 package identity 的确定映射 |
| [ADR-0006](./0006-typed-ssa-block-parameters.md) | accepted | typed SSA 使用 block parameters、IR-local 类型与显式所有权效果 |
| [ADR-0007](./0007-llvm-toolchain-and-first-target.md) | accepted | 固定 LLVM 21 / Inkwell 0.10、显式 prefix 与首个 aarch64 macOS target |
| [ADR-0008](./0008-internal-value-and-allocation-abi.md) | accepted | 使用 target DataLayout、first-class aggregate、无对象 header 的独占 handle 与集中系统分配边界 |
| [ADR-0009](./0009-concrete-closure-internal-abi.md) | accepted | 具体闭包使用函数指针与内联环境，无捕获值退化为裸函数指针 |
| [ADR-0010](./0010-first-native-object-and-linker-contract.md) | accepted | LLVM TargetMachine 生成 Mach-O object，系统 Clang driver 链接，显式 Koven entry 由 C ABI `main` wrapper 调用 |
| [ADR-0011](./0011-first-dwarf-line-mapping.md) | accepted | 显式 SourceMap 驱动首个 DWARF 行表映射，synthetic code 不伪造源码位置 |
| [ADR-0012](./0012-standard-library-bootstrap.md) | accepted | 标准库 bootstrap 由 CLI 编排显式源码/entry，Koven 源码保持唯一真源且不提前定义通用入口 |
| [ADR-0013](./0013-conservative-source-formatting.md) | accepted | formatter 保留 token/comment/newline，只规范合法源码的安全空白并提供非破坏性 CLI |
| [ADR-0014](./0014-versioned-machine-diagnostics.md) | accepted | CLI 机器诊断使用显式选择、版本化 JSON Lines，并与 LSP 位置协议和 operational error 分离 |
| [ADR-0015](./0015-shared-owner-runtime-abi.md) | accepted | Rc 使用 pointer-width 非原子 strong control block、显式 SSA retain 与归零 drop/free |
| [ADR-0016](./0016-interprocedural-borrow-abi.md) | accepted | Borrow/Inout 调用使用显式 loan-backed internal reference，不把 MoveOnly owner 当 Value 消费 |

`proposed` 只表示已有推荐方案，不授权实现。关联 Spec 进入 `in-progress` 前，ADR 必须为
`accepted`；本规则生效后接受的 ADR 还须记录接受依据。存在有效用户站立授权时无需逐份
再次确认，但仍须完成背景、决策、替代方案、收益与代价审计；既有 accepted ADR 不追溯
改写历史。
