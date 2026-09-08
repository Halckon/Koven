# SPEC-0044：标准 `Pair` 与 `Result`

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P5-044` |
| 所属 Phase | Phase 5 |
| 语言规范 | 现行 [v0.29](../guides/v0.34-pre-restructure/00-index.md)；[`Pair` 解构与条件 `Copyable`](../guides/v0.34-pre-restructure/01-design-decisions.md#11-解构声明与-componentn-约定)、[`Result`](../guides/v0.34-pre-restructure/01-design-decisions.md#19-resultt-e-错误值与-postfix-v019-正式启用)、[constructor 契约](../guides/v0.34-pre-restructure/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029) |
| 批准依据 | 当前持续 Goal“继续推进guide和分阶段实施specs，先审计roadmap，再根据依赖图推进”的站立授权 |
| 前置 Spec | SPEC-0028、0035、0042、0183、0184、0185、0188 `done` |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0012](../../adr/accepted/0012-standard-library-bootstrap.md) `accepted` |
| 阻塞项 | 无；Roadmap 审计与 enum payload projection 回归已分别由 `efa0e64`、`3d340f2` 完成 |
| 影响范围 | `lang-std/koven/prelude.ko`、`lang-cli` bootstrap tests、Architecture/Roadmap |
| 语言语义变更 | 否；只实现现行 guide 已确定的标准声明和既有 compiler facts |

## 1. Goal

完成后，Koven 标准源码真源声明泛型 `Pair<A, B>` 与 `Result<T, E>`；仓库 bootstrap 能真实
构造、投影、复制/解构 `Pair<Int, Int>`，并构造、分支收窄和读取 `Result<Int, Int>` 的
`Ok(success)` / `Err(error)` payload。条件 `Copyable` 继续完全由现有类型/所有权规则推导。

## 2. 背景

SPEC-0042 已建立唯一 `prelude.ko` bootstrap，SPEC-0183/0188/0184 已完成泛型名义/enum 构造、
所有权与 native lowering。Roadmap 审计确认这些前置均已闭合；随后回归审计补齐了 smart-cast
enum payload projection 的 frontend fact 与 LLVM place lowering。因此本 Spec 不需要新增
compiler intrinsic，只需把现行 guide 的两个核心类型写入 Koven 标准源码并锁定真实行为。

## 3. 范围与需求

- 在唯一标准源码中声明 `value class Pair<A, B>(val first: A, val second: B)`。
- 在同一源码中声明 `enum class Result<T, E> { Ok(success: T), Err(error: E) }`；不得把硬关键字
  `value` 用作 payload 名称。
- 增加仓库 bootstrap entry，真实验证 `Pair<Int, Int>` 的构造、字段投影、复制与完整解构，
  以及 `Result<Int, Int>` 两个 case 的构造、smart cast 和 payload 投影。
- 测试从真实 `prelude.ko` 派生输入，证明含 MoveOnly 实参的 `Pair` / `Result` 按值移动后再次
  使用产生 L0131；不得复制标准声明到另一份测试真源。

## 4. 非目标

- 不实现 postfix `?` 的 Phase 2/4 传播语义、`map`/`fold` 等成员 API、异常或栈展开。
- 不实现 `Rc`、一般 String runtime、Map、容器 API、`for`、instance receiver 或接口委托。
- 不把 prelude 隐式拼接到任意公开单文件命令；跨文件标准库可见性等待 package/import 主线。
- 不新增 Rust 依赖、compiler intrinsic、runtime 表示或稳定错误码。

## 5. 验收标准

- [x] `prelude.ko` 是 `Pair` / `Result` 声明的唯一目标语言真源，字段/case/payload 名称精确。
- [x] 真实 bootstrap entry 经 frontend→verified SSA→LLVM object→Clang link/run 退出 0，覆盖
      `Pair` copy/destructuring/projection 与 `Result.Ok`/`Err` payload projection。
- [x] 基于真实 prelude 派生的两个 compile-fail 用例分别证明 MoveOnly `Pair` / `Result` 的
      move 后使用产生唯一 L0131，且 object/executable 均未生成。
- [x] `lang-std`、`lang-cli`、`lang-codegen` 受影响窄测与 workspace 五项标准基线通过。
- [x] Architecture/Roadmap/Spec 已同步为真实状态并创建独立 SPEC-0044 提交。

## 6. 技术方案与边界

保持 `prelude.ko` 为单一源码；bootstrap 测试读取该文件原文，运行标准 entry，或仅为负例在
临时文件末尾追加一个测试函数。编译器继续按通用 value class / enum 规则推导 copyability、
construction descriptor、ownership delivery 与 lowering，不注册 `Pair` / `Result` 名称特例。

## 7. 实施计划

1. [x] 增加两个标准声明与 native smoke entry → 验证：repository bootstrap 窄测。
2. [x] 增加从真实 prelude 派生的 MoveOnly compile-fail 矩阵 → 验证：L0131 与不落盘。
3. [x] 同步 Architecture/Roadmap/Spec → 验证：文档与实现一致。
4. [x] 运行 workspace 基线、审查并提交。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 标准声明、bootstrap 正反验收与事实文档 | `feat(std): add Pair and Result (SPEC-0044)` |

## 9. 未决问题

- 无。postfix `?` 与跨文件标准库可见性已明确排除，不影响本 Spec 的单文件 bootstrap 验收。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 Roadmap/前置审计 | 通过 | SPEC-0028/0035/0042/0183/0184/0185/0188 均 `done`；enum payload projection 回归已真实 native 验证 |
| `cargo test -p lang-cli bootstrap_tests::repository_prelude_is_the_single_enumerated_bootstrap_source_and_runs -- --exact` | 通过 | 唯一 prelude、标准声明与 Pair/Result native entry |
| `cargo test -p lang-cli bootstrap_tests::standard_pair_and_result_are_move_only_when_an_argument_is_move_only -- --exact` | 通过 | 两个派生负例各产生唯一 L0131 且不落盘 |
| `cargo test -p lang-std` / `-p lang-cli` / `-p lang-codegen` | 通过 | 受影响 member 全量窄基线；codegen 108 passed、1 ignored |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace` | 通过 | 五 member 检查通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace` | 通过 | 全量通过；1 项既有 LLDB task-port 权限测试按设计 ignored |
| `cargo build -p lang-cli` | 通过 | 公共 `kovenc` target 构建成功 |
