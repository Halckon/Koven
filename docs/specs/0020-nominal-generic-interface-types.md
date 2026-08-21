# SPEC-0020: 建立名义/泛型类型与静态 interface 实现

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-020` |
| 所属 Phase | Phase 2 |
| 语言规范 | 当前 [v0.23 §23](../guide/01-design-decisions.md#23-名义类型泛型与接口实现v023)，已取代 v0.22 |
| 批准依据 | 当前持续 Goal 的站立授权可批准 Spec，但不能替代 guide 版本级确认 |
| 前置 Spec | SPEC-0019、SPEC-0017、SPEC-0064 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无；不改变 workspace/阶段/IR 边界 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` nominal model、泛型替换、classifier/interface graph、L0091–L0105、Phase 2 fixture、Architecture |
| 语言语义变更 | 是；v0.23 已明确启用 |

## 1. Goal

完成后，SPEC-0019 的 typed 产物可确定表示同文件名义类型、类型参数和泛型实例；检查
interface 继承/实现、member 签名与 override/default 冲突，并验证 SPEC-0064 的静态接口
委托，而不把 member/call 选择、smart cast、能力推导或所有权混入本 Goal。

## 2. 背景

v0.22 只把源码 classifier/type parameter 标为 deferred。旧 guide 已表达“无 class 继承、
interface 静态分发、委托必须指向 `val` 字段”等设计意图，但没有规定泛型上界集合、interface
runtime 表示边界、签名等价、override 必需性、冲突优先级或错误码。v0.23 §23 把这些
缺口封闭后，本 Spec 才能形成可验证实现，不能用 Rust/Kotlin 经验反向补规范。

## 3. 范围与需求

- 扩展类型表为稳定 `NominalId`、`Nominal(arguments)`、`TypeParameter(SymbolId)` 与仅用于
  interface default body 的静态 `Self`；同结构确定性规范化，不以名称字符串定义身份。
- 扩展 `TypeEnvironment`，按 `ExternalSymbolId` 显式绑定封闭的 `Copyable`/`Transferable`
  capability identity；不按名称猜 prelude，重复/错 kind 绑定 fail loud。
- 为单文件 classifier 建立源码有序 descriptor：kind、类型参数/上界、字段、enum 变体参数、
  interface closure、实例 member 签名、body 形态和委托计划。
- 为顶层和 member 泛型函数建立独立类型参数环境与 known signature descriptor；body 可使用
  `T` 检查，调用点实例化/推导继续后置。
- 实现捕获规避类型替换与 invariant use-site 实参检查；无/多实参、interface bound 和不能
  作为 runtime value 的裸 interface 使用分别产生 L0091–L0094。
- 检查所有 class-family supertype 都是 interface，拒绝重复实例与继承环；诊断 L0095/L0096
  必须按源码 edge 确定，图遍历不得受 hash 顺序影响。
- 建立 alpha-equivalent overload shape（模式/返回/bound 不作重载判据）与独立完整 callable
  contract，拒绝不可由调用点区分的重复签名；检查 concrete body、interface
  requirement/default、显式 override、可见性、缺失实现和默认冲突，产生 L0097–L0102。
- 验证 `Interface by field` 的同构造器 immutable `val` 身份、静态 interface 满足关系和
  多来源冲突，保存转发计划但不生成隐藏 AST，产生 L0103–L0105。
- 把源码 nominal/type-parameter TypeRef、known signature 与合法 `this` 从 SPEC-0019 的
  deferred 收敛为 known/error；其余 deferred 仍保留原专用 reason。
- 新增真实 Phase 2 nominal pass/fail fixture，精确断言诊断 code/primary/关键 label 和顺序。

## 4. 非目标

- 不选择 member、constructor、overload、call argument、callable reference 或 `super<I>` 调用；
  不检查命名实参和调用点 Value/Borrow/Inout 匹配。
- 不实现 `when` 穷尽性/smart cast、cast/type-test、`Result?`、assignment/index place、for
  protocol、componentN/destructuring 或 container 原语。
- 不推导 `Copyable` / `Transferable`，不检查 value class 内联递归和 `Box<T>` kind；只保存
  capability bound 给 SPEC-0022/Phase 3。
- 不检查 companion/const/关联调用，不展开 package/import，不建立 external nominal descriptor；
  分别留给 SPEC-0026、SPEC-0025。
- 不实现类继承、`dyn`、variance、raw/default/star type argument、多个/类/F-bound、用户能力
  实现或隐藏 runtime proxy。
- 不执行移动/复制/借用/析构，不修改 Parser AST，不新增 crate 或第三方依赖。

