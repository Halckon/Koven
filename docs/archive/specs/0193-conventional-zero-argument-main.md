# SPEC-0193：单文件零参数 conventional `main`

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-193` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [`guide/01-design-decisions.md` §30.1](../guides/v0.34-pre-restructure/01-design-decisions.md#301-单文件-conventional-main) |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据依赖图推进”的站立授权 |
| 前置 Spec | SPEC-0190 `done` |
| 前置 ADR | ADR-0010、ADR-0012 `accepted` |
| 关联 ADR | 无；复用既有显式 native entry 与 C ABI main wrapper |
| 阻塞项 | 无；v0.30 已生效，参数化 main 明确后置 |
| 影响范围 | `lang-cli` 参数/entry 选择、CLI 集成测试、Architecture/Roadmap |
| 语言语义变更 | 否；实施 v0.30 已批准语义 |

## 2. Goal

公开以下零参数默认入口，并保持显式 entry 完全兼容：

```text
kovenc build <source.ko> -o <executable>
kovenc run <source.ko>
```

省略 `--entry` 时选择单文件中唯一合法的顶层 `fun main(): Unit`，继续复用现有
frontend→verified SSA→LLVM object→Clang link/run 主线。

## 3. 范围与需求

- `build` 只新增固定顺序 `<source> -o <executable>`；`run` 只新增单个 `<source>` 形式。
- 显式 `--entry <name>` 继续覆盖 conventional lookup，行为和错误保持 SPEC-0190 契约。
- conventional lookup 只执行顶层、非泛型、零参数、精确返回 `Unit` 的 `main`；已由 v0.30
  定义但等待 SPEC-0194 的参数化形状必须识别为独立 unsupported failure，不能误报非法签名。
- 没有同名函数、只有非法形状、多个合法零参数候选分别产生 missing、invalid-shape、ambiguous
  operational failure，不分配语言诊断码。
- frontend 诊断必须先于 entry selection 返回，human/JSON Lines 路径保持不变。

## 4. 非目标

- 不实现 `main(args: Array<String>)`、argv、默认输出名、任意参数排列、多 source 或项目入口。
- 不改变 codegen 的显式 resolved `FunctionId` API、C ABI wrapper 或语言诊断 schema。
- 不让 `main` 成为关键字，也不跨 package 搜索。

## 5. 验收标准

- [x] public build/run 可省略 `--entry` 编译并运行仓库外 `fun main(): Unit` Hello World。
- [x] missing、invalid-shape 和 ambiguous conventional main 有互不混淆的 operational error。
- [x] 显式 entry 覆盖默认选择，既有 SPEC-0190 CLI 测试保持通过。
- [x] 参数化 main 以独立 unsupported failure 精确拒绝，未构造 argv 或宿主字符串后门。
- [x] `cargo test -p lang-cli --test native_cli`、workspace 基线和文档一致性检查通过。
- [x] Architecture 已更新为实现后的事实。

## 6. 技术方案与边界

CLI 参数解析把 entry 选择表达为 `Explicit(name)` 或 `ConventionalMain`，由 bootstrap 在完整
frontend 成功后使用 typed callable facts 选择 `SymbolId`。codegen 仍只接收已解析、已验证的
零参数 entry，不按名称猜测；默认 lookup 因而不会泄漏到 SSA/LLVM。

## 7. 实施计划

1. [x] 建立显式/conventional entry selection 并锁定三类 operational failure。
2. [x] 扩展固定 CLI 参数形状与真实进程正反测试。
3. [x] 同步 Architecture、Roadmap、验收记录并运行 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 默认 main 选择、CLI 正反测试和事实文档 | `feat(cli): add conventional zero-argument main (SPEC-0193)` |

## 9. 未决问题

- 无。参数化 main 已明确交给 SPEC-0194，不构成本 Spec 未决项。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 前置审计 | 通过 | SPEC-0190 `done`，v0.30 已由提交 `e28fc06` 启用 |
| `cargo test -p lang-cli --test native_cli` | 通过 | 4 项；默认/显式 entry、三类选择失败与真实 Hello World |
| `cargo test -p lang-cli` | 通过 | 20 unit + 7 CLI integration tests |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace` | 通过 | 五个 member 均通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace` | 通过 | 全量通过；1 项既有 LLDB task-port 权限测试按设计 ignored |
| `git diff --check` 与相对 Markdown 链接检查 | 通过 | 无空白错误或失效相对链接 |
