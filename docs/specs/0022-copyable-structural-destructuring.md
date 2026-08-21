# SPEC-0022: 推导条件 `Copyable` 并检查结构化解构类型

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-022` |
| 所属 Phase | Phase 2 |
| 语言规范 | 当前权威为 [v0.24](../guide/00-index.md)；实施契约为尚未启用的 [v0.25 候选 §25](../guide/01-design-decisions.md#25-条件-copyable内联递归与结构化解构v025-候选) |
| 批准依据 | 当前持续 Goal 的站立授权；仍不替代 guide 版本级明确启用 |
| 前置 Spec | SPEC-0019、SPEC-0020 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 用户尚未明确启用 guide v0.25；解除前不得进入 `approved` / `in-progress` |
| 影响范围 | `lang-frontend` type environment / nominal model / type checker、L0115–L0118、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；实现经明确启用后的 guide 契约 |

## 1. Goal

完成后，Phase 2 类型检查能够按名义结构和实际泛型实参确定 `Copyable` 能力，拒绝无限内联
布局与非法 intrinsic `Box` 实参，并把局部 value-class 解构记录为类型安全、顺序稳定的
Copy/Consume 操作，供 Phase 3 所有权检查继续使用。

## 2. 背景

SPEC-0020 已建立 nominal descriptor、泛型替换、interface bound 与 capability identity，
SPEC-0021 已建立 enum case payload 描述；当前仍把 `Copyable` capability bound 留待后续，
局部解构绑定只得到 `DeferredReason::Destructuring`。SPEC-0022 补齐这两类 Phase 2 类型事实，
但不提前建立移动状态或析构语义。

审计发现 v0.24 没有唯一决定 enum / `Nothing` 的 `Copyable` 规则、局部 `_` 的身份、内联图是否
包含 enum payload，以及泛型 `Box<T>` 的 kind 证明。上述选择已集中到 v0.25 候选 §25；该版本
获得明确启用前，本 Spec 只能保持草案。

## 3. 范围与需求

- 在外部 `TypeEnvironment` 中绑定 intrinsic `Box` 身份；源码同名 class 保持普通名义类型，
  未绑定 intrinsic 时不得按拼写猜测。
- 建立可缓存且递归安全的能力查询，至少区分 `Copyable`、`MoveOnly`、`Unknown`、`Error`；
  按 v0.25 §25.1 处理基础类型、`Nothing`、nullable、value class、enum、类型参数与 move-only
  类型，并在 generic substitution 后判定。
- 扩展 type-argument bound 检查：interface bound 沿用 SPEC-0020，内建 `Copyable` bound
  使用同一能力查询；失败产生 L0115，poisoned type 不追加同根级联。
- 按声明顺序建立 value-class field / enum payload 的内联图，nullable 为透明边，所有规定的
  handle 打断环；未经过 handle 再遇同一 nominal declaration 时不因泛型实参变化而无限展开。
  确定性报告 L0116，并阻止无效类型继续进入能力推导。
- 对 intrinsic `Box` 精确检查 arity 与 type kind；非具体 value-class 实参产生 L0117，
  `T : Copyable` 仍不足以证明合法。
- 为局部 value-class 解构检查精确 arity并产生 L0118；记录 statement identity、源类型、
  Copy/Consume mode 及有序的 binding symbol / component type。initializer 只检查一次。
- 局部解构中的 `_` 按普通绑定处理；非 value-class 解构继续使用专用 deferred reason，等待
  一般 member/call 选择，不在本 Spec 猜测 `componentN()`。
- 新增 Rust 窄测与 Phase 2 pass/fail fixture，锁定 source/environment identity、泛型替换、
  递归环顺序、诊断码/关键 Span、重复运行确定性和深图复杂度边界。

## 4. 非目标

- 不实现 move-after-use、部分移动、借用、drop、ASAP 析构或消费式解构后的所有权状态；这些
  属于 SPEC-0027/0028 及 Phase 3。
- 不实现一般 constructor/member/overload/call 选择、独立 `componentN()` 调用或字段投影
  所有权合法性。
- 不实现顺序容器、Map、`Transferable`、companion、跨文件 package/import 或 codegen 布局。
- 不新增语法、crate、依赖或用户可实现的 marker；不按 `Box` / `Copyable` 名称字符串特判。
- 不计算目标相关 size/alignment，也不构造 LLVM 类型。

## 5. 验收标准

- [ ] 用户明确启用 v0.25，本 Spec 从 `draft` 推进为 `in-progress`。
- [ ] `Copyable` 查询覆盖全部封闭类型类别、实际泛型替换、能力上界和同名冒充反例；结果
      可供后续阶段按 stable type identity 查询。
- [ ] enum payload、nullable、value-class field 的直接/间接内联环得到确定性 L0116；经
      class/object/function/intrinsic Box/动态容器打断的环不误报。
- [ ] intrinsic `Box` 只接受具体 value-class instance；普通 class、enum、builtin、interface、
      function、type parameter 和源码同名 `Box` 的边界均有测试。
- [ ] `Copyable` generic bound 的正反例覆盖 L0115、精确 primary/label、poison 抑制与 interface
      bound 回归。
- [ ] value-class 局部解构覆盖 Copy/Consume、泛型替换、精确/过少/过多 arity、普通 `_`
      binding、initializer 单次检查，以及非 value-class deferred 边界。
- [ ] 深名义图、重复查询、重复执行与声明顺序诊断保持确定性，并有与风险相称的复杂度预算。
- [ ] `type_checking` 窄测、Phase 2 pass/fail fixture 与 workspace 标准基线全部通过；
      Architecture、guide 路线图和本 Spec 验收记录同步为实际事实。

## 6. 技术方案与边界

- `type_checking/mod.rs` 继续作为稳定门面；在 `model.rs` 增加最小公开 typed descriptor，
  不把递归算法堆回门面。
- intrinsic identity 与能力绑定归 `TypeEnvironment` / type model；声明图、替换和 nominal kind
  复用现有 descriptor，不建立第二套名义数据库。
- `checker` 下按单一职责增加能力 / 布局与解构检查模块；诊断只经集中 error catalog 产生。
- 能力与布局递归使用显式 visitation state / memo，错误和环按源码声明顺序稳定输出；不得用
  随机 hash 迭代决定诊断。
- 解构 descriptor 只描述类型层面的原子动作，不修改 AST，不伪造 `componentN()` call，也不
  承担 Phase 3 的 move state。

## 7. 实施计划

1. [ ] 激活 v0.25 并把本 Spec 标为 `in-progress` → 验证：索引、正文、路线图、变更记录一致。
2. [ ] 建立 intrinsic Box identity 与 `Copyability` 查询 → 验证：能力矩阵和同名冒充窄测。
3. [ ] 实现 generic capability bound 与有限内联图 → 验证：L0115–L0117、递归和确定性窄测。
4. [ ] 实现 typed structural destructuring descriptor → 验证：L0118、Copy/Consume、泛型与
       deferred 边界窄测。
5. [ ] 补 Phase 2 pass/fail fixture、复杂度和回归测试 → 验证：受影响 frontend 测试。
6. [ ] 同步 Architecture、guide、Spec 验收与验证记录 → 验证：Markdown 链接和 diff。
7. [ ] 运行 workspace 标准基线并创建独立提交 → 验证：实际退出状态与 staged diff。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | v0.25 候选与本 Spec 草案，不含实现 | `docs(guide): draft Copyable checking for v0.25` |
| 2 | 能力、布局、解构实现及测试、Architecture、done 验收 | `feat(frontend): infer Copyable types (SPEC-0022)` |

## 9. 未决问题

- 阻塞：等待用户明确指定 guide v0.25 取代 v0.24。候选 §25 已把实施所需语义选择封闭；
  若用户要求修改其中任何规则，应先更新候选和本 Spec，再启用。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| Markdown 相对链接检查 | 通过 | `AGENTS.md` 与 `docs/**/*.md` 共 58 个文件的本地目标均存在 |
| `git diff --check` | 通过 | 草案 diff 无空白错误 |
| Cargo 基线 | 未执行 | 本提交只改 guide / Spec 草案；实现阶段按根 AGENTS 执行 |
