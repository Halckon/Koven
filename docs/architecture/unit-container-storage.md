# Unit 容器存储

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Unit 列表式容器、零大小 element layout 或 callable ABI 边界时 · **唯一真源**：`lang-codegen` 代码与测试

## 单文件构造边界

单文件 lowering 支持 `arrayOf<Unit>`、`listOf<Unit>`、`mutableListOf<Unit>` 的列表式构造。
既有 `EmptyMutableList` 路径的 `MutableList<Unit>()` 也使用同一存储表示；这不扩展到一般
runtime-length initializer。现行语义与存储决定仍来自 Guide 和 [ADR-0008](../adr/accepted/0008-internal-value-and-allocation-abi.md)。

`ssa/lower_frontend/container.rs` 先按源码顺序 lower 每个 operand 一次，遇到
`LoweredValue::Unit` 时核对 resolved frontend element / expression 的内建 Unit identity，
取得 canonical SSA type，再追加 `Constant(Unit)` 作为 container operand。
Diverged 保留已执行前缀并终止构造，不生成后续 operand 或 ContainerConstruct。
普通 Unit expression 仍使用原无结果表示；Unit 是 Copyable，不引入 element owner/drop。

## 独立 element storage 与 ABI

`llvm/layout.rs` 用目标 DataLayout 查询空 literal struct `{}` 的 size/alignment，再进入
既有 checked plan；size=0、stride=0，alignment 不从宿主常量表推断。
`llvm/type_map.rs` 将 Unit 存在独立 `container_unit_storage` 表，仅
`container_element_type` 查询可取到；普通 `basic_type` 仍拒绝 Unit。
非 Unit element 复用原 basic type，foreign/unknown/Opaque identity 没有空表示 fallback。

三种 container header 保持原 shape、size、field offsets 与 target alignment。
空与非空 Unit buffer 都指向非空、对齐、module-private readonly sentinel；没有 Unit element
GEP/store、buffer malloc/free call，也没有 Copyable Unit element drop loop。
普通 Unit producer/entry 仍为 SSA returns `[]`、LLVM `void`，factory 返回既有 header。
这次接入不开放 Unit-field aggregate、shared-control payload、Value(Unit) 参数或显式
SSA Unit return；一般 callable/storage 能力不能通过 container 查询旁路取得。

## 逻辑读取与 CFG

element place 仍先执行 signed Int 的负数和 `index >= length` guard。
直接零大小 place 的 Read 可使用 Unit 常量；loan 跨 CFG 由 pointer phi 携带时，LLVM 可产生
合法的 `load {}`。该 load 的 store/ABI size 均为 0，alignment 来自同一 DataLayout，且仅能从
bounds-success block 进入；不能据此泛称所有 Unit Read 均无 load。

真实 `for` 仍按逻辑 cursor/length 和 owner/place/loan 身份迭代，不比较 sentinel 地址合并元素。
checked `count += 1` CFG 后的 named Unit Read 使用重绑定 loan；正常回边结束本轮 element
loan 并递增 cursor，耗尽边结束 source loan 后 drop temporary container owner。

## 直接证据与边界

2026-10-02 Linux 本地测试覆盖：

- 三容器 × 0/1/3 native 固定 bytes，元素 producer 源码顺序一次、source 一次、body/count 精确；
  另有 discard、if 后 Read 和 `MutableList<Unit>()` 零次迭代。
- 独立 SSA 九格检查 FunctionId/entity、preheader、guard、edge arguments、CFG 后 Read、
  loan cleanup/owner drop；三 provider Diverged 保留前缀，一格 fresh frontend chains 的全部
  SSA/LLVM bytes 相等。
- 三 header 真实 target layout、九格 LLVM sentinel/void ABI、跨 CFG 的零字节 Read；低层
  verified SSA Array 的索引 0/2 各输出 `read-ok\n`，-1/length/empty 0 在 marker 前 SIGABRT。
- Unit/Int 两项 OperationContract、L0087/L0135 精确 span、mixed-analysis 写出前拒绝且保留
  旧 object bytes，以及一般 Unit ABI/aggregate/shared payload 的原 Unsupported 边界。

测试位于 `ssa/sequential_for_lowering_tests/unit_storage*_tests.rs`、
`native_sequential_for_tests/unit_storage*_tests.rs` 和 `llvm/unit_storage_tests.rs`。
本地完整 codegen 为 730 unit + 4 doctests，通过且无 ignored/filtered；精确命令、其他门禁与
交付状态见 [SPEC-0248](../specs/active/0248-unit-container-storage.md)。双宿主 CI 尚未运行，
上述新能力仅有 Linux 本地实测，不将通用 target 代码误称为新增 macOS 运行证据。

Borrow Unit 跨函数读取、一般 Unit call operand / Value ABI、Unit-field value class 构造、
CompilationUnit `for`、Inout/field source 与 MoveOnly ZST 完整析构仍不由此证明。
这些合法源码能力缺口不等于语言禁止；Copyable Unit 不替代 MoveOnly ZST drop 验收。
