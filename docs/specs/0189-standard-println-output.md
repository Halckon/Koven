# SPEC-0189：标准 `println(String)` 与最小 stdout 输出

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P5-189` |
| 所属 Phase | Phase 5 |
| 语言规范 | 现行 [callable 参数契约](../guide/05-grammar-calls-lambda.md#63-phase-2-调用检查契约) 与 [Phase 5 roadmap](../guide/06-roadmap.md#phase-5最小标准库) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs，尝试验证 hello world 程序”的站立授权 |
| 前置 Spec | SPEC-0039、0042、0043、0184 `done` |
| 前置 ADR | ADR-0010、ADR-0012 `accepted` |
| 阻塞项 | 无；guide 已固定 `println(value)` 的单 Borrow 参数，并把可打印类型集合交给 Phase 5 |
| 影响范围 | `lang-frontend` 标准环境/effect、`lang-codegen` SSA/LLVM stdout runtime、`lang-std` bootstrap、Architecture/Roadmap |
| 语言语义变更 | 否；只发布 guide 已预留的首个 String 重载，不改变一般 String 表示或 IO 模型 |

## 2. Goal

完成后，标准分析环境发布唯一 `println(value: String): Unit` Borrow 重载；非插值 String literal
可经 compiler-bound typed identity lower 为确定的 UTF-8 字节加单个 LF，通过系统 stdout 写出。
真实 `lang-std` Koven entry 必须生成、链接、运行并由父进程精确观察 `Hello, World!\n`。

## 3. 范围与需求

- 标准环境按确定顺序在 `error` 后发布 `println`；effect 只能绑定到单个 Borrow String 参数、
  Unit 返回的外部 identity。同名源码函数不得获得 stdout effect。
- call 继续使用普通名称、overload、参数映射和调用期 Borrow 规则；typed descriptor 显式携带
  stdout effect，后端不得按名称或文本猜测。
- 当前一般 String runtime 尚未定义，只接受无 interpolation、无 lexer/parser error 的 String
  literal。按 lexer 已接受的转义集合解码为 UTF-8 bytes，随后追加一个 ASCII LF；空文本、Unicode、
  `\\`、`\"`、`\n`、`\r`、`\t`、`\0`、`\$` 都必须精确测试。
- SSA 增加无结果、无 owner operand 的 `PrintLiteral` operation；verifier 锁定其非空最终输出必须
  恰好以 LF 结尾。LLVM 为每个 operation 创建 private constant bytes，通过 C `write` 写 fd 1；
  返回字节数不匹配时进入既有 abort，不引入 String heap ABI、stdio buffering 或 unwind。
- object/link/run 测试验证 LLVM 声明/常量、无 malloc/retain/clone，以及真实 stdout 的精确字节。
  写盘前的 frontend/lowering 失败不得留下 object。

## 4. 非目标

- 不实现插值、连接、变量/参数 String 表示、`print`、其他 println 重载、stderr/stdin、File、
  BufferedReader、网络、格式化协议或可恢复 IO error。
- 不实现公开 `kovenc build/run`、入口命名、项目布局或多文件 import；由后续独立 Spec 承接。
- 不改变 `error()` 的 abort/message 边界，不把目标平台 IO 细节泄漏到 frontend。

## 5. 验收标准

- [x] 标准环境、typed call、Borrow 所有权与同名源码 shadowing 正反矩阵通过。
- [x] verified SSA/LLVM 覆盖空串、ASCII、Unicode、全部合法 escape、NUL 与多个 println 的源码顺序；
      corrupt operation/effect 被拒绝。
- [x] 真实 `.ko` entry 经 object/Clang link/run，stdout 精确等于 `Hello, World!\n`，stderr 为空、退出 0。
- [x] 插值和非 literal String 在写盘前结构化失败；IR 无 String heap、malloc/retain/clone/unwind。
- [x] Architecture/Roadmap/Spec 与 workspace 五项标准基线通过，并形成只含 SPEC-0189 的提交。

## 6. 技术方案

- 新增 `EnvironmentFunctionEffect::PrintLine` 与 `CallDescriptor::prints_line()`；沿用 Abort effect 的
  签名准入和稳定 external identity 模式。
- frontend lowering 在 call effect 分支中解析单个 literal 的 `StringPart::Text` spans，并在独立
  helper 中确定性解码；生成完整 bytes 后追加 LF，再建立 `Operation::PrintLiteral`。
- `RuntimeRequirements` 只在存在该 operation 时声明 `write` 和 abort；adapter 把 operation
  委托给 runtime，runtime 建 private constant、调用 write 并建立 success/abort CFG。
- CLI bootstrap 的内部执行边界捕获子进程结果；测试随后直接运行所生成 executable，精确断言
  真实标准源码 stdout，公开 CLI 保持不变。
- `llvm/adapter.rs` 原已超过 1000 行软上限；本 Spec 只增加 exhaustive operation dispatch，实际
  stdout/CFG 逻辑保留在 `runtime.rs`。新增 String 解码则提取为独立 67 行模块，使
  `lower_frontend.rs` 保持在 1000 行内；adapter 的整体拆分继续等待独立行为保持 Spec。

## 7. 实施计划

1. [x] 发布标准 identity/effect并锁定类型与所有权事实。
2. [x] 实现 literal 解码、SSA verifier/render 与 LLVM stdout runtime。
3. [x] 扩展真实 prelude 和 stdout 进程验收。
4. [x] 同步事实文档、运行基线、审查并提交。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 完整标准 println identity→SSA→LLVM→真实 stdout 闭环及文档 | `feat(std): add standard println output (SPEC-0189)` |

## 9. 未决问题

- 一般 String runtime、其余 printable 重载和公开 CLI build/run 仍是后续独立 Goal；本 Spec 不用
  字面量特例反向定义它们的 ABI。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | guide 已固定 Borrow；0039/0042/0043 提供 native/prelude/effect 先例；0184 已完成源码聚合闭环 |
| `cargo fmt --all -- --check` | 通过 | 全 workspace 格式基线 |
| `cargo check --workspace` | 通过 | 五个 member 均通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace` | 通过 | 全量通过；1 项既有 LLDB task-port 权限测试按设计 ignored |
| `cargo build -p lang-cli` | 通过 | `kovenc` debug target 构建成功 |
