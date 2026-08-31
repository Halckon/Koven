# SPEC-0220：compilation-unit pointer-like nullable storage lowering

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-220` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34](../guide/00-index.md) 的既有 `T?`、generic 与 instance receiver 规则 |
| 批准依据 | 2026-08-31 持续 Goal 要求继续按 Phase 推进 guide 对应 Specs、简化验收并避免 `lang-frontend` 全量测试；SPEC-0191 审计确认 generic `T?` 字段只缺 compilation-unit Phase 4 消费 |
| 前置 Spec | SPEC-0035、0184、0196、0199、0218、0219 `done` |
| 前置 ADR | [ADR-0017](../adr/0017-nullable-handle-ssa-abi.md) `accepted` |
| 关联 Spec | SPEC-0191 |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` compilation-unit type planning、expression/value adaptation、field replacement 与 native tests；Architecture/Roadmap |
| 语言语义变更 | 否；复用既有 nullable typed/ownership facts 与 nullable-handle SSA/LLVM ABI |

## 1. Goal

完成后，compilation-unit lowering 可把 ordinary class、`Box`、`Rc` 的 concrete nullable 类型映射为
ADR-0017 `NullableHandle`，并让 generic ordinary-class 的 direct `T?` 字段经 non-null wrap、null
物化、construction、Value call delivery、Inout replacement 与 conditional drop 走 verified
SSA→LLVM→object/link/run 主线。

## 2. 范围与需求

- `UnitTypeKind::Nullable(inner)` 仅当 concrete inner 为 ordinary class、intrinsic `Box` 或 `Rc` 时
  映射为独立 `NullableHandle<inner>`；inner 与 nullable 保持不同 `SsaTypeId`，LLVM 继续使用单指针 niche。
- generic callable/field recipe 的 direct `T?` 按 concrete owner/callable substitutions 查找已有 canonical
  nullable identity；不得在 codegen intern frontend 类型，也不得从全局存在推测字段授权。
- null literal 只按 frontend expression type 生成 `NullableNull`；non-null inner 适配 nullable expected type
  时，先按 Phase 3 delivery 消费原 owner，再生成 `NullableWrap`，不 copy/retain 或增加 allocation。
- local binding、constructor argument、Value call argument、普通 root assignment、return 与 current
  receiver field replacement 共享同一 exact expected-type adaptation，避免各路径形成不同所有权语义。
- nullable field replacement 继续消费 SPEC-0218 assignment descriptor 与既有唯一
  `BeforeReplacement/ReplacedField` drop fact；LLVM 顺序保持 RHS 完成→load old nullable→conditional
  inner drop→store new nullable。
- validated-before-LLVM 不变；缺 canonical identity、inline/function/String nullable、错误 delivery/fact 或
  非 pointer-like inner 必须以确定性 `UnsupportedNode` / `MissingFact` 失败，不能 panic。

## 3. 非目标

- 不实现 inline value/enum/function/String nullable ABI，不改变 ADR-0017。
- 不接 nullable `if`/smart cast、nullable `when`、`!!`、Elvis、safe-call 或 non-null extraction；单文件
  SPEC-0196 能力不会因本 Spec 自动扩张到 compilation-unit control flow。
- 不扩张 closure thunk 的隐式 tail expected-type gate；本次 `return` 适配限定在具名 callable 的
  expression body 与显式 `return` 路径。
- 不开放 `List<T?>`、`Wrapper<T?>`、参数增长型 owner 或 inherited owner recipe；SPEC-0191 其他门禁不变。
- 不改变 `Module::add_nullable_handle_type` 的“inner owner 已定义”不变量；因此
  `class Node(val next: Node?)` 这类 owner-definition cycle 留给独立 SSA type-cycle Spec，本次保持带 Span
  的确定性拒绝。
- 不新增 frontend fact、诊断码、runtime ABI、依赖或公开跨 unit callable ABI。

## 4. 验收标准

- [x] class/Box/Rc concrete nullable type 形成独立 `NullableHandle`；Int/value class/function/String nullable
  以带来源 Span 的确定性错误拒绝。
- [x] generic `Holder<T>(var item: T?)` 对 class actual 完成 exact layout、non-null/null construction、Value
  delivery 与 field replacement；模板 `T?` 与 concrete `Node?` 双 identity 均被核对。
- [x] SSA/LLVM 锁定 `NullableWrap`/`NullableNull`、RHS-before-old-load、nullable conditional drop-before-store，
  且无 tag、wrapper allocation、copy 或 retain。
