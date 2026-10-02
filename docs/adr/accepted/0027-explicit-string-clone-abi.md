# ADR-0027：String 显式深拷贝增量 ABI

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：实现 StringClone SSA、LLVM helper 或验证其 owner 边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-10-01，用户对“先落地 String.clone()，暂缓 Str 和 toString() 的规范切换”明确回复
“认可，开始实施”。本记录封闭该已批准切片所需的深拷贝 ABI；不批准其他候选字符串能力。

## 背景

[ADR-0018](0018-string-owner-runtime-abi.md) 已定义 StringOwner、三字段目标布局、
static/heap/empty provenance 和唯一 drop。v0.39 新增 shared Borrow receiver 到独立
String owner 的显式 clone，需要跨 typed、ownership、SSA 与 LLVM 固定身份及分配契约。
只复用布局或按成员拼写调用 runtime，不能证明源未被消费、loan 有效或结果不共享缓冲区。

## 决策

### 对既有布局的增量

- 保留 ADR-0018 的 `StringOwner` identity、`{ bytes: ptr, length: usize, capacity: usize }`
  target DataLayout、UTF-8/内嵌 NUL、static provenance 和统一 drop 规则，不改字段或既有操作。
- 非空 clone 对全部源 provenance 一视同仁：通过集中 malloc adapter 分配精确 `length`
  字节，完整复制 `bytes[0..length]`，结果 `capacity == length > 0`，与源缓冲区独立。
  静态 literal 也必须走此深拷贝路径，不能返回静态别名冒充独立复制。
- 空串不调用 malloc；返回既有 canonical pointer 和 `length == capacity == 0`。源与结果
  可复用非解引用 storage，但各自有独立逻辑 owner obligation，drop 分别按 provenance no-op。
- 长度与分配按目标位宽和布局验证；allocation failure 在发布结果 owner 前走既有 abort。
  不要求 NUL 终止，不建立共享 control block，不 retain，不把宿主 String 布局引入 ABI。

### 显式阶段产物

- Phase 2 发布稳定 String clone operation identity、builtin receiver 类型、shared Borrow
  receiver effect 和 owned String result；Phase 3 消费此事实建立调用期 shared loan。
- target-independent SSA 使用专用 `StringClone` operation。operand 必须是有效的 String
  shared loan；结果必须是新 StringOwner，加入普通 move/drop 检查。verifier 拒绝错误类型、
  无效/已结束 loan 与不满足 shared 模式的输入，不把普通 owner 值当作已证明的借用。
- LLVM 只消费通过验证的 operation 和类型；不得按源码 `clone` 名称分支，也不得从 LLVM
  布局反推语言身份。复用 lang-codegen 内部 helper 与 malloc/free/abort 边界，不增加 crate。
- clone 只读源且只求值一次；loan 覆盖复制过程，结束后源仍可用。容器元素 place 的 owner
  留在容器中，结果独立析构；ASAP drop 仍禁止 loan 活跃时释放源。

### 适用范围

语言表面唯一真源为 [v0.39 String](../../guide/13-program-runtime-standard-library.md#string)。
本增量不使 String 成为 Copyable，不引入 Cloneable、ARC/GC、SSO、容器深拷贝或 nullable 特例。
String? 的 inline-nullable native ABI 限制保持不变。Str 和 toString() 延后。

## 替代方案

- 静态源直接返回原 bytes：违反此次非空显式深拷贝合同，不能统一验证缓冲区独立性
- 用 Rc / retain 共享存储：改变 unique owner 与 Transferable 成本模型，不在授权范围
- 用 `source + ""` 降低：隐藏明确 operation identity 和 loan/result 合同，不作为阶段间表示
- backend 按成员名称识别：绕过 typed facts 与 verifier，用户同名成员可能获得错误语义

## 后果

收益：保留原 String ABI，同时使显式 O(n) 复制、独立析构和借用来源可由各阶段直接验证。
代价：每次非空 clone 有一次分配和完整字节复制；static/heap/empty 以及单/多文件路径都需
正反例。空串无分配不等于共享逻辑 owner，测试不得只依赖 pointer 是否相同来判断所有权。

## 关联

- 实施 Spec：[SPEC-0236](../../archive/specs/0236-explicit-string-clone.md)
- 增量扩展：[ADR-0018 String owner ABI](0018-string-owner-runtime-abi.md)；原决定继续有效
- 取代的 ADR：无
- 被以下 ADR 取代：无
