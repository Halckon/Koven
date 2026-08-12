# Koven Spec 路线图

本目录把现行 [v0.4 语言规范](../agent-language-design-guide-v0.4.md) 拆成可独立验证、可独立
提交的 Goal。路线图负责排序，Spec 文件负责定义一次交付；路线图条目本身不等于已批准的
Spec，也不授权实现。

## Goal 与提交工作流

只有同时满足以下条件的 Spec 才能进入 `in-progress`：状态已经是 `approved`、所有前置
Spec 均为 `done`、所有前置 ADR 均为 `accepted`、阻塞项已经解除。

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

下表预留编号、Phase、单一 Goal 和依赖门槛。候选项尚不是 Spec 文件；只在前置 Phase 接近
完成、适用 guide 已明确且必要 ADR 已接受时，从模板创建对应文件并补齐可执行计划。这能
避免长期空壳 Spec 与实现事实漂移。

### Phase 1：Lexer / Parser

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0006 | Lexer 覆盖字面量、标识符、关键字和 trivia | 0002、0003、0005 |
| 0007 | Pratt parser 覆盖完整表达式优先级 | 0004、0006 |
| 0008 | 解析变量、函数、泛型与函数类型声明 | 0007 |
| 0009 | 解析控制流及 class / interface / enum / object 结构 | 0008 |
| 0010 | 解析 lambda、索引、调用参数模式与解构 | 0007、0008 |
| 0011 | 实现错误恢复并完整解析 Phase 1 范例 | 0009、0010 |
| 0012 | 解析 `module` / `import` | 0008；先由新 guide 定义语法 |

### Phase 2：名称与类型检查

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0013 | 完成单文件声明收集、作用域和名称诊断 | 0011 |
| 0014 | 检查基础类型、局部推导、显式返回类型与 `Nothing` | 0013 |
| 0015 | 检查泛型及 class / interface / enum / value class 名义类型 | 0014 |
| 0016 | 实现 `when` 穷尽性与 smart cast | 0015 |
| 0017 | 推导条件 `Copyable` 并检查结构化解构类型 | 0014、0015 |
| 0018 | 建立多文件 module / import 名称解析 | 0012、0013；新 guide + module 映射 ADR |
| 0019 | 检查 `object` / `companion object` | 0015；先由新 guide 明确成员函数限制 |

### Phase 3：所有权与借用

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0020 | 建立变量所有权状态并检测 use-after-move | 0014、0015 |
| 0021 | 实现条件复制、移动与消费式解构检查 | 0017、0020 |
| 0022 | 检查 `borrow` / `inout` / `own` 冲突并确定 ASAP 析构点 | 0021；新 guide 明确借用与析构规则 |
| 0023 | 检查 move closure 与 `Shareable` / `Transferable` | 0015、0022；新 guide 明确标记能力推导 |

### Phase 4：SSA、LLVM 与原生 AOT

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0024 | 实现最小 typed SSA IR 与 verifier | 0016、0022；接受 SSA ADR |
| 0025 | 把标量表达式和控制流 lower 到 LLVM | 0024；接受 LLVM / target ADR |
| 0026 | 生成聚合、class 分配和显式 drop / free | 0025、0022；接受 runtime ABI ADR |
| 0027 | 生成捕获闭包环境和无捕获函数指针 | 0025、0023 |
| 0028 | 生成 object、链接 `main` 并把 `error()` 映射到 abort | 0026、0027；接受 linker 决策 |
| 0029 | 生成 DWARF 并用首个支持平台的调试器验收 | 0028；接受 debug mapping ADR |
| 0030 | 提供用户可见 `extern` FFI | 0028；新 guide 定义 FFI 与所有权边界，非 v1 主路径 |

### Phase 5：最小标准库

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0031 | 用编译器构建并运行 `lang-std` 目标语言源码 | 0028；接受 bootstrap / runtime ADR |
| 0032 | 实现 prelude、基础操作和 `error()` | 0031 |
| 0033 | 实现条件可复制的 `Pair` 与 `Result` | 0031、0021、0026 |
| 0034 | 实现独占 `Box` 与共享 `Rc` 所有权类型 | 0031、0021、0026 |
| 0035 | 实现 Array / List / MutableList 顺序容器 | 0032、0034 |
| 0036 | 实现独立契约的 Map / MutableMap | 0032、0034 |
| 0037 | 实现 `map` / `filter` / `reduce` / `forEach` | 0035、0036、0027 |
| 0038 | 实现同步 File / BufferedReader / 标准流 | 0032、0028 |
| 0039 | 实现 thread / channel | 0031、0023 |
| 0040 | 实现目标语言测试发现与断言 runner | 0031；新 guide 定义最小 `@Test` 语法 |

### Phase 6：工具链

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0041 | 定义并解析最小 `project.toml` | 0018；接受 package schema ADR |
| 0042 | 实现依赖解析与确定性 `project.lock` 核心 | 0041；接受解析 / 锁定策略 ADR |
| 0043 | 由 package CLI 编排 manifest、解析与锁定 | 0042 |
| 0044 | 让 LSP 发布 frontend 诊断 | 0018、0016、0003 |
| 0045 | 让 LSP 支持跳转定义 | 0044、0018、0016 |
| 0046 | 实现稳定、幂等的格式化器 | 0011、0006 |
| 0047 | 提供 TextMate grammar 与回归 fixture | 0011、0012 |
| 0048 | 提供 Tree-sitter grammar 与 corpus | 0011、0012 |
| 0049 | 提供版本化机器可读诊断协议 | 0003、0044；接受协议 ADR |
| 0050 | 构建首个支持平台的 compiler + stdlib 发行包 | 0029、0031–0040、0043；接受发布矩阵 ADR |

增量编译不预留在 Phase 0–6 主链中。它依赖稳定 module identity、package lock、SSA 和依赖
图；推荐在 SPEC-0043 完成后另建 Phase 6+ Spec，并先接受缓存键与失效策略 ADR。

## 未决决策的推荐方向

以下是起草后续 guide / ADR 时的默认推荐，不是已经接受的决策；触及对应 Spec 前仍需正式
文档批准。

| 决策 | 推荐方案 | 需要的权威文档 |
|---|---|---|
| `object` / `companion object` | 编译期限制只约束存储状态和初始化式；成员函数体可使用普通 v1 代码，但不能读取或修改运行时单例状态 | 新 guide |
| `module` / `import` | 使用显式、点分层级 module 名和绝对 import；source root 到文件的映射由 package ADR 决定，不从相对路径静默推导语义 | 新 guide + ADR |
| `Shareable` / `Transferable` | v1 与 `Copyable` 一样采用编译器已知的结构化自动推导，不开放手动实现；标准库并发类型的例外逐项写入 Spec | 新 guide |
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