## 5. 验收标准

- [x] 用户明确启用 v0.23；Spec 由 `draft` 推进到 `in-progress`，当前阻塞清零。
- [ ] 同名不同声明保持不同 nominal identity；相同声明/实参规范化；类型参数身份和替换在
      嵌套 nullable/function/nominal 中确定且捕获规避。
- [ ] arity、invariance、interface bound、能力 bound 延后和 interface runtime-value 边界分别
      有正反例；L0091–L0094 锁定 primary/label，错误后无同根级联。
- [ ] class/value/enum/object/interface 的直接/传递 interface closure 正确；普通 class
      supertype、重复实例和多节点环覆盖 L0095/L0096，N→2N 图族保持线性。
- [ ] 文件与成员 overload shape 覆盖泛型 alpha-equivalence、参数类型及“模式/返回/bound 不参与
      overload”，完整 contract 另行锁定模式和返回；
      concrete/interface body、override 缺失/多余/不匹配/降可见性覆盖 L0097–L0100。
- [ ] abstract requirement、单 default、interface 本地替换、两个 default 冲突和手写 override
      消歧覆盖 L0101/L0102；错误 override、poisoned hierarchy/委托不产生同根缺实现级联，
      结果不依赖声明容器迭代顺序。
- [ ] delegation 覆盖同构造器 `val` 正例、`var`/错作用域/错类型、泛型替换、手写优先及
      delegate/default/双 delegate 冲突，L0103–L0105 保存源码有序来源。
- [ ] known nominal/type-parameter/signature/`this` 不残留旧 deferred；member/call/when/能力/
      qualified 等非目标仍保留准确 reason，不出现通用 unsupported 桶。
- [ ] source/environment identity、重复运行确定性、深泛型/长继承图预算与非法 AST 内部失败
      有测试；所有手写生产 Rust 文件保持 1000 物理行软上限。
- [ ] frontend 窄测试、workspace fmt/check/Clippy/test、CLI build、Markdown 链接和 diff 通过；
      Architecture、guide 路线图、错误码索引和验证记录同步最终事实。

## 6. 技术方案与模块边界

- 保持 `type_checking/mod.rs` 为门面；`model` 只拥有公开 identity/descriptor/typed 查询。
  新增 `nominal/` 子模块，按 `collect`、`substitute`、`hierarchy`、`members`、`delegation`
  变化原因拆分，现有 `checker/` 只消费已验证 descriptor 检查表达式/body。
- 第一趟按 AST/NameResolution 的真实 SymbolId 收集 descriptor 与 signature skeleton；第二趟
  解析 TypeRef/上界；第三趟以显式颜色状态验证 interface graph；第四趟做替换后的
  requirement/default/override/delegation 合并；最后检查 body。每趟只沿有序 ID/edge 访问。
- callable signature key 使用 typed identity，不复制源码字符串。泛型 member 的局部参数以
  declaration-relative slot 做 alpha-normalization；替换进入具体实例前再映射为 TypeId。
- 冲突集合保持来源 enum 和源码 Span，供诊断与后续 `super<I>`/lowering 复用；不把冲突仅
  压成 bool，也不创建用户不可查询的假函数 Item。
- 本 Spec 不新增依赖，不改变 crate DAG，也不需要 ADR。

## 7. 实施计划

1. [x] 用户明确启用 v0.23，清除版本门禁并把本 Spec 置为 `in-progress`。
2. [ ] 建立 nominal/type-parameter/descriptor 与替换模型 → 验证：identity/arity/bound 窄测。
3. [ ] 验证 supertype/interface graph → 验证：kind/duplicate/cycle/线性族。
4. [ ] 收集并合并 member signature/requirement/default/override → 验证：L0097–L0102。
5. [ ] 验证 delegation 并保存转发计划 → 验证：L0103–L0105 与冲突来源。
6. [ ] 收敛 deferred、补 fixture/确定性/source/budget → 验证：frontend 全测试。
7. [ ] 同步 Spec/Architecture/guide 并执行 workspace 基线。
8. [ ] 创建独立提交 `feat(frontend): check nominal types (SPEC-0020)`。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | nominal/generic/interface/delegation typed 实现、测试、Architecture 与完成状态 | `feat(frontend): check nominal types (SPEC-0020)` |

## 9. 未决问题

- 无；v0.23 已获得版本级明确启用，正文已封闭实施所需语义选择。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| Markdown 相对链接、`git diff --check` | 通过 | 所有本地目标存在；无空白错误 |
| Rust / Cargo 基线 | 不适用 | 当前仅起草候选 guide 与 draft Spec，未修改 Rust |
