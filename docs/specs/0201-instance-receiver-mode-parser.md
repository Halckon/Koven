# SPEC-0201：instance receiver mode Parser

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P1-201` |
| 所属 Phase | Phase 1 |
| 语言规范 | 起草基线 v0.32；候选 [v0.34 §34.1](../guide/01-design-decisions.md#341-声明语法与规范化-receiver) |
| 批准依据 | 无；v0.34 尚未启用，且尚未显式重基到现行 v0.33 |
| 前置 Spec | SPEC-0017、0064、0176 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 明确 v0.34 对现行 v0.33 的重基与取代关系；v0.34 启用 |
| 影响范围 | `lang-frontend` Parser/AST、class-family grammar/fixtures、formatter/grammar bridges；Architecture/Roadmap |
| 语言语义变更 | 否；只实施候选 guide 获得效力后的声明语法 |

## 1. Goal

完成后，instance member 可按固定顺序保存缺省/显式 Borrow、Inout 或 Value receiver marker，
非法位置、重复和乱序能稳定恢复，后续类型阶段无需从方法体猜测 receiver contract。

## 2. 范围与需求

- 把 `method_modifiers` 扩展为 `[visibility] [override] [borrow|inout|own] fun`；interface member
  使用 `[public] [receiver-mode] fun`，缺 marker 的语法事实与显式 Borrow 保持可区分。
- `DeclarationModifiers` 保存 receiver marker kind 与精确 Span；不得把 marker 塞入普通参数、
  函数名或通用 visibility 字段。
- receiver marker 只在 class/value/interface/enum/object 的 instance-function slot 解析；顶层、
  companion、constant、field、classifier 或其他声明位置继续确定性拒绝。
- duplicate、逆序、marker 后缺 `fun` 与 owner-aware recovery 复用 L0076/L0077 边界；错误 member
  不吞掉下一 member、enum delimiter、owner `}` 或下一顶层声明。
- 具名 object 的显式 Inout/Value 由 Phase 2 拒绝；Parser 只保存合法语法形态，不承担 owner
  runtime-state 判定。

## 3. 非目标

- 不规范化 receiver mode，不检查 override/interface/delegation contract。
- 不建立 `this` 类型、member call、loan/move/drop 或 SSA/LLVM。
- 不增加调用点 receiver marker、extension receiver、callable reference 或 safe-call 语法。

## 4. 验收标准

- [ ] class/value/interface/enum 的缺省、Borrow、Inout、Value，以及 object 的三种显式 marker
  均能形成保留真实语法的 AST/Span；object Inout/Value 的语言拒绝留给 SPEC-0180。
- [ ] visibility/override/receiver 固定顺序、重复/逆序、顶层/companion/非函数位置正反矩阵通过。
- [ ] 缺 `fun`、缺名称/body 与相邻 member 恢复保持确定，L0076/L0077 primary 精确。
- [ ] formatter、TextMate/Tree-sitter bridge 与双 Lexer/Parser 不变量不回归。
- [ ] `lang-frontend` 窄测试和 workspace 五项基线通过，Architecture/Roadmap 同步。

## 5. 技术方案与边界

扩展既有 class-family modifier scanner 和 `DeclarationModifiers`，复用 `ParameterModeMarker` 的
三种 token identity 或增加职责等价的 receiver marker enum；不得把 member parser 复制成第二套
function parser。formatter 仍按源码 token 保守输出，不因规范化改写缺省/显式 Borrow。

## 6. 实施计划

1. [ ] 扩展 grammar/AST 与 modifier scanner → 验证：Parser AST/Span 窄测。
2. [ ] 完成非法位置、顺序和恢复矩阵 → 验证：class-family compile-fail fixtures。
3. [ ] 同步 grammar bridges、Architecture/Spec 并运行 workspace 基线。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver marker AST/Parser/恢复与文档 | `feat(frontend): parse receiver modes (SPEC-0201)` |

## 8. 未决问题

- 无；语义门禁由 v0.34 与 SPEC-0180 表达。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | Lexer 已有三个 marker；现行 member modifier 明确拒绝它们，需独立 Parser Goal |
| 2026-08-26 候选闭合审计 | 通过 | grammar §13.5 已同步产生式、合法 owner slot、固定顺序与恢复边界；v0.34 未启用，仍不授权实现 |
