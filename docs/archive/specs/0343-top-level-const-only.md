# SPEC-0343: 顶层运行时值的前端诊断

> **性质**：实施合同 · **状态**：done · **读取时机**：实现或验收顶层 `val`/`var` 拒绝时 · **唯一真源**：本片实施范围与验收；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-0343` |
| 所属 Phase | Phase 2（compilation-unit 类型检查） |
| 语言规范 | [Guide v0.46 顶层与关联命名空间的值声明](../../guide/05-declarations-callables.md#顶层与关联命名空间的值声明) |
| 批准依据 | 2026-10-22 用户同意 D4（v1 顶层只允许 `const val`）并要求按计划开始实施 |
| 基线 | SPEC-0342 之后的分支头（main `3b202f86` 之上） |
| 分支 | `feature/spec-0343-top-level-const-only` |
| 前置 Spec | [SPEC-0342](0342-companion-block-syntax.md)（仅分支顺序，无技术依赖） |
| 语言语义变更 | 否；落实已启用的 Guide v0.46 规则 |

## 1. Goal

compilation-unit 类型检查对顶层运行时 `val`/`var` 报 L0167，主 Span 精确覆盖关键字，消息给出
`const val` 与“在 `main` 中构造后传参”两条迁移路径；initializer 仍照常检查以支持恢复，但 unit
不能通过验证，因此不会进入 ownership 与 native。单文件与 project 共用该检查。

## 2. 范围

- 诊断目录新增 `L0167 UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE`。
- `check_top_level_initializer` 对 `Item::Variable` 先报 L0167，再检查 initializer。关键字 Span 由
  去掉修饰符后的 item 起点与关键字长度确定。
- Guide 05 写明诊断码。
- 依赖顶层运行时值作为测试夹具的前端测试：
  - 测试对象本身就是顶层 initializer 的类型检查时，保留源码，期望值改为同时出现 L0167；
  - 只是借顶层值做夹具时，改为函数参数、函数局部或 `const val`；
  - 主题已不成立的用例（Map consume 的 global-root、名称修复中的顶层 `val`）删除。

## 3. 非目标

- 旧的单文件 checker 自 PR #120 起已无生产调用者，不实现本规则；比较两者的测试单独标注差异。
- 不改 Parser；具名 `object` 与 companion body 中的普通值声明继续由 L0077 拒绝。
- 不实现静态不可变存储或任何全局状态。

## 4. 验收标准

- [x] 顶层 `val`、`var` 各报一次 L0167，Span 为关键字；`const val` 不受影响。
- [x] initializer 的类型事实仍发布，后续诊断（如 L0084、L0166、L0156）照常出现。
- [x] 含 L0167 的 unit 不能通过基础或常量验证。
- [x] 工程检查见 §5。

## 5. 实际验收（2026-10-22）

工具链：云端 Linux x86_64，Rust stable，LLVM 21.1.8，`CARGO_PROFILE_DEV_DEBUG=0`。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --locked --no-fail-fast --workspace` | 188 个测试目标中仅 `lang-cli` `unreadable_directory_and_source_report_io_operations` 失败；云端以 root 运行，目录权限不生效，属环境原因，与 SPEC-0342 记录一致 |
| `scripts/check_docs.py`、`gen_spec_dag.py`、`gen_roadmap.py` | 通过 |

受影响测试的处理：

| 测试 | 处理 |
|---|---|
| `diagnostic_model` 目录清单 | 加入 L0167 |
| `multifile_type_checking::initializers`（4 项） | 测试对象即顶层 initializer：期望加 L0167 并改为验证失败；companion 用例的辅助值改为函数 |
| `final_type_admission`（3 项） | 期望加 L0167；旧单文件 checker 不实现本规则，单独标注 |
| `multifile_constant_qualification`（2 项） | 普通顶层值只报 L0167；短路用例期望 L0167 + L0156 |
| `unit_recovery_this` | 顶层 `this` 用例期望 L0167 + L0166 |
| `unit_recovery_missing_names`、`ownership_map::consume` | 删除主题已不成立的顶层 `val` 修复用例与 global-root 用例 |
| `multifile_ownership_checking`（2 项）、`lang-cli` entry resolver、`lang-lsp` missing names | 夹具改为参数、函数局部、`object` 或函数 |
| `lang-codegen` runtime global（4 项） | 改为断言 Phase 2 L0167 且 unit 不能验证；声明式根仍生成目标文件 |

未运行：远端 CI、macOS、sanitizer。
