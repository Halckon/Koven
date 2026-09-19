# Koven Architecture Decision Records

> **性质**：架构决策索引 · **状态**：current · **读取时机**：变更 workspace、IR、ABI、runtime、目标或长期工具策略时 · **唯一真源**：各 ADR 正文

只读取与当前任务直接相关的 ADR。Accepted 表示决定仍生效，不表示对应功能优先级或当前实现状态。

## Accepted

| 领域 | ADR |
|---|---|
| 治理与 workspace | [0001 ADR 流程](accepted/0001-record-architecture-decisions.md)、[0002 workspace](accepted/0002-bootstrap-workspace-layout.md) |
| 诊断与 Source | [0003 诊断架构](accepted/0003-diagnostic-architecture.md)、[0004 Span 模型](accepted/0004-source-span-position-model.md)、[0014 机器诊断](accepted/0014-versioned-machine-diagnostics.md) |
| package / tooling | [0005 source root](accepted/0005-package-source-root-mapping.md)、[0013 formatter](accepted/0013-conservative-source-formatting.md)、[0020 compilation unit](accepted/0020-multifile-compilation-unit.md)、[0021 LSP source set](accepted/0021-lsp-explicit-source-set-protocol.md)、[0022 project manifest](accepted/0022-minimal-project-manifest-source-discovery.md) |
| SSA / target / native | [0006 typed SSA](accepted/0006-typed-ssa-block-parameters.md)、[0007 LLVM/target](accepted/0007-llvm-toolchain-and-first-target.md)、[0010 object/linker](accepted/0010-first-native-object-and-linker-contract.md)、[0011 DWARF](accepted/0011-first-dwarf-line-mapping.md) |
| Runtime / ABI | [0008 value/allocation](accepted/0008-internal-value-and-allocation-abi.md)、[0009 closure](accepted/0009-concrete-closure-internal-abi.md)、[0015 Rc](accepted/0015-shared-owner-runtime-abi.md)、[0016 Borrow](accepted/0016-interprocedural-borrow-abi.md)、[0017 nullable](accepted/0017-nullable-handle-ssa-abi.md)、[0018 String](accepted/0018-string-owner-runtime-abi.md)、[0019 argv](accepted/0019-parameterized-process-entry-bridge.md)、[0023 iteration provider](accepted/0023-borrowed-sequential-iteration-provider.md)、[0024 growing recipe](accepted/0024-reject-parameter-growing-runtime-recipes.md) |
| 标准库 | [0012 bootstrap](accepted/0012-standard-library-bootstrap.md) |

## Proposed

当前无 proposed ADR。

新决策使用 [TEMPLATE.md](TEMPLATE.md) 并先放入 `proposed/`。改变 accepted 决定时新增 ADR；旧记录
进入 archive 并建立双向取代关系。
