# Koven Spec 路线图

本目录依据现行 [v0.19 语言规范](../guide/00-index.md) 维护可独立验证、可独立
提交的 Goal；已完成 Spec 保留其实施时适用的 guide 引用。路线图负责排序，Spec 文件负责
定义一次交付；路线图条目本身不等于已批准的 Spec，也不授权实现。

[v0.19](../guide/00-index.md) 已由用户同意前述完整错误值契约并要求继续实施，取代 v0.18；
v0.12、v0.13 内容已合入 v0.14。
SPEC-0010、SPEC-0011、SPEC-0012、SPEC-0013、SPEC-0014、SPEC-0015、SPEC-0016、SPEC-0062、SPEC-0063 已完成；尚未物化的条目仍只是候选 Goal，不因编号预留而
自动获得实现授权。

## Goal 与提交工作流

Spec 进入 `in-progress` 前必须已有单份明确确认或有效站立授权作为批准依据，并满足所有前置
Spec `done`、前置 ADR `accepted` 和阻塞项解除。存在有效站立授权时，可以在同一工作流中
按逻辑顺序完成 `approved → in-progress`，不要求中间状态形成独立提交。

后续使用 Goal 时固定按以下顺序推进：

1. 以 `完成 SPEC-NNNN：〈Goal〉；按本 Spec 验收并创建独立提交` 作为 Goal objective。
2. 把 Spec 标为 `in-progress`，按“实施计划”逐项执行；每一步先跑窄检查。
3. 完成测试后运行 Spec 要求的 workspace 基线，并同步 Architecture 当前事实。
4. 填写验证记录、逐项勾选验收，把 Spec 标为 `done`。
5. 只暂存该 Spec 范围，检查 staged diff 后提交；提交信息必须包含 `(SPEC-NNNN)`。
6. 提交成功后再把 Goal 标记为完成。未提交、验收缺失或检查未运行时不得完成 Goal。

一个提交不得混合两个 Spec。一个 Spec 可以有多个提交；含 Rust / 目标语言实现的提交必须
可构建、可测试，workspace 建立前的纯文档提交执行适用的链接、术语和 diff 检查。所有提交
都引用同一 Spec 编号；最终实现提交同时包含验收记录、Architecture 更新和 `done` 状态。
若一个 Spec 无法形成清晰的独立提交边界，应在批准前继续拆分。

Spec 草案、批准和 `in-progress` 状态不要求分别提交；最终实现提交仍必须独占一个 Spec，
并包含 `done` 状态、实际验收记录和 Architecture 更新。新 ADR 的决策正文仍应形成独立文档
提交，但可在首次提交时直接为 `accepted`，不得把 ADR 与依赖它的实现混入同一提交。
简化的是人工确认和重复状态文书，不是行为验收；任何检查只有实际成功后才能记录为通过。

## Phase 0 Spec 队列

Phase 0 已细化为以下实际 Spec。状态以各 Spec 文件为准，并按前置关系依次推进：

