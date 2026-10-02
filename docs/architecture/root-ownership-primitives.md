# Owned root 原子所有权置换

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 replace / swap 的 ownership、SSA 或 native 时 · **唯一真源**：frontend / codegen 代码与定向测试

## 身份与提交

两条入口消费 Phase 2 的 compiler-bound `OwnershipPrimitiveDescriptor`，保留 `Replace` / `Swap`、
规范化 `T`、源码顺序两个 operand；unit 身份包含 source unit。同名源码函数仍走普通调用。
普通调用的 Inout 参数立即建立 exclusive loan，不采用仅限方法 receiver 的 two-phase 模型。

Phase 3 只为正常继续到 commit 的 owned mutable `var` whole-root 发布
`OwnershipPrimitiveOwnershipPlan` / `UnitOwnershipPrimitiveOwnershipPlan`。plan 关联原 typed
identity、root place 与新值 Copy / Move / Temporary 交付；temporary 分类也可为 Copyable，
不意味着必有唯一析构义务。错误清空可执行事实。unit 基础与 constant-enabled 验证入口交叉
检查 source、root、exclusive loan、参数合同与普通 Value delivery；任意 descriptor 不能单独
授予正常提交能力。完全 Nothing / return / break / continue 前缀没有 commit plan。

新值完整求值前旧 root 保持 initialized；early return / break / continue 结束已建立 loan
后消费原 drop facts，Abort 不展开。正常 commit 不使用 assignment 的旧值析构路径，旧 owner
交给 replace 结果；swap 交换两端的完整 owner。single planner 运输现有 owner version，已
完成的多分支非 closure 值在原语提交处归一为一个完整 owner，避免为同一 root 生成多份
条件析构。这个处理不放宽条件 closure 清理或普通字段部分移动。

## SSA 与 LLVM

- `RootReplace { owner, loan, replacement }` 返回 `[new_root, old]`
- `RootSwap { owners, loans }` 返回 `[new_a, new_b]`
- commit 消费 exclusive loan 与 MoveOnly 输入，结果取得全新 owner identity，旧 place 失效
- Copyable 值保留独立快照；lowering 给可能共享同一 SSA Value 的不同源码 root 建立独立 Copy
- verifier 验证 active exclusive loan、类型/结果、exact direct-root provenance、重复消费及重叠
- exact-root 验证按同一 CFG edge 成对追踪 owner 与 loan；不能把相同 alias roots 集合当成证明，
  也不接受字段、element、reborrow 或 entry Inout loan 冒充 owned whole-root

LLVM 先读取旧值再存新值；swap 先完成两个 load 再完成两个 store，无 drop、retain、clone、
分配或用户调用插入交换中间。“原子”仅表示语言所有权转移，不是 CPU 跨线程原子指令。
root 原语对 Unit 仅物化局部零大小 storage（独立的[容器存储](unit-container-storage.md)见专页），源码绑定在正常与提前退出边都保持 Unit；物理值随调用前缀运输。函数 / 调用 / Return 的既有 void ABI 保持。
Copyable root 的独占前缀运输槽在内层 loop 中保持与 loan 成对的身份，普通 Value 实参快照仍独立复制。

## 已验收表示与边界

源码端覆盖整数、Boolean、Unit、String、class、value class、enum、Box、Rc、顺序容器与
pointer-like nullable owner 中既有 storage 表示；Box 仍只接受 Guide 规定的 value-class / enum
payload。LLVM 定向矩阵另覆盖精确标量宽度、Char 与零大小表示，不能据此声称所有源码字面量
入口或任意类型均已支持。泛型具体名义实例沿现有 storage；尚未具体化 `T` 的原语模板不
发布执行计划。

本页的 root 专用路径继续拒绝字段、index 与普通 Inout 参数原语 native。独立的
[一级字段 replace](direct-field-replace.md)已在 SPEC-0246 完成原基线的有界本地与首轮双平台 CI
验收，不放宽 root 操作的 exact direct-root 合同。closure 与包含 closure 的递归来源运输
独立 deferred；raw pointer、借用返回、NLL 或跨线程共享未扩展。
普通具体 class 资源的 deinit 与词法清理已由[资源析构](resource-deinit.md)接入 main，
root commit 无提前 drop 的合同保持，并有该切片的组合 native / 唯一释放证据。
field replace 已整合含资源析构的 main `e6e1100`；4 项前端与 8 项 native 资源交叉、
整合后 core/stage/Guide 已通过，独立证据见字段专页，修复 head 双平台结果由 PR 跟踪。

unit 的普通基础入口仍保持既有实参控制退出限制；constant-enabled 入口有真实 native
return / break / continue 覆盖。基础 ownership 的通用短路精度不在本片重写；未到达可信
commit 或依赖缺失计划的路径不得生成假正常交换，专用常量入口依据已有精确短路合同。

## 测试位置

- frontend：`ownership_primitives`；unit 内部 `root_primitive_` validation / const tests
- single lowering：`root_primitive_lower_tests.rs`；unit：`unit_root_primitive_tests.rs`
- verifier：`verify_ownership/root_exchange_tests.rs`
- LLVM：`llvm/adapter/root_exchange_tests.rs`
- native：`native_root_primitive_tests.rs` 与 `native/unit_root_primitive_tests.rs`

实际命令、平台与结果保存在本次 Spec 的验收账本；定向覆盖不代表 frontend 全量通过。
