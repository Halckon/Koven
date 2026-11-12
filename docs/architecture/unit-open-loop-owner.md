# 开放 loop 的 owner 终止端

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改开放 loop owner 产品或 body Return · **唯一真源**：Phase 3 open-loop 产品与 Phase 4 open-loop Return consumer

## 统一产品与兼容

`UnitOwnerObservation` 只有一份 `open_loop_value`，公开统一 getter 返回 BreakLive 或
ReturnDelivered。共享 Prefix 保存实际 P→Q producer、catalog、状态与六步事件；两 catalog
不合并、不重编号。[旧 Replace→Break](unit-replace-break-owner.md) getter 只投影 BreakLive，
保留旧 completion identity、jump、独立 body 完成快照及各后继 consumer。

原 closed-loop 产品和 validator 仍要求原封闭 body；Return 不成为其新完成枚举，不把
已交付 Q 表示为仍存活的 Break，不生成共同 incoming 或一般 owner phi。

## 有界 Return 路径

首片仅一个 String 根、Boolean 参数 Name 条件、body 一次 root Replace，随后直返根；
条件 false 路径在循环后独立直返根。body detached P 的原 AfterStatement Drop 不移动。

Phase 3 在实际 consuming Name 读取即将移出的根状态，交付时核实际 payload，原 Return
cleanup 后封印 endpoint；实际 body 遍历返回 Ok(false) 且没有旧 jump witness 才承认终止。
条件 false 的 P 从原状态独立 Take/交付/cleanup。旧 Unsupported scalar 不回填新 owner；
两 endpoint 分别绑定本来的 P/Q catalog，完整产品在两端均完成后才发布。

Phase 4 消费统一 ReturnDelivered 分支，复核同轮 primitive、实际 String producer、根及
previous binding、原 RootReplace 发射后的 installed/old 槽与实际 detached P Drop。
两 Name 各自受源身份与块身份授权，旧 delivery/cleanup 后调用唯一 finish_return；其后
再核真实 Return terminator。body 只 Return(Q)，tail exit 只有 condition-false 前驱并
Return(P)，不存在 body→exit/backedge。旧 Break 消费者与发射器保持原路径。

## 验证边界

独立 Phase 3 产品缺失红与 Phase 4 endpoint 授权缺失红均已同源转绿；普通 Basic/Const
SSA/LLVM 证明两终止路径与槽版本，有限 native 输出 outer/next。它不是一般循环 owner
能力，也不证明析构计数或泄漏。CI-only 损坏/foreign 用例仅编译，正式 CI 未运行。
精确阶段头、最终门禁和剩余限制见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#开放-loop-的已交付-return-endpoint2026-11-11)。