| 顺序 | Spec | Goal | 前置条件 |
|---|---|---|---|
| 1 | [SPEC-0001](./0001-bootstrap-cargo-workspace.md) | 建立可检查的五 member Cargo workspace | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) accepted |
| 2 | [SPEC-0002](./0002-source-span-foundation.md) | 建立统一 source / `Span` 基础设施 | SPEC-0001 done；[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 3 | [SPEC-0003](./0003-structured-diagnostics.md) | 建立稳定、确定性的结构化诊断核心 | SPEC-0002 done；[ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 4 | [SPEC-0004](./0004-indexed-ast-foundation.md) | 建立保留 `Span` 的索引式 AST 基础 | SPEC-0001、0002 done |
| 5 | [SPEC-0005](./0005-language-fixture-harness.md) | 建立真实枚举 `.ko` 文件且零用例失败的 harness | SPEC-0001、0002、0003、0004 done |

Phase 0 的建议依赖关系：

```text
SPEC-0001
    └── SPEC-0002
            ├── SPEC-0003 ──┐
            └── SPEC-0004 ──┴── SPEC-0005
```

## 后续 Spec 候选

下表预留编号、Phase、单一 Goal 和依赖门槛。没有文件链接的候选项尚不是 Spec；只在前置
Phase 接近完成、适用 guide 已明确且必要 ADR 已接受时，才从模板创建文件并补齐可执行
计划。已有链接但尚未批准或仍带阻塞项的文件继续保持 `draft`，不得据此实施。这能避免
长期空壳 Spec 与实现事实漂移。

### Phase 1：Lexer / Parser

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0006](./0006-deterministic-lexer.md) | Lexer 覆盖字面量、标识符、关键字和 trivia（`done`） | 0002、0003、0005 `done`；v0.5 已生效 |
| [0007](./0007-pratt-expression-parser.md) | Pratt parser 覆盖完整表达式优先级（`done`） | 0004、0006 `done`；v0.6 已生效；站立授权已记录 |
| [0008](./0008-declaration-parser.md) | 解析 `val` / `var` / `const val`、函数、泛型与调用点类型实参（`done`） | 0007 `done`；v0.7 已生效；站立授权已记录 |
| [0009](./0009-block-statement-parser.md) | 解析 block / statement 序列与函数 block body（`done`） | 0008 `done`；v0.8 已生效；站立授权已记录 |
| [0010](./0010-lambda-literal-parser.md) | 解析 lambda literal（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0011](./0011-implicit-unit-return.md) | 解析具名函数省略返回标注时的隐式 `Unit`（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0012](./0012-callable-parameter-and-call-argument-parser.md) | 解析统一 callable 参数 marker、typed call argument、命名实参与调用点 `borrow` / `&` 模式（`done`） | 0010、0011 `done`；v0.14 已生效；站立授权已记录 |
| [0013](./0013-local-val-destructuring-parser.md) | 解析 block / lambda body 内局部 `val` 解构（`done`） | 0012 `done`；适用 guide 已启用；站立授权已记录 |
| [0014](./0014-complete-file-parser.md) | 组合 0007–0013 已有节点为完整文件并实现声明分隔、跨声明恢复与级联抑制（`done`） | 0011、0013 `done`；v0.15 已封闭完整文件恢复契约；站立授权已记录 |
| [0062](./0062-top-level-declaration-separators.md) | 按 v0.16 修正顶层声明换行 / 分号分隔（`done`） | 0014 `done`；v0.16 已生效；当前持续 Goal 的站立授权 |
| [0015](./0015-package-import-parser.md) | 解析 `package` / Kotlin 风格 `import`（`done`） | 0014、0062 `done`；v0.17 已生效；当前持续 Goal 的站立授权 |
| [0016](./0016-control-flow-parser.md) | 解析 `if` / `when` / `super`、loop-family 与 jump 控制流（`done`） | 0009、0014 `done`；v0.18 已生效；当前持续 Goal 的站立授权 |
| [0063](./0063-postfix-error-propagation-parser.md) | 解析 postfix 错误传播 `?`（`done`） | 0016 `done`；v0.19 已生效；当前持续 Goal 的站立授权 |
| 0017 | 解析 `value class` / `class` / `interface` / `enum class` / `object` / `companion object` | 0009 `done`；先由后续 guide 明确定义 |

### Phase 2：名称与类型检查

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0018 | 完成单文件声明收集、作用域和名称诊断 | 0014 |
| 0019 | 检查基础类型、局部推导、隐式 `Unit` / 显式返回类型与 `Nothing` | 0018 |
| 0020 | 检查泛型及 class / interface / enum / value class 名义类型 | 0019、0017 |
| 0021 | 实现 `when` 穷尽性与 smart cast | 0020、0016 |
| 0022 | 推导条件 `Copyable` 并检查结构化解构类型 | 0019、0020 |
| 0023 | 检查顺序容器的名义类型、元素可存储性、核心构造和索引 place 类型 | 0020、0022；v0.6 生效 |
| 0024 | 检查 `Map` / `MutableMap` 的 key 契约、value 所有权约束和查询结果类型 | 0020；新 guide 明确 key 等价关系、返回所有权与修改 API |
| 0025 | 建立多文件 package / import 名称解析 | 0015、0018；package 映射 ADR |
| 0026 | 检查 `object` / `companion object` | 0020、0017；先由新 guide 明确成员函数限制 |

