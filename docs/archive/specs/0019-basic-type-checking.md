# SPEC-0019: 建立基础类型检查与局部推导

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-019` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.22 §22](../guides/v0.34-pre-restructure/01-design-decisions.md#22-基础类型检查与局部推导v022) |
| 批准依据 | 用户于 2026-08-21 明确启用 v0.22 并要求实施 |
| 前置 Spec | SPEC-0018、SPEC-0066 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` typed model、基础类型检查、L0082–L0090、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；启用后只实施对应 guide 契约 |

## 1. Goal

完成后，调用方可在 SPEC-0018 名称产物上解析基础 TypeRef，为已封闭表达式与 local 建立
确定类型，检查具名函数返回契约，并以精确 deferred reason 保留后续 Phase 2 责任。

## 2. 背景

名称解析已经建立 scope、symbol、overload set 与引用目标，但尚无类型身份、expected-type
传播或 typed 产物。直接扩张到完整类型检查会混入 nominal、泛型、member、callable 和 smart
cast 多个独立变化原因；本 Spec 只物化拟议 v0.22 §22 的基础闭包。

## 3. 范围与需求

- 新增显式 `TypeEnvironment` 与稳定 `TypeId` / typed 产物；环境按 `ExternalSymbolId` 绑定
  builtin 和单态外部签名，不按源码字符串猜测 prelude。
- 解析 builtin / nullable / function TypeRef，规范化相同结构；区分 known、`Error` 与具有
  专用 reason 的 deferred，不允许单一 unsupported 桶。
- 实现 §22 的 integer literal constraint、固定标量字面量、最小相容关系、基础 prefix /
  binary、Elvis 和 non-null assertion 类型规则。
- 按单向 expected type 检查显式 local，推导无标注 local；处理 lambda expected function
  type、普通 block `Unit` 与 lambda/control tail value。
- 预收集本阶段可完整解析的具名函数签名，检查 expression/block body、最近 callable return、
  `Nothing` bottom、fallthrough 与基础 `if` join。
- 注册并产生 L0082–L0090，保留 primary、label、稳定全序和级联抑制；新增真实 Phase 2
  compile-pass / compile-fail fixture。

## 4. 非目标

- 不处理 nominal classifier、type parameter、泛型实例化、member/constructor、overload 或
  call argument 映射；这些节点只使用逐类 deferred reason。
- 不检查 assignment place、mutability、index、callable reference、postfix `?`、接口委托、
  override、visibility、constant evaluation、when 穷尽性或 smart cast。
- 不实现 Kotlin 的完整局部双向约束求解或 mixed numeric operator 提升；类型检查器只消费
  SPEC-0066 已规范化的 `L` / `u` / `uL` / `f` 身份，不回读源码或接受其他后缀。
- 不执行移动、复制、借用、捕获或析构检查，不建立 HIR/MIR，也不接线 CLI/LSP。
- 不再次改变 v0.22 的语言语义；本 Spec 只实现用户已明确启用的契约。

## 5. 验收标准

- [x] TypeEnvironment 显式、不可变且不按拼写硬编码 builtin；跨环境身份失败 loud。
- [x] TypeId、结构规范化、symbol/type-ref/expression 结果和 deferred reason 在重复运行中确定。
- [x] builtin、nullable、function 与 integer-literal constraint 正例覆盖；默认 `Int`→`Long`、
      `u` 的 `UInt`→`ULong`、固定 `L` / `uL` / `f`、signed/unsigned expected type 边界和
      L0090 越界行为明确；L0082 锁定 builtin arity。
- [x] local annotation/inference 覆盖各标量、null、expected integer、lambda 与 deferred initializer；
      L0083/L0084 的 primary 和 expected label 精确。
- [x] 单向 expected type 的边界有定向测试：不从后续 local 使用、overload candidate 或带参
      lambda body 反推类型。
- [x] 基础 operator、Elvis、`!!` 覆盖正反例；L0085 精确锁定 operator 与 operand labels。
- [x] 隐式 `Unit`、显式 expression/block body、return boundary、`Nothing`、fallthrough 与基础
      `if` join 覆盖 L0086–L0089。
- [x] 每类可产生的 deferred reason 都有定向测试并映射到一个后续 Spec；本阶段闭包内没有 deferred。
- [x] frontend 窄测试、workspace fmt/check/Clippy/test、CLI build、文档链接和 diff 全部通过。
- [x] Architecture、guide 路线图、错误码索引和本 Spec 验证记录同步为最终事实。

## 6. 技术方案与边界

- `type_checking/mod.rs` 是稳定门面；`model` 拥有公开环境、类型表和 typed 结果，`checker`
  独占 AST 遍历与 expected-type / callable context，`error` 收敛内部失败。
- 类型结果使用按 typed ID / SymbolId 可查询的有序表，不修改 Parser AST 或 NameResolution；
  名称引用按真实 Span 建立只读索引，查不到应返回内部错误而不是重新按字符串解析名称。
- 函数签名与 body 分两趟；block/control 的 flow summary 与 expression type 同时产生，避免为
  missing-return 再做一次全树扫描。
- 本 Spec 不新增依赖。所有生产 Rust 文件遵守 1000 物理行软上限，状态机不变量和 deferred
  边界使用 rustdoc / 必要私有注释说明。

## 7. 实施计划

1. [x] 用户明确启用 v0.22 并批准本 Spec；等待 SPEC-0066 完成后推进为 in-progress。
2. [x] 建立类型环境、类型表、typed 产物和错误边界 → 验证：model 窄测试。
3. [x] 实现 TypeRef、字面量、local 与基础 operator → 验证：表达式/local 正反例。
4. [x] 实现 callable、return、flow 与基础 control join → 验证：函数正反例。
5. [x] 补 deferred、确定性、source identity 与 fixture → 验证：frontend 全测试。
6. [x] 同步 Spec / Architecture 并执行 workspace 基线 → 验证：所有验收与文档一致。
7. [x] 创建独立提交 → 验证：staged diff 只包含 SPEC-0019。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 基础类型模型、检查器、诊断、fixture、Architecture 与完成状态 | `feat(frontend): check basic types (SPEC-0019)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| Markdown 相对链接、`git diff --check` | 通过 | 所有本地目标存在；无空白错误 |
| `cargo test -p lang-frontend --all-targets --locked --offline` | 通过 | 330 passed；0 failed / ignored / filtered |
| workspace fmt / check / Clippy / test；CLI build | 通过 | 最终基线全部退出 0 |
