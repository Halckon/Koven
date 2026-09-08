# SPEC-0043：标准 `error()` identity 与 abort 接线

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P5-043` |
| 所属 Phase | Phase 5 |
| 语言规范 | 现行 [v0.28 `error()` 契约](../guides/v0.34-pre-restructure/01-design-decisions.md#3-error-与空安全相关运算符) 与 [Phase 4/5 roadmap](../guides/v0.34-pre-restructure/06-roadmap.md#phase-4llvm-代码生成) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0039、SPEC-0042 `done`；SPEC-0019/0067/0029 的 `Nothing`、callable 与调用期所有权事实已完成 |
| 前置 ADR | [ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md)、[ADR-0012](../../adr/accepted/0012-standard-library-bootstrap.md) `accepted` |
| 阻塞项 | 无；现行 guide 已固定 `fun error(message: String): Nothing` 与 abort，ADR-0010 已要求标准环境稳定 identity；完整 String runtime/message 输出明确排除 |
| 影响范围 | `lang-frontend` 标准环境与 typed call effect；`lang-codegen` frontend→SSA Abort；`lang-cli`/`lang-std` bootstrap 运行验收；Architecture、roadmap |
| 语言语义变更 | 否；只实现既有标准函数 identity 和不可捕获 abort，不改变名称、签名、求值顺序或异常模型 |

## 2. Goal

完成后，标准分析环境发布不能由同名源码函数冒充的 `error(message: String): Nothing` identity，
其成功 typed call 携带 compiler-bound abort effect；当前本机源码子集可把使用非插值字符串字面量
的标准 `error()` 调用 lower 为既有 SSA/LLVM Abort，并由真实 `prelude.ko` 入口验证进程终止。

## 3. 范围与需求

### 3.1 标准环境与 typed identity

- `standard_environments()` 在 16 个既有 builtin type 后按固定顺序声明一个外部 `error` function，
  精确绑定一个无名称、`Borrow String` 参数和 `Nothing` 返回类型；不得把 `error` 加入关键字表。
- 扩展既有 compiler-bound function effect 表达 abort。环境构造时必须拒绝把该 effect 绑定到
  其他签名；成功 call 把 effect 复制到 `CallDescriptor`，后续阶段不得再读取源码 spelling。
- `error("message")` 使用普通 call mapping、类型和所有权规则；参数数量/类型错误继续使用现有
  callable 诊断。同名源码 callable 的 target 仍是 `Source(SymbolId)`，不获得 abort effect。

### 3.2 frontend→SSA Abort

- instance planning 不为标准 `error` 建立源码 function instance；expression lowering 只根据
  typed call effect 选择已有 `TerminatorKind::Abort`，并返回 diverged，不生成 direct call 或
  unwind cleanup。
- 当前 source→native 子集尚无 String runtime representation。本 Spec 只把不含 interpolation 的
  String literal 作为无额外可观察求值的标准 error message 接入 Abort；其他 String expression
  继续返回既有 `UnsupportedSource`，不得伪造 String ABI、忽略 interpolation 或扩大为一般优化。
- 相同文本但 target 为源码函数、普通返回 `Nothing` 的函数或没有 compiler-bound effect 的外部
  函数均不得被改写为 Abort。

### 3.3 真实标准源码验收

- 保留 `bootstrapSmoke(): Unit` 正常入口，并在同一磁盘 `prelude.ko` 增加只调用标准
  `error("...")` 的显式 abort smoke entry；不在 Rust 中复制目标程序行为。
- CLI bootstrap 测试分别选择两个 resolved source entry：正常入口退出 0；abort 入口生成
  object/executable 后以 signal 或非零状态结束。输入源码保持不变，失败仍归类为 process failure。

## 4. 非目标

- 不实现 String heap/storage ABI、插值、连接、message 输出、`println`、IO、`!!`/`?` codegen 或
  一般 String expression lowering；不承诺 abort message 可观察。
- 不实现公开 `kovenc build`、隐式 `main`、多文件 prelude/import、package、manifest 或安装布局。
- 不按函数名、参数文本、声明顺序或返回 `Nothing` 猜测 Abort；不允许用户源码声明
  compiler-bound effect。
- 不实现 Pair/Result、Box/Rc、容器 API、析构接线、异常、unwind 或 panic runtime。

## 5. 验收标准

- [x] 标准环境按确定顺序发布 16 个 builtin type 与唯一 `error` function；签名和 Abort effect
      可查询，错误 effect/signature 组合 fail-loud。
- [x] frontend compile-pass 证明标准 `error` 参与普通 Borrow call、产生 `Nothing` bottom typing
      和 typed Abort effect；参数错误保留现有诊断 code/Span。
- [x] 同名源码函数与无 effect 的外部 `Nothing` function 均不获得 Abort，重复分析结果确定。
- [x] canonical `error("message")` 经 verified SSA 产生 source-anchored Abort、LLVM `abort` +
      `unreachable`，不生成同名 direct call、landing pad 或 String ABI。
- [x] 非插值之外的 String message 和伪造/missing effect 结构化失败，不输出 object；正常标量 call
      和 checked arithmetic Abort 回归不变。
- [x] 真实 `prelude.ko` 的正常 entry 退出 0，abort entry 由进程失败边界观察到非零/signal；源码
      和调用方拥有的输入均不被修改。
- [x] 受影响 crate 窄测与 workspace 五项基线通过；Architecture、roadmap、Spec 状态和验证记录
      只描述实际完成事实。

## 6. 技术方案与边界

- 在 `EnvironmentFunctionEffect` 与 `CallDescriptor` 增加最小 Abort fact；标准环境仍是每次编译
  独立创建的 owner，不增加 singleton、全局状态或名称特判。
- lowering 在现有 `lower_call` 内先检查 typed Abort effect，再验证本 Spec 的 message expression
  边界并设置现有 SSA terminator；LLVM adapter 和 C runtime symbol保持不变。
- frontend unit/integration tests 锁定 identity、call fact、诊断和 shadowing；codegen 测试锁定
  SSA/LLVM/object；CLI 测试继续使用真实磁盘 prelude 与显式 entry。

## 7. 实施计划

1. [x] 发布标准 `error` environment identity 与 typed Abort effect → 验证：环境、call mapping、
   bottom typing、shadowing 和错误绑定窄测。
2. [x] 把 canonical standard error call lower 到既有 SSA Abort → 验证：SSA/LLVM 文本、unsupported
   message、非 intrinsic 同名 call 与失败不落盘。
3. [x] 扩展真实 prelude abort entry 与进程验收 → 验证：正常 0、abort 非零/signal、输入不变。
4. [x] 同步 Architecture/roadmap/Spec，运行 workspace 基线并审查 staged diff。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 标准环境/typed Abort fact 与 frontend→SSA/LLVM 接线 | `feat(codegen): lower standard error to abort (SPEC-0043)` |
| 2 | 真实 prelude 进程验收、Architecture 与完成记录 | `feat(std): verify standard error abort (SPEC-0043)` |

## 9. 未决问题

- 无。完整 String representation 是后续独立 runtime/ABI 决策；本 Spec 不通过占位值提前固定它。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | v0.28 已固定签名、Borrow 参数、Nothing 与 abort；ADR-0010 已固定 stable identity→SSA Abort 边界；0039/0042 已有真实 Abort backend 与磁盘 prelude bootstrap |
| `cargo clippy -p lang-frontend -p lang-codegen --all-targets -- -D warnings` | 通过 | frontend/codegen 实现切片零 warning |
| `cargo test -p lang-frontend --all-targets` | 通过 | 标准环境、callable mapping、shadowing、错误签名与既有 frontend 回归全部通过 |
| `cargo test -p lang-codegen --all-targets` | 通过 | 95 项测试通过；含 SSA/LLVM Abort、插值拒绝与 object 不落盘 |
| `cargo test -p lang-cli repository_prelude_is_the_single_enumerated_bootstrap_source_and_runs --bin kovenc` | 通过 | 真实 prelude 正常入口退出 0；abort 入口被归类为进程失败，object/executable 均生成且源码未变 |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace --all-targets` | 通过 | workspace 所有 target 检查通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 零 warning |
| `cargo test --workspace --all-targets` | 通过 | 全部测试通过，无失败或 ignored |
| `cargo build -p lang-cli` | 通过 | `kovenc` 构建成功 |
