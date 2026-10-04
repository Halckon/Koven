# Owned local 普通 class 一级字段 replace

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改直接字段 replace 的事实或后端边界时 · **唯一真源**：frontend / codegen 代码、测试与实际验收记录

## 当前集成状态

SPEC-0246 在原 main `8eb2cd3` 上已完成两入口 ownership、lowering、SSA/LLVM 与有界
frontend/native 验收，global/captured 和 paired target 身份已闭合，首轮双平台 PR CI 通过，
Spec 已归档。[PR #13](https://github.com/Halckon/Koven/pull/13) 最终 head
`c5507a51e0a630bb888a95b2f30247e09e1f60c5` 整合 PR #12 deinit 的 main `e6e1100` 后，
资源交叉与全部本地门禁、[双宿主 CI 36975900096](https://github.com/Halckon/koven/actions/runs/36975900096)
均通过，已合入 main `efc52b6993b60c0bdf5072ef1283c145479f7531`。
此前归档 head `4d84ed5b` 因冲突未产生 PR CI 是历史事件，不能以其 push 检查代替最终验证。
历史证据从[完成 Spec 索引](../archive/specs/README.md)追溯。
已交付的 whole-root replace / swap 仍见[root 专页](root-ownership-primitives.md)。

## 独立字段事实

单文件与 unit 发布 `FieldReplaceOwnershipPlan` / `UnitFieldReplaceOwnershipPlan`，通过
`field_replacements()` / `field_replacement(expression)` 查询。字段计划保留原 typed
intrinsic descriptor、精确 place、父 class 类型和新值 Copy / Move / Temporary 交付；unit
使用 source-qualified identity。它们不进入 root commit 查询，不以静态 descriptor 单独
授予正常继续的执行许可。unit 基础及 constant-enabled gate 有独立损坏事实拒绝测试。

unit 字段可变性在一次 ownership 分析内从全部 source 的声明收集，以完整 `UnitSymbolId`
索引，各 body checker 只读共享；使用文件不再决定字段是否为 var。跨文件 owned local
字段 replace 有 frontend 身份/拒绝回归和 CLI build/artifact/run 用例，同名旁源及相同局部
symbol ID 不覆盖权限。该修复不扩展直接字段 Borrow、普通字段赋值或 unit for lowering。

有界目标是 owned local 的普通非泛型 class 一级 `var` 字段；父绑定可为 `val` 或 `var`。
receiver / 字段投影的 Group 只透传 identity；字段与父绑定可变性分开检查。错误或 blocking
deferred 清空可执行计划，未正常继续的 operand 不发布 commit。producer 只接受 body-local
变量，并对 captured receiver 显式 deferred。unit 保存 call-qualified 的原始已批准 target
快照，防止发布后将 plan 与 loan 配对改到同类型 val 字段或另一 receiver；var/local/projection
合法性由封闭 producer 证明，gate 复核快照、typed descriptor/type、loan 与新值交付。

single/unit 的 drop planner 已修复 Member 作为 Place 时将父 owner 提前 ASAP 释放的问题：
投影不是 receiver 的完整 Value 交付，前缀期间仍保留父 owner。temporary 字段 receiver
在 Phase 2 按非 place 报 L0122，不推迟到 ownership 或后端伪装支持。

## SSA、lowering 与 LLVM

`HeapFieldExchange { owner, field: usize, loan, replacement }` 返回一个旧字段值，消耗
exclusive loan 与 MoveOnly replacement，保留父 owner 的完整身份。它与析构旧值的
`HeapFieldReplace` assignment 及 `RootReplace` / `RootSwap` 分开。

verifier 证明 loan 对应同一 owner 的精确 `HeapPayloadPlace → FieldPlace(field)`，CFG
每条 edge 成对核对 owner/loan，不只比较 alias root 集合。错误 owner/field、root/entry
Inout/reborrow loan、shared/inactive loan、消费/泄漏和结果 type/arity 有定向拒绝证据。
field exchange 过滤器已通过 13 项 SSA 测试和 1 项 LLVM 测试；后者内部覆盖 38 个 case。

两入口降低在新值求值前保存字段 loan，在正常 commit 时从当前 binding/pending 状态刷新
owner/loan，避免沿用 CFG 前身份。LLVM 先读旧字段，再存新字段；中间没有 drop、retain、
clone、分配或用户调用。旧字段成为独立 owner，父对象继续承载新字段；无新增 runtime ABI。

single 六项 native 与 unit 直接测试已运行 object/link/run，覆盖一次求值、父对象继续、
旧值借用/返回、Copyable 快照、return / break / continue、内层循环与 Abort 无 unwind，
并逐 pointer 核对新旧字段和父对象释放。unit 同时保留基础与 constant-enabled 入口的边界。

## 已验证表示与未扩大范围

直接测试覆盖整数、Boolean、String、普通 class、value class、Box、Rc 与顺序容器的若干
既有字段表示；unit 另覆盖 enum。single nullable class 正例通过显式 nullable local 初始化
字段后交换，不证明 constructor 中 `Node → Node?` 隐式包装已闭合。single Unit 字段构造
仍缺事实，unit `Rc<Int>?` 名义字段 layout 仍 Unsupported，不宣称任意源码 storage 已支持。

receiver 的 class 声明仍需与使用处同 source；unit 跨文件 class `var` 字段可变性路径实测
L0134，属于保留的前端缺口。跨文件 `make()` 产生 replacement 已有验证，两者不能混同。

nested fields、`this` / 隐式字段、non-owning 或参数 receiver、temporary receiver、index、
generic receiver、value-class receiver、closure/provenance、field swap 和普通 Inout ABI
不在本片。SSA AliasRoots 仍把同父对象字段关联到共同 root；字段 loan 活跃期间的 sibling
read/clone/嵌套 exchange 在 backend 明确 Unsupported。frontend 继续接受可证明 disjoint
的 sibling places，这些后端限制不改写 Guide 的合法性。

普通 class 字段 Borrow 实参（如 `println(h.state)`）仍 Unsupported；native 使用再次
replace 提取新值或读取 Copyable `h.id` 证明父对象继续。普通 field 值直接置于复杂 `&&`
的既有 MissingFact 未扩大；快照在条件前读取到 local 再比较，不声称新增短路支持。

PR #12 的[普通资源 deinit](resource-deinit.md)现已进入 main，当前整合包含该实现。
资源字段交叉已通过 4 项前端与 8 项 single/unit native：父对象和旧值各自保留词法义务，
commit 不提前执行 deinit，正常与控制退出按实际 owner 唯一清理，Abort 不 unwind；8/6/5
组分配/释放逐 pointer 核对。合并后 core、63-target stage 与 Guide 已重跑通过。
本片原有界范围保持，沿用现有 payload ABI，不新增跨线程原子语义或语言规则。

## 测试位置

- frontend：`ownership_field_replace`，unit `field_replace_` validation；已纳入 stage 脚本
- single lowering/native：`field_replace_lower_tests.rs`、`native_field_replace_tests.rs`
- unit lowering/native：`unit_field_replace_tests.rs`
- 资源交叉：`ownership_resource_deinit` 的 `direct_field_resource`；两份 `field_resource_replace_tests.rs`
- verifier：`verify_ownership/field_exchange_tests.rs`
- LLVM：`llvm/adapter/root_exchange_tests.rs` 内 field exchange 测试

实际批次数不累加为一次执行；13 项字段 integration 与 3 项内部 gate 覆盖最终身份边界。
首轮两宿主均执行 core、61-target stage 与 201 项 Guide；macOS 仅保留既有 LLDB 权限 ignore。
新 main 整合重跑的 stage 为 63 targets / 810，Guide 201；上述最终 head 的双宿主结果已实际通过；有界证据不代表 frontend 全量通过。
