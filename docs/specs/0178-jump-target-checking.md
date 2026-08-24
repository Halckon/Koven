# SPEC-0178：检查 break / continue 词法目标

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-178` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.18/v0.28 loop 与 return 边界](../guide/04-grammar-declarations-blocks.md#123-loops-与迭代契约) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0016、0019 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无；现行 guide 已封闭词法目标规则 |
| 影响范围 | `lang-frontend` type checker callable/loop context、L0142、Phase 2 fixture、Architecture/roadmap |
| 语言语义变更 | 否；只修复现行 guide 已明确的 Phase 2 实现漂移 |

## 2. Goal

让 Phase 2 只接受控制当前 callable 内最近词法 enclosing loop 的 `break` / `continue`，拒绝
loop 外 jump 以及试图从 lambda 跨越 callable boundary 控制外层 loop 的 jump。

## 3. 范围与需求

- `while`、`for`、`loop` 都建立当前 callable 内的词法 loop context；嵌套 loop 采用最近目标。
- 每个具名函数和 lambda 都建立新的 jump boundary；进入 lambda 后不能看到外层函数的 loop，
  但 lambda 自身 loop 内的 jump 合法。
- 无 enclosing loop 的 `break` / `continue` 使用统一 L0142，primary 精确覆盖对应关键字；一次
  jump 只产生一次根因诊断，不伪造后续目标。
- 合法 jump 继续保持 `Nothing` 和 non-fallthrough；非法 jump 使用 error type 并保持控制流
  已终止，避免附带无意义的 missing-return 级联。
- `for` 的 jump context 不依赖尚未发布的 iterator/binding typed facts；本 Spec 不因此批准或
  实现迭代协议。

## 4. 非目标

- 不实现 label、非局部 return、inline callable 例外或完整 CFG target descriptor。
- 不实现 `for` iterator/binding 类型事实、Map、package/import、object/companion 或 receiver
  所有权。
- 不改变 Parser AST、所有权阶段 loop 合流、SSA lowering 或现行 L0086–L0088 含义。

## 5. 验收标准

- [x] `while` / `for` / `loop` 内的 break/continue，以及嵌套 loop 最近目标 compile-pass。
- [x] 顶层 initializer、具名函数 loop 外和 lambda 跨 callable boundary 的 jump 产生 L0142。
- [x] lambda 自身 loop 内 jump 合法；同一外层 loop 中普通 jump 仍合法。
- [x] L0142 的 catalog、消息、primary UTF-8 byte Span 与确定性顺序由领域测试和 fixture 锁定。
- [x] 受影响 frontend 窄测和 workspace 标准基线通过；Architecture、roadmap、Spec 状态与事实一致。

## 6. 技术方案与边界

- `Checker` 保存当前词法 loop depth，`CallableContext` 保存进入 callable 时的 depth base；jump
  只有在当前 depth 大于最近 callable base 时才有合法目标，因此无需回扫 scope graph。
- 检查 loop body 时递增全局 depth 并在返回后恢复；嵌套 lambda 记录包含外层 loop 的新 base，
  所以不能越过 callable boundary，但 lambda 自身再进入 loop 后仍可合法 jump。
- jump 检查仍位于 expression checker，复用集中 diagnostic emitter，不增加 AST 或公开 typed API。

## 7. 实施计划

1. [x] 注册 L0142 并扩展 callable/loop context → 验证：catalog 与 checker 编译。
2. [x] 检查 break/continue target 与 callable boundary → 验证：领域正反矩阵。
3. [x] 补 Phase 2 pass/fail fixture → 验证：非零 fixture harness 与精确 Span。
4. [x] 同步事实并运行 workspace 基线 → 验证：五项标准命令实际退出 0。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0178`。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | jump context、L0142、测试、fixture 与事实文档 | `fix(frontend): check lexical jump targets (SPEC-0178)` |

## 9. 未决问题

- 无语言语义未决项；`for` 的 iterator/binding typed facts 继续由后续独立 Spec 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | v0.18/v0.28 已明确最近词法 loop、callable boundary 与无 label；当前 checker 只赋 `Nothing`，未检查目标 |
| `cargo test -p lang-frontend --test diagnostic_model --test type_checking --test ownership_checking` | 通过 | 53 项；覆盖 L0142 catalog、正反领域矩阵、Phase 2 fixture 与 Phase 3 相邻回归 |
| `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 | callable/loop context 实现无 warning；未扩大已超限的 typed model 文件 |
| 2026-08-25 workspace 标准基线（均设置 LLVM prefix） | 通过 | fmt、check、Clippy `-D warnings`、all-targets test、`lang-cli` build 均退出 0；codegen 门禁测试同步改为接受更早的 L0142 frontend 拒绝 |
