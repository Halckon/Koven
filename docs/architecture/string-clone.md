# String clone 阶段事实与 runtime

> **性质**：当前实现事实快照 · **状态**：current · **读取时机**：修改 String clone 的类型、借用、SSA 或 runtime 链路时 · **唯一真源**：代码及本页链接的验收记录

## String clone intrinsic

单文件 `StringOperationDescriptor` 与 source-qualified `UnitStringOperationDescriptor`
发布 `StringOperationKind::Clone`、receiver expression/type、Borrow receiver 和 Value result。
两条类型检查入口只为已绑定的 builtin String receiver 建立该事实，普通用户同名 member
不获得 intrinsic 身份。下游按 expression identity 查询 descriptor，不重新按 `clone` 拼写识别。
实现位于 `type_checking/string.rs`、`checker/string.rs` 与 compilation-unit 对应子模块。

## String clone ownership facts

`StringOwnershipEffect` 与 `UnitStringOwnershipEffect` 包装已通过检查的 typed descriptor，
向下游发布 receiver/result 类型和稳定 Clone 身份。单文件 checker 复用调用参数 Borrow
合同与 call-loan 结束路径；unit checker 对 receiver place 或 temporary 建立 Shared loan。
clone 结果进入普通 temporary / binding 的 owner、transfer 和 drop 规划，源不因 clone 移出。
借用源、容器元素与临时源的清理仍使用既有 drop point；实现位于 `ownership_checking/string.rs`
及 checker、compilation-unit/dataflow 的 `string.rs`，完整验收账本见
[SPEC-0236](../specs/active/0236-explicit-string-clone.md)。

## 显式 StringClone

单文件与 unit lowering 消费 typed String descriptor 和 ownership effect，核对 receiver/type/
operation identity，再通过已有 Borrow argument 或显式 shared-field projection 路径生成 `Operation::StringClone { source }`。
该 operation 产生独立 StringOwner；operation verifier 检查 shared-loan 类型，ownership verifier
检查 loan 活性，普通结果 move/drop 路径负责 owner 唯一消费。lowering 不按成员名恢复语义。

LLVM adapter 调用 `llvm/string.rs::clone_owner`：非空来源统一通过集中 allocator 分配 length
字节并 memcpy，结果 capacity 等于 length；空结果使用 canonical null pointer、零 length/
capacity，不分配。literal 来源仍深拷贝，drop 复用原 String provenance。具体合同见
[ADR-0027](../adr/accepted/0027-explicit-string-clone-abi.md)，定向测试与 CI 状态见
[SPEC-0236](../specs/active/0236-explicit-string-clone.md)。此路径不扩展 String? native ABI。


## 当前 native 边界

已覆盖 owned / Borrow String、静态与动态临时源、owned / Borrow 顺序容器元素、临时容器元素、
具名 class/value-class 字段和 owned Rc 的 String payload。字段路径生成 SharedFieldLoan /
SharedHeapFieldLoan，不按值读出 MoveOnly 字段；投影实参的表达式局部 drop 在调用 loan 结束后执行。

Borrow 参数 `Rc<String>` 的 payload clone 仍返回确定性的 UnsupportedNode：既有 Rc nullable
view 与普通 Borrow handle slot 的内部边界尚未统一，本切片不改写该 ABI。单文件和 compilation
unit 均有负向回归，不能把该场景表述为可运行。`String?` inline native ABI、安全调用与
既有未封闭 receiver 形状的边界也不由 clone 自动扩展；Str 与 toString 仍未启用。
