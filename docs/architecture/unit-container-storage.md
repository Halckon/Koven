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

2026-10-02 Linux 本地及 PR24 的 Ubuntu/macOS 双宿主测试覆盖：

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
本地完整 codegen 为 730 unit + 4 doctests，通过且无 ignored/filtered。PR24 实现 head
`c77a9aa0a6fc1b0fcf951afcf4ea9ea945778cfd` 的
[CI 37024363674](https://github.com/Halckon/Koven/actions/runs/37024363674) 已 9/9 jobs
success，两宿主逐名核同一 32 项 Unit storage 测试均 `ok`（31 新增 + 1 旧 Unit root）。
Ubuntu 完整 codegen 730 passed，macOS 729 passed + 1 既有 LLDB 权限 ignored；两宿主各
4 doctests 通过。ignored 不计调试器通过，性能/RSS 未测；该结果不推定后继文档 head 的 CI。
原始红绿与精确验收可按需追溯 [SPEC-0248](../archive/specs/0248-unit-container-storage.md)。

Borrow Unit 跨函数读取、一般 Unit call operand / Value ABI、Unit-field value class 构造、
CompilationUnit `for`、Inout/field source 与 MoveOnly ZST 完整析构仍不由此证明。
这些合法源码能力缺口不等于语言禁止；Copyable Unit 不替代 MoveOnly ZST drop 验收。

## Source size与unit String读取

普通compilation-unit `.size` 经frontend source-qualified descriptor和同步SharedLoan生成
`ContainerLength`，复用当前Borrow参数loan，只结束本次新建loan，再执行CallReturn清理。
具名/Value/Borrow/group/temporary沿用同一header；unit Borrow String `==`/`!=`/`+`消费
current binding，RHS CFG后使用重绑定loan，不隐式clone/消费源。前端replacement保护旧root
至完整RHS完成；提前return仍按ControlTransfer清理。0274双宿主各37条公共命令及15项CI已通过；完整字节证据见[验收身份](../development/evidence/word-frequency-0274/ci.json)。
`||`不同路径容器last-use合流以及一般投影仍有既有拒绝边界。

## Unit 泛型函数直接容器签名

普通unit函数的直接`Array<T>`、`List<T>`、`MutableList<T>`参数和返回通过
`unit_plan/concrete_types.rs::resolve_concrete_type`消费frontend已发布的canonical identity。
resolver只替换直接元素实参，再用`find`查找具体容器；不intern，不递归展开模板。
Borrow size继续读取current SharedLoan，own参数/返回沿用原容器表示和唯一清理。

定向SSA/native覆盖三容器的Int/String空与非空、显式/推断实参、重复Borrow后源复用与own返回；
另覆盖CFG后的loan重绑定、跨文件同名T隔离、重复实例去重和输入顺序确定性。
真实资源元素在Borrow期间存活，own返回后逆序deinit；计数harness逐指针核对分配与释放，
禁止隐式String clone或shared retain。typed arena在lowering前后不增长。

直接T可以取已具体化的List<Int>，三种外层容器已有真实长度与完整清理证据。
即使frontend已发布List<List<Int>>，模板List<List<T>>仍按精确参数Span拒绝为UnsupportedNode。
generic body单独构造listOf(x)且未发布List<Int>的情况仍按构造Span拒绝为MissingFact；
native沿用UnsupportedNode→UnsupportedSource、MissingFact→InvalidModel映射。
两类拒绝各覆盖新/旧object目标，实际native API保全全部目录文件名、bytes与邻居文件，
不残留sibling temporary。这些实现缺口不改变Guide允许的泛型语义；后续body需求发布独立排期。
本地定向证据及尚待完成的交付门禁见[SPEC-0275](../specs/active/0275-unit-generic-container-signatures.md)。