### Phase 3：所有权与借用

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0027 | 建立变量所有权状态并检测 use-after-move | 0019、0020 |
| 0028 | 实现条件复制、移动与消费式解构检查 | 0022、0027 |
| 0029 | 检查 `Value` / `Borrow` / `Inout` 契约、调用点 `borrow` / `&` 冲突并确定 ASAP 析构点 | 0028；新 guide 明确借用与析构规则 |
| 0030 | 检查顺序容器元素 place 的读取、借用、替换与析构所有权规则 | 0023、0029；v0.6 生效 |
| 0031 | 检查 `Map` / `MutableMap` 查询和修改的 key / value 所有权规则 | 0024、0029；新 guide 明确完整 Map 契约 |
| 0032 | 检查 move closure 与 `Transferable` | 0020、0029；适用 guide 明确标记能力推导 |

### Phase 4：SSA、LLVM 与原生 AOT

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0033 | 实现最小 typed SSA IR 与 verifier | 0021、0029；接受 SSA ADR |
| 0034 | 把标量表达式和控制流 lower 到 LLVM | 0033；接受 LLVM / target ADR |
| 0035 | 生成聚合、class 分配和显式 drop / free | 0034、0029；接受 runtime ABI ADR |
| 0036 | 生成顺序容器的单一连续缓冲区基元、边界检查和 drop 路径 | 0023、0030、0035；接受 runtime ABI ADR |
| 0037 | 生成 `Map` / `MutableMap` 查询与修改的 runtime 基元 | 0024、0031、0035；接受 runtime ABI ADR、Map 存储策略 ADR |
| 0038 | 生成捕获闭包环境和无捕获函数指针 | 0034、0032 |
| 0039 | 生成 object、链接 `main` 并把 `error()` 映射到 abort | 0035、0038；接受 linker 决策 |
| 0040 | 生成 DWARF 并用首个支持平台的调试器验收 | 0039；接受 debug mapping ADR |
| 0041 | 提供用户可见 `extern` FFI | 0039；新 guide 定义 FFI 与所有权边界，非 v1 主路径 |

### Phase 5：最小标准库

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0042 | 用编译器构建并运行 `lang-std` 目标语言源码 | 0039；接受 bootstrap / runtime ADR |
| 0043 | 实现 prelude、基础操作和 `error()` | 0042 |
| 0044 | 实现条件可复制的 `Pair` 与 `Result` | 0042、0028、0035 |
| 0045 | 实现独占 `Box` 与共享 `Rc` 所有权类型 | 0042、0028、0035 |
| 0046 | 提供 Array / List / MutableList 的目标语言公共 API 与顺序算法 | 0036、0043、0045 |
| 0047 | 提供 Map / MutableMap 的目标语言公共 API 与键值算法 | 0037、0043、0045；新 guide 明确完整 Map 契约 |
| 0048 | 为顺序容器实现 `map` / `filter` / `reduce` / `forEach` | 0046、0038 |
| 0049 | 实现同步 File / BufferedReader / 标准流 | 0043、0039 |
| 0050 | 实现 thread / channel | 0042、0032 |
| 0051 | 实现目标语言测试发现与断言 runner | 0042；新 guide 定义最小 `@Test` 语法 |