- [x] source→object→Clang link→run 覆盖旧值非空→null→新非空 replacement，退出 0 且无重复 drop/free。
- [x] 运行 `unit_plan_tests`、nullable type/aggregate/receiver 职责组与 `native::unit_tests`；运行 workspace
  library check/clippy、fmt/diff，并由独立复核检查 fact/owner/ABI 边界。不运行约一小时的
  `lang-frontend` 全量测试，除非窄测暴露 frontend 公共不变量问题。
- [x] Architecture、Roadmap、SPEC-0191 与本 Spec 验证记录同步。

## 5. 技术方案与边界

扩展现有 `UnitTypeLowering` 与 `resolve_concrete_type`，复用 `Module::add_nullable_handle_type`、
`NullableWrap`、`NullableNull` 和既有 drop glue。expected-type adaptation 由 unit lowerer 单一 helper
完成，调用方仍负责消费对应 frontend delivery/drop fact；helper 不重新解释 AST 或重算 copyability。

字段 layout 仍先消费 SPEC-0219 exact owner descriptor。codegen 的 recipe gate 只允许 direct
owner parameter 的 nullable 模板，concrete inner 再由 type lowerer 核对 pointer-like kind；因此
`Holder<Int>` 不会因同一 `Holder<T>` 模板而获得 inline nullable ABI。

## 6. 实施计划

1. [x] 建立 unit nullable type/substitution/storage gate → 验证：type planner 与正反 layout 窄测。
2. [x] 接 null/wrap expected-type adaptation 与 construction/call/assignment/return → 验证：SSA 正反矩阵。
3. [x] 接 generic nullable field replacement 的 LLVM/native 闭环 → 验证：顺序、conditional drop、真实运行。
4. [x] 独立复核、同步 Architecture/Roadmap/SPEC-0191 并运行分层验收。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Spec 范围与精简验收门禁 | `docs(spec): stage unit nullable storage lowering (SPEC-0220)` |
| 2 | unit nullable type/adaptation/replacement/LLVM/native 与完成文档 | `feat(codegen): lower unit nullable storage (SPEC-0220)` |

## 8. 未决问题

- 无。nullable control flow 与 inline ABI 已明确排除，不阻塞本切片。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-31 前置审计 | 通过 | SPEC-0196 已提供 nullable SSA/verifier/LLVM；SPEC-0219 exact descriptor 已含 concrete `T?`，缺口限定在 compilation-unit Phase 4 type/adaptation/drop 消费 |
| `git diff --check` | 通过 | Spec staging 文档空白门禁 |
| nullable 红测 | 按预期失败后转绿 | generic `Holder<T>(T?)` 初始在 construction exact-type gate 以 `MissingFact` 失败；type/substitution 与统一 expected-type adaptation 接线后通过 |
| `cargo test -p lang-codegen --lib unit_plan_tests` | 23/23 通过 | direct nullable recipe 不扩张 inherited/参数增长型 recipe |
| `cargo test -p lang-codegen --lib 'ssa::unit_lower_'` | 106/106 通过 | nullable type 正反矩阵、local/construction/call/root assignment/return、receiver replacement 及全部 unit-lower 职责回归 |
| `cargo test -p lang-codegen --lib 'native::unit_tests'` | 20/20 通过 | generic nullable field 的非空→null→非空 replacement 经 object→Clang link→run 输出 `nullable-field`；其余 unit native 回归不变 |
| 递归 owner-definition cycle 审计 | 显式保持门禁 | `class Node(val next: Node?)` 需要放宽共享 SSA model 的“inner owner 已定义”不变量，留给独立 Spec；当前以带 Span `UnsupportedNode` 拒绝 |
| `cargo check --workspace --lib --locked --offline` | 通过 | workspace library Layer 2 构建门禁；未运行耗时 `lang-frontend` 全量测试 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 首轮发现并移除 assignment 分支的冗余 let，复跑零 warning |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 | Rust 格式与补丁空白门禁 |
| 独立高风险复核（nullable delivery/fact/ABI） | 通过 | 首轮发现 Move place→nullable adaptation 跳过 delivery root identity 的 P2 与对应覆盖缺口 P3；加入 fact root＝AST place symbol＝live ValueId 核对及 place fixture 后二次复核关闭，无剩余 P1/P2/P3 |
| 独立复核 `cargo test -p lang-codegen --lib` | 312 通过、1 ignored | ignored 为既有 LLDB 权限用例；未运行约一小时的 `lang-frontend` 全量测试 |
