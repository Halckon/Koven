# SPEC-0355: abort 输出与 `kovenc run` 退出状态

> **性质**：实施合同 · **状态**：done · **读取时机**：实现或验收 abort 的 stderr 输出、`kovenc run` 退出状态时 · **唯一真源**：本片实施范围与验收；语义以 Guide 为准

> 原编号 SPEC-0347，2026-10-24 因与 main 撞号改为 0353，同日 main 又把 0353 给了 LSP 格式化，再改为 0355；分支名保持不变。

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0355` |
| 所属 Phase | Phase 4（SSA lowering）、Phase 5（LLVM 与 CLI） |
| 语言规范 | [Guide v0.46 Abort 输出合同](../../guide/09-nullability-errors.md#abort-输出合同)、[Guide 13 `kovenc run`](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 2026-10-22 用户同意 D9、D12 并要求按计划开始实施（路线图 C1） |
| 基线 | SPEC-0343 之后的分支头 `176221de` |
| 分支 | `feature/spec-0347-abort-output` |
| 前置 Spec | [SPEC-0343](0343-top-level-const-only.md)（仅分支顺序） |
| 语言语义变更 | 否；落实已启用的 Guide v0.46 规则，并补全 message 表 |

## 1. Goal

每次 abort 向 stderr 写

```text
abort: <message>
  at <logical-path>:<line>:<column>
```

写失败忽略，随后 `abort()`。`error(message)` 原样输出参数字节；编译器触发的 abort 使用 Guide
表中的固定文本，索引越界带运行时的 index 与 size。`kovenc run` 以子进程退出码结束，被信号终止时
以 `128 + 信号编号` 结束。

## 2. 设计

- SSA：`Terminator` 新增 `abort: Option<AbortReason>`，由 `Function::set_abort_terminator` 设置。
  `AbortReason` 为 `Error { message: LoanId }`、`CheckedArithmetic { operator, divisor }`、
  `CheckedConversion { target }` 与 `Text(String)`。未携带原因的旧路径输出 `abort: abort`。
- unit lowering 的生产 abort 点全部携带原因：`error()`、`!!`、checked 算术与转换、负长度容器
  构造，以及两处内部不变量（when 无分支命中、Map borrow 证明失败）。
- LLVM：可能 abort 的模块总是声明 `write`，并定义内部函数 `koven.abort.write`（逐段写 fd 2，
  短写重试、失败即停）、`koven.abort.write_i64`、`koven.abort.report` 与
  `koven.abort.report_index`。`abort_if`/`emit_abort` 必须给出 `AbortMessage`。
- 位置：adapter 在降级每条 instruction 与 terminator 前，用 debug plan 把 origin 换算为
  `  at <source-name>:<line>:<column>`；source name 即诊断使用的逻辑路径，列按 Unicode 标量计数。
  无 source map 的渲染路径省略第二行。drop 函数等非源码上下文同样省略。
- `/` 与 `%` 的 checked 失败在 abort 块内比较除数是否为零，选择 `division by zero` 或
  `integer overflow in /`/`%`。
- 区间元素访问先检查负 index，再做 size 换算，使负 index 也输出越界文本；区间构造先检查
  负端点，输出 `range out of bounds` 而不是 `negative size`。
- `map.get(k)!!`（Copyable）经 `MapResultUnwrap` 降级，其缺值边输出 `null assertion failed`，
  位置为整个断言表达式起点；该操作的其余生产者只解包编译器已证明存在的结果。
- `println` 直接调用 `write(1, …)`，没有用户态缓冲，因此“abort 前写出已交付的 stdout”自然成立。
- CLI：单文件与 project 的 `run` 共用 `child_exit_status`。

## 3. 非目标

- 旧 `lower_frontend` 路径的 abort 点不补原因（无生产调用者），输出通用文本。
- 不实现 `main` 返回 `Result`（L21）与 `exitProcess`（L6）。
- 线程相关 message 留给线程实现。

## 4. 验收标准

- [x] `error("…")` 原样输出 UTF-8 message，位置指向调用起点，先前 stdout 已写出。
- [x] `!!`、`+ - * / %` 溢出、除以零、checked 转换、List 正/负越界各输出 Guide 文本与位置。
- [x] 全部 abort 仍以 SIGABRT 结束。
- [x] `kovenc run` 对 abort 程序以 134 结束，stdout/stderr 原样转发。
- [x] Guide 09 message 表补全实际出现的固定文本。

## 5. 实际验收（2026-10-22）

工具链：云端 Linux x86_64，Rust stable，LLVM 21.1.8，`CARGO_PROFILE_DEV_DEBUG=0`。按用户要求只跑受影响目标。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo test -p lang-codegen --lib`（全部 2399 项） | 修正旧测试后通过；新增 `native_tests::abort_output_tests` 7 项 |
| `cargo test -p lang-cli --test native_cli --test range_project_cli --test project_cli --bin kovenc` | 除 root 环境下目录权限测试外通过 |
| `scripts/check_docs.py`、`gen_spec_dag.py`、`gen_roadmap.py` | 通过 |

旧测试调整：以桩替换 `abort` 的原生测试不再断言 stderr 为空，改为断言报告前缀；检查 IR 文本的
测试先剥离 `koven.abort.*` 辅助函数，原先断言 `call void @abort()` 的改为断言调用
`koven.abort.report`；`project_cli` 中 abort 程序的 `run` 退出码由 1 改为 134。

已知限制（后续）：`AbortReason` 引用的 loan/value 不在 terminator entities 中，SSA 支配与所有权
验证器看不到它们；当前构造保证其支配 abort 块。

未运行：工作区全量测试、远端 CI、macOS、sanitizer。
