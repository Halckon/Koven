# SPEC-0018: 建立单文件作用域与名称解析

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-018` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.21](../guide/00-index.md)：[单文件名称、作用域与预声明环境](../guide/01-design-decisions.md#21-单文件名称作用域与预声明环境v021) |
| 批准依据 | 当前持续 Goal 要求启用 v0.21 并继续分阶段实施 Specs；构成有效站立授权 |
| 前置 Spec | SPEC-0014 `done`；依赖的完整 Phase 1 Parser 增量均已 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无；package / source-root 映射留给 SPEC-0025 前置 ADR |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 名称解析、诊断目录、Phase 2 集成测试、Architecture |
| 语言语义变更 | 否；实现已启用的 v0.21 契约 |

## 1. Goal

完成后，调用方可对一份 `ParsedFile` 和显式不可变 `NameEnvironment` 建立确定性的单文件
scope、symbol、overload set 与名称引用产物，并得到 L0079–L0081 精确诊断。

## 2. 背景

Phase 1 已交付完整文件 Parser 和索引式 AST，但后续类型检查尚不能稳定回答名称指向哪个
声明。v0.21 已封闭双命名空间、预声明、顺序 local 与外部环境边界；本 Spec 把这份契约
物化为 Phase 2 的第一项可复用产物，为 SPEC-0019 及后续类型检查提供唯一名称入口。

## 3. 范围与需求

- 新增只依赖 `SourceMap`、`ParsedFile` 和显式 `NameEnvironment` 的名称解析入口；不读文件
  系统、不隐式加载 prelude，也不展开 package/import。
- 建立文件、classifier member、companion、callable、lambda、block、control body 与 loop
  scope；按确定性遍历顺序分配 `ScopeId` / `SymbolId`。
- 分离类型和值命名空间；预声明顶层与 classifier member；函数同作用域形成源码有序
  overload set，其他同命名空间冲突产生 L0079。
- 按源码顺序处理 type/value parameter、local、解构和 `for` binding；local initializer 在
  binding 生效前解析，嵌套作用域允许遮蔽，声明前 local 产生 L0081。
- 遍历所有 Phase 1 expression、statement 与 TypeRef child；普通名称和 TypeRef 首段形成
  引用，member 名称及限定类型后续段留给后续 Spec。
- 输出只读 scope、symbol、reference、diagnostic 表；诊断按现有确定性规则排序，内部 AST、
  source 或 diagnostic 构造错误通过具体错误类型返回，不对用户源码 panic。

## 4. 非目标

- 不推导或检查类型、泛型 arity、函数签名重复、调用实参映射或 overload 选择。
- 不解析 member、constructor、companion 或 enum variant 的最终目标，不检查 override、
  visibility、接口委托、smart cast、捕获或所有权。
- 不展开 package/import，不决定 source root、文件路径或跨文件身份。
- 不修改 Parser AST、语法、恢复、诊断或 fixture 契约，不新增第三方依赖。

## 5. 验收标准

- [x] 空文件和显式外部环境可解析；环境不会被修改，也没有隐式内建名称。
- [x] 类型/值双命名空间、顶层和成员前向引用、递归及有序函数 overload set 有正例覆盖。
- [x] callable/type/lambda/for/destructuring binding、嵌套遮蔽、initializer-before-binding 和外层
      同名优先行为有正例覆盖。
- [x] L0079 覆盖重复类型、非函数值、参数与函数/非函数冲突，primary / 首声明 label 精确。
- [x] L0080 与 L0081 分离，精确覆盖普通名称和 TypeRef 首段，并保留 member/限定后续段边界。
- [x] scope、symbol、reference ID 与诊断顺序在重复运行中确定，错误 AST/source identity
      通过具体内部错误返回而不 panic。
- [x] frontend 窄测试、workspace fmt/check/Clippy/test 与 CLI build 全部通过。
- [x] Architecture、guide 路线图、错误码索引和 Spec 验收记录与实现事实一致。

## 6. 技术方案与边界

- `name_resolution/mod.rs` 作为稳定门面；`model.rs` 定义公开不可变环境与解析产物，
  `resolver.rs` 独占遍历状态、scope binding map 与诊断生成，`error.rs` 收敛内部失败。
- scope 内部使用确定性映射；公开表保持分配顺序。值 binding 明确区分单一 symbol 与函数
  overload set，lookup 从当前 scope 沿 parent 链再查询外部环境。
- block 在遍历 element 前只扫描稍后 local 的名称/Span，用于 L0081；不提前分配 SymbolId，
  因而嵌套 callable 的 symbol 仍按实际遍历顺序分配。
- member、callable reference 与限定 TypeRef 的后续段不产生 L0080；它们依赖 receiver/type
  信息，由后续类型检查继续解析。

## 7. 实施计划

1. [x] 建立公开模型、错误边界和 L0079–L0081 目录 → 验证：model/diagnostic 窄测试。
2. [x] 实现预声明、scope 与完整 AST 遍历 → 验证：名称解析集成正反例。
3. [x] 补确定性、source identity 与全量回归 → 验证：frontend 全测试和 workspace 基线。
4. [x] 同步 Spec 验收记录与 Architecture → 验证：文档链接、diff 与实现一致。
5. [x] 创建独立提交 → 验证：staged diff 只包含 SPEC-0018。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 名称解析模型、实现、诊断、测试、Architecture 与完成状态 | `feat(frontend): resolve single-file names (SPEC-0018)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test name_resolution --locked --offline` | 通过 | 11 passed；含真实 Phase 2 pass / fail fixture，0 failed / ignored / filtered out |
| `cargo test -p lang-frontend --all-targets --locked --offline` | 通过 | 320 passed；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 workspace member 全部成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 327 passed；frontend 320、CLI 6、lang-std 1；0 failed / ignored / measured / filtered out |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI dev target 构建成功 |
| Markdown 相对链接、`git diff --check` | 通过 | 所有本地目标存在；无空白错误 |