### Phase 6：工具链

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0052 | 定义并解析最小 `project.toml` | 0025；接受 package schema ADR |
| 0053 | 实现依赖解析与确定性 `project.lock` 核心 | 0052；接受解析 / 锁定策略 ADR |
| 0054 | 由 package CLI 编排 manifest、解析与锁定 | 0053 |
| 0055 | 让 LSP 发布 frontend 诊断 | 0025、0021、0003 |
| 0056 | 让 LSP 支持跳转定义 | 0055、0025、0021 |
| 0057 | 实现稳定、幂等的格式化器 | 0014、0006 |
| 0058 | 提供 TextMate grammar 与回归 fixture | 0014、0015 |
| 0059 | 提供 Tree-sitter grammar 与 corpus | 0014、0015 |
| 0060 | 提供版本化机器可读诊断协议 | 0003、0055；接受协议 ADR |
| 0061 | 构建首个支持平台的 compiler + stdlib 发行包 | 0040、0042–0051、0054；接受发布矩阵 ADR |

增量编译不预留在 Phase 0–6 主链中。它依赖稳定 package identity、package lock、SSA 和依赖
图；推荐在 SPEC-0054 完成后另建 Phase 6+ Spec，并先接受缓存键与失效策略 ADR。

现行 v0.19 沿用 v0.14 已确定的规则：v1 的 `Transferable` 与 `Copyable` 一样由编译器结构化自动推导，不开放
手动实现；标准库并发类型的例外由后续实施 Spec 逐项锁定，`Shareable` 连同跨线程共享原语
延后到 v2。这是已批准但尚待 Phase 3 实施的规则，不属于下列未决推荐。

## 未决决策的推荐方向

以下是起草后续 guide / ADR 时的默认推荐，不是已经接受的决策；触及对应 Spec 前仍需正式
文档批准。

| 决策 | 推荐方案 | 需要的权威文档 |
|---|---|---|
| `object` / `companion object` | 编译期限制只约束存储状态和初始化式；成员函数体可使用普通 v1 代码，但不能读取或修改运行时单例状态 | 新 guide |
| 借用与析构 | v1 借用只存在于一次调用的动态期间，不允许存储或返回；ASAP 析构以所有权检查标出的最后一次合法使用为准，分支合流采用保守点 | 新 guide |
| `lang-std` bootstrap / runtime | `.ko` 标准库保持独立真源；最小 ABI 支撑先收敛在 codegen 的私有 runtime 边界，证明需要独立发布后再提新增 crate 的 ADR | ADR |
| SSA | 采用 typed SSA + block parameters，显式表达 move / drop；用 verifier 锁定类型、CFG 与所有权不变量 | ADR |
| LLVM / target / linker | 固定一组经兼容矩阵验证的 LLVM major 与 `inkwell` feature；先支持单一 host target，再扩展 CI 矩阵 | ADR |
| FFI | 不放入 v1 主交付路径；待内部 ABI 稳定后，以受限 C ABI 和显式 `unsafe` / 所有权边界起步 | 新 guide + ADR |
| `@Test` | v1 只定义编译器保留的最小 `@Test`，不顺带实现通用运行时注解或反射 | 新 guide |
| 机器诊断 | 人类可读诊断走 stderr；机器模式使用带 schema version 的 JSON Lines，稳定字段由后续协议 ADR 固定 | 后续 ADR |
| package / lock | `project.toml` 只保留 package、target、dependency 最小字段；`project.lock` 完全由工具生成并确定性排序 | ADR |
| 首发平台 | 先验收开发主机 `aarch64-apple-darwin`，再增加一个 Linux CI target；跨平台承诺以发行 ADR 为准 | ADR |

## 路线图维护规则

- guide 改变 Phase 或语言语义时，先更新 guide，再调整尚未批准的路线图候选。
- 已批准或已完成 Spec 不因路线图重排而改号；需要替代时使用 `superseded` 并建立双向链接。
- ADR 只决定 guide 留白处的长期方案；路线图中的“推荐”不能替代 accepted ADR。
- Architecture 只描述已经落地的事实，不复制本页计划。
