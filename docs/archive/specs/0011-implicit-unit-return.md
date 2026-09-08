# SPEC-0011: 支持具名函数隐式 `Unit` 返回

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-011` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [`agent-language-design-guide-v0.9.md`](../guides/legacy/agent-language-design-guide-v0.9.md) 第一部分第 6 节、第四部分第 7 至第 9 节 |
| 批准依据 | 用户在当前持续 Goal 中授予的后续 Spec 站立授权；用户已明确启用包含本语义变更的 v0.9 |
| 前置 Spec | SPEC-0009 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../../adr/accepted/0003-diagnostic-architecture.md)、[ADR-0004](../../adr/accepted/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；实施现行 v0.9 已授权的隐式 `Unit` 语义；该语义相对历史 v0.8 有变化 |

## 1. Goal

完成后，`lang-frontend` 能把无体或 block body 的具名 `fun` 所省略的返回标注明确保存为
`ImplicitUnit`，继续要求表达式体显式声明返回类型，并以稳定的 `L0021`、精确 `Span` 和
显式 error node 恢复非法表达式体；整个过程不进行函数级返回类型推导，也不伪造源码中
不存在的 `Unit` TypeRef 或 token。

## 2. 背景

SPEC-0008 和 SPEC-0009 按其实施时适用的 v0.7 / v0.8，要求具名函数无体、表达式体和 block
body 三种形态都显式写出 `: type_ref`。当前 Parser 因此把返回类型保存为必填的 colon
`Span` 与 `TypeRefId`，并对任何省略返回标注的函数产生 `L0021`。这些已完成 Spec 是历史
验收记录，不因新语义而回写。

现行 v0.9 将省略返回标注收敛为一个固定含义：无体或 block body 的具名函数固定返回
`Unit`；表达式体仍必须显式声明返回类型。该规则减少 `Unit` 样板但不查看函数体推导类型，
也不改变 block 本身没有尾表达式值的既有语义。本 Spec 只负责这一项 Parser / AST 增量。

## 3. 范围与需求

### 3.1 语法与固定语义

- 本 Spec 只接入 v0.9 第 8 节当前独立入口的具名 `fun` 声明。未来 class / interface Parser
  在适用 guide 未改变时必须复用本节固定的返回形态；无体函数在具体容器内是否合法，仍由
  后续容器检查决定。
- 无体或 block body 的具名函数可以省略完整的 `: type_ref`。省略时签名的返回类型固定为
  内建 `Unit`，不检查 body 内容，也不从任一路径、尾表达式或 expected type 推导。
- 表达式体 `= expression` 必须位于显式 `: type_ref` 之后。`fun f() = 1` 不是隐式 `Unit`
  函数，而是缺少显式返回类型的语法错误；Parser 必须恢复并保留表达式体。
- 显式 `: Unit` 继续合法；无体或 block body 也可以显式声明任意可解析的 `type_ref`。返回
  类型与 block 路径是否一致属于 Phase 2，不由本 Spec 提前拒绝。
- 一旦消费真实 `:`，Parser 就提交显式返回标注分支；其后缺失或非法的 TypeRef 必须形成
  `TypeRef::Error`，不能回退为 `ImplicitUnit`。
- 本次增量可等价写为以下互斥 suffix；`implicit_unit_suffix` 故意不含表达式体：

```ebnf
function_declaration   = "fun", [ type_parameter_list ], Identifier,
                         "(", [ value_parameter,
                                 { ",", value_parameter } ], ")",
                         function_suffix ;

function_suffix        = explicit_return_suffix
                       | implicit_unit_suffix ;
explicit_return_suffix = ":", type_ref, function_body ;
implicit_unit_suffix   = /* empty */
                       | block ;
function_body          = /* empty */
                       | "=", expression
                       | block ;
```

### 3.2 AST 不变量

- `Item::Function` 必须用一个合并的封闭枚举同时表达返回标注来源与 body，不得把
  `return_type` 和 `body` 暴露为可独立构造的字段，也不得使用多个可独立变化的 `Option`
  制造非法组合。最小 payload 至少等价于：

```rust
pub enum FunctionForm {
    ImplicitUnitAbsent,
    ImplicitUnitBlock(StatementId),
    Explicit {
        colon_span: Span,
        type_ref: TypeRefId,
        body: FunctionBody,
    },
}

pub enum Item {
    // 其他 variants 保持不变。
    Function {
        // 既有名称、泛型参数和值参数字段保持不变。
        form: FunctionForm,
    },
}
```

- 两个 implicit variant 都不携带返回标注 `Span`、`TypeRefId`、合成名称或虚构 token。
  Phase 2 直接把它们映射为内建 `Unit`；不得为了复用现有查询而向 TypeRef table 插入源码中
  不存在的 `Unit`。
- `FunctionForm::Explicit` 表示 Parser 已进入需要显式返回类型的分支。真实 `:` 精确保存在
  `colon_span`；缺冒号恢复时保存当前插入位置的空 `Span`。`type_ref` 引用真实 TypeRef 或
  本次恢复建立的 `TypeRef::Error`，两者都遵守现有 typed table 和 SourceMap identity。
- `FunctionBody::Absent | Expression { equals_span, expression } | Block(StatementId)` 只嵌在
  `Explicit` 分支。隐式分支只存在 Absent 和 Block 两个 variant，因此 `ImplicitUnit +
  Expression` 在类型上不可表示，无须依赖运行时构造器校验。
- 公共语法查询和结构测试必须能区分源码 `fun f(): Unit {}` 与 `fun f() {}`，并忠实返回
  `FunctionForm` 来源状态；把两者映射为同一个内建 `Unit` 类型对象属于 Phase 2，不能在
  本 Spec 中通过伪造 TypeRef 提前实现。语法来源信息不得在 AST 构造时丢失。

### 3.3 `L0021`、局部恢复与级联抑制

本 Spec 不新增错误码。既有 `L0021` 的稳定含义和固定主消息
`expected explicit return type` 保持不变，但只在当前语法确实要求或明显尝试显式返回类型时
产生：

| 参数列表后的输入 | 结果与恢复 |
|---|---|
| 调用方声明 stop 或 EOF | 构造 `FunctionForm::ImplicitUnitAbsent`，不产生 `L0021`，不消费 stop |
| `{` | 构造 `FunctionForm::ImplicitUnitBlock`，由 block owner 消费真实 `{ ... }`，不产生 `L0021` |
| `:` | 消费真实 colon 并构造 `FunctionForm::Explicit`；TypeRef 缺失或非法时复用 `L0014`，不得回退为 implicit variant，也不追加 `L0021` |
| `=` | 在 `=` 起点的空 `Span` 产生一条 `L0021`；不消费 `=`，构造空 colon `Span` 与同位置 `TypeRef::Error` 后继续解析唯一表达式体，不追加同根因 `L0014` |
| 可开始 TypeRef 的 token | 沿用缺 `:` 恢复：以该 token 为 `L0021` 主 `Span`，不消费并按插入空 colon 继续解析真实 TypeRef；随后仍可解析无体、表达式体或 block body |
| Lexer invalid / reserved token，或有 L0004–L0006 根因的 segmented string / interpolation poison | 不把 poison 猜成显式返回标注；先构造 `FunctionForm::ImplicitUnitAbsent`，再由独立声明 / 未来容器的 trailing owner 消费实际 poison 区域，只保留 Lexer 根因，不追加同 `Span` 的 `L0021` / `L0013` |
| terminal Lexer 根因后的 EOF | 构造 `FunctionForm::ImplicitUnitAbsent`；不追加 Parser 诊断 |
| 其他普通 token | 以 `FunctionForm::ImplicitUnitAbsent` 结束函数；由独立声明或未来容器的 trailing / boundary 恢复拥有该 token，不把无关尾随输入误报成缺返回类型 |

- `fun f(): = expr`、`fun f(): {}` 或 `fun f():` 已经消费真实 `:`，因此只走现有 expected
  TypeRef 恢复；不得因新规则吞掉 colon、降为 implicit Unit 或重复产生 `L0021`。
- `fun f() = expr` 的 `L0021` 必须保留 `=` 及 expression 给 expression-body owner；表达式
  自身非法时可以产生自己的根因诊断，但不得为同一个缺返回标注根因再产生 `L0014` 或
  trailing-token 诊断。
- 缺 colon 后真实 TypeRef 的恢复继续使用 SPEC-0008 的 owner-aware 扫描与
  `LexicalRecoveryIndex`。string / interpolation、nested delimiter 与调用方 hard closer 的
  所有权规则不变；上表 poison / terminal 分支固定了可选返回标注新增后的交接边界。
- 每个分支要么消费输入、要么明确停在 body opener、调用方 stop 或 EOF；不得以“也许是
  implicit Unit”为由原地重试或二次扫描。总体时间和递归预算保持既有 Parser 基线。

### 3.4 `Span` 与父子范围

- implicit variant 本身没有返回标注源码范围。隐式无体函数 Item 从真实 `fun` 起，到值
  参数表的真实 `)` 终；不得把 EOF、调用方 stop 或空插入位置纳入父范围。
- 参数表缺 `)` 或缺整个 `(` 时，函数 Item 只到参数恢复最后实际消费的 token；空 closer /
  error `Span` 不扩大父范围。参数列表 parser 可以返回真实或缺失 closer 元数据，但不得伪造
  `)` 位置。
- 隐式或显式 block-body 函数 Item 均从 `fun` 起，到 block 的匹配 `}` 终；缺 closer 时止于
  block owner 最后实际消费位置。表达式体函数到 body expression 终。
- 显式无体函数 Item 到返回 TypeRef 终；恢复 TypeRef 为空时，父范围只到最后实际消费的
  token，不能借空 error range 扩大范围。
- `L0021`、空 colon 和 Error TypeRef 的插入位置使用同一 `SourceId` 的 UTF-8 字节空范围；
  真实 colon / TypeRef / block / expression 范围继续使用半开区间。跨 SourceMap child 必须
  返回具体内部错误，不能用合并范围掩盖。

### 3.5 Fixture 与回归接入

- 在现有 Cargo fixture target 中新增 `phase1/parser-implicit-unit-pass/` 与
  `phase1/parser-implicit-unit-fail/`；各至少枚举一个真实小写 `.ko`，零 suite 必须失败。
- pass fixture 使用生产 Lexer 与 `parse_declaration`，覆盖隐式无体、空 / 非空 block、显式
  `: Unit` 及显式其他类型；runner 必须验证零诊断、完整 EOF 消费和对应 AST marker。
- fail fixture 至少覆盖缺显式返回类型的表达式体和 colon 后缺 TypeRef，以 `.diag` 精确断言
  Lexer / Parser 合并后的错误码、主 `Span` 与稳定顺序；固定消息由 Rust diagnostic catalog
  测试锁定，现有 sidecar 协议不为本 Spec 扩张。
- 人工审阅并只定向迁移 SPEC-0008 / SPEC-0009 中“无体或 block body 缺返回标注”的历史
  fail 用例；表达式体缺返回标注必须继续作为 `L0021` 负例。不得批量接受其他 golden 变化。
- 既有 source、Lexer、expression / TypeRef、declaration、block 和全部 fixture suite 继续
  执行。测试要证明显式返回类型及函数类型 parsing 无回归，且非法用户输入不 `panic!`。

## 4. 非目标

- 不实现函数级返回类型推导。尤其不从表达式体、block 中的 expression statement、显式
  `return`、所有路径结果或 expected type 推导具名函数返回类型。
- 不改变 lambda 的 header、尾表达式或返回类型规则，不改变 `(...) -> T` /
  `move (...) -> T` 函数类型；二者仍必须写出箭头后的返回 TypeRef。
- 不定义或修改构造器语法。构造器没有本 Spec 所讨论的具名函数返回标注，不能借
  `ImplicitUnit` 为其合成函数返回类型。
- 不实现 `return` / `break` / `continue`、控制流、完整文件、class-family、module / import、
  名称解析、重载 / override 检查或无体函数容器合法性。
- 不检查显式或隐式返回类型与函数 body 是否一致，不实现 `Nothing`、类型推导或其他
  Phase 2 规则，也不实现所有权、借用、IR、codegen 或标准库变化。
- 不修改已完成的 SPEC-0008 / SPEC-0009 历史验收，不新增 ADR、crate、第三方依赖、公共
 机器诊断协议、CST / green tree、增量 Parser 或通用 AST visitor。

## 5. 验收标准

- [x] compile-pass 覆盖 `fun f()`、`fun f() {}`、非空 block body、显式
      `fun f(): Unit` / `fun f(): Unit {}` 以及显式非 `Unit` 的无体 / block body；trivia 变体
      不改变返回 marker 或 body 归属。
- [x] AST 测试证明省略标注只产生 `FunctionForm::ImplicitUnitAbsent` 或
      `FunctionForm::ImplicitUnitBlock`，显式或恢复分支只产生 `FunctionForm::Explicit`；
      `ImplicitUnit + Expression` 在类型上不可表示，也不存在孤儿 TypeRef、伪造
      `Unit` TypeRef、伪造 colon token 或可独立变化的双 `Option` 状态。
- [x] `fun f() = 1` compile-fail 恰好产生一条 `L0021`，主 `Span` 为空且位于真实 `=` 起点；
      AST 保留空 colon、同位置 Error TypeRef、真实 `equals_span` 与 body expression，不追加
      `L0014` 或 trailing 级联。
- [x] `fun f(): = 1`、`fun f(): {}` 和 colon 后 EOF 只复用 expected TypeRef 诊断并保持
      `Explicit`，不产生 `L0021`、不回退 implicit Unit；后续 body opener 的所有权不丢失。
- [x] 缺 colon 后紧跟普通 / 泛型 / nullable / 函数 TypeRef 的恢复继续产生 `L0021`，保留真实
      TypeRef 和随后的三种 body；其他尾随 token 归调用方 trailing 恢复而不是误报 `L0021`。
- [x] Item、return marker、colon / TypeRef、body 与 error node 的 UTF-8 半开 `Span` 符合本
      Spec；隐式无体 Item 精确止于 `)`，隐式 block Item 止于 block，multi-SourceMap 混用受控
      返回具体内部错误。
- [x] owner recovery 覆盖缺返回标注的表达式体嵌套 group、call、index 与 string
      interpolation，以及直接函数 block body；独立声明入口的 EOF owner 与 block closer
      保留，Lexer poison / `L0004`–`L0006` 不产生同 Span 或同 owner 的 Parser 级联。
- [x] source、Lexer、SPEC-0007 expression / TypeRef、SPEC-0008 declaration 与 SPEC-0009
      block / function-body 窄测试通过；只定向迁移本版取代的历史负例。
- [x] parser-implicit-unit pass / fail suite 各真实执行至少一个 `.ko`；零 suite、非法 sidecar、
      orphan pair 与空范围 policy 自检继续失败，诊断全序确定。
- [x] 深泛型、函数 TypeRef、block 和表达式体继续共享固定 32 MiB worker 与 1024 递归预算；
      超预算受控返回具体内部错误，长正确 / 错误输入保持单调 `O(n)`，不 panic。
- [x] `cargo tree -p lang-frontend --edges all --locked --offline` 与 manifest / lock diff 证明未新增
      normal、dev 或 build 依赖。
- [x] frontend 窄测试及 workspace fmt、check、Clippy、test、CLI build 基线通过；实际记录
      passed / failed / ignored / filtered 数量，不把未执行检查写成通过。
- [x] Architecture 更新为实现后的 FunctionForm AST、Parser dispatch、`L0021` 新适用边界和
      fixture 事实；计划不得写成当前事实。

## 6. 技术方案与边界

保持现有 declaration Parser、typed AST、诊断目录、owner-aware scanner、固定 worker 与递归
预算。值参数表解析完成后按当前 token 做一次互斥 dispatch：

1. `:` 提交 `FunctionForm::Explicit` 并解析 TypeRef，再选择无体、表达式体或 block body；
2. `=` 产生 `L0021` 及显式错误返回标注，再按既有 expression-body 路径解析；
3. `{` 直接构造 `FunctionForm::ImplicitUnitBlock` 并交给 block owner；
4. EOF 或调用方 stop 构造 `FunctionForm::ImplicitUnitAbsent`；
5. 可开始 TypeRef 的 token 按缺 colon 的 `L0021` 路径恢复为 `FunctionForm::Explicit`；
6. 其他 token 先结束隐式无体函数，由调用方恢复尾随输入。

`FunctionForm` 是语法来源与合法 body 组合的封闭表示；后续类型查询可以在单一 helper 中把
两个 implicit variant 解析为内建 `Unit`，但本 Spec 不提前建立 Phase 2 类型对象。现有 `L0021` 编号、
消息和排序注册保持不变，只收窄其适用语法位置；不分配替代错误码。

本 Spec 沿用 ADR-0003 / ADR-0004，不需要新 ADR，也不引入依赖或改变 workspace / crate
边界。准确 Rust 类型名可在不改变封闭状态、typed ID、`Span` 和可观察诊断时微调。

## 7. 实施计划

1. [x] 引入封闭 `FunctionForm` 并迁移函数 Item、getter 与现有结构测试
   → 验证：显式分支行为不变，implicit marker 无 TypeRef / Span，非法状态不可构造
2. [x] 重排函数 suffix dispatch，接入 implicit Unit 与 `L0021` 表达式体恢复
   → 验证：无体 / block pass 矩阵、expression-body fail、colon / TypeRef 恢复和精确范围
3. [x] 补齐 owner、Lexer terminal、SourceMap、递归预算与复杂度回归
   → 验证：调用方 closer 保留、无级联、无 panic、N→2N 同族输入保持线性
4. [x] 接入 parser-implicit-unit pass / fail fixture并定向迁移历史负例
   → 验证：真实 `.ko` / `.diag`、零用例保护、诊断全序和既有 suite 回归
5. [x] 同步 Spec 验收记录与 Architecture
   → 验证：frontend 窄测试、workspace 全基线、依赖树、staged diff 与文档事实一致

v0.9 已由用户明确启用；本 Spec 已在 SPEC-0010 Goal 完成后，依据站立授权按
`draft → approved → in-progress` 的逻辑顺序推进，无需单独的批准状态提交。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | FunctionForm AST、函数 suffix dispatch、`L0021` 恢复、测试 / fixture、Architecture 与完成记录 | `feat(frontend): support implicit Unit returns (SPEC-0011)` |

实现提交只属于 SPEC-0011，不混入 lambda、typed call argument、解构、完整文件、新 guide
激活或其他语法。v0.9 启用必须在实施前形成独立文档边界；实现、测试、Architecture、验收与
`done` 状态全部满足后才能创建本表提交。

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| v0.9 版本级启用确认 | 满足 | 用户已明确指定 v0.9 取代 v0.8；本 Spec 依站立授权完成 `draft → approved → in-progress → done` 流转 |
| Markdown 相对链接与路线图一致性检查 | 通过 | v0.9、ADR-0003、ADR-0004 链接存在；README 的 0011 状态、前置与站立授权一致 |
| 独立红队与终审 | 通过 | 最终 P0 = 0、P1 = 0；封闭 AST、suffix dispatch、`L0021` / `L0014`、nested lexical owner 与 trailing 归属均闭环 |
| `cargo test -p lang-frontend --test parser_implicit_unit --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered；十三套 suite 均有真实用例 |
| `cargo test -p lang-frontend --test source_span --locked --offline` | 通过 | 9 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test lexer --locked --offline` | 通过 | 18 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 通过 | 52 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_declaration --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_block --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_lambda --locked --offline` | 通过 | 16 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 19 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --doc --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo fmt --all -- --check` | 通过 | rustfmt 无差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target 编译成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 零 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 208 passed；0 failed / ignored / measured / filtered；CLI 6、frontend 201、std 1，`lang-codegen` / `lang-lsp` 当前测试数为 0 |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI debug target 构建成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend`；manifest / lock 无差异，未新增 normal / dev / build 依赖 |
| `git diff --check` | 通过 | 无空白错误 |
| Architecture 同步 | 完成 | 已记录 `FunctionForm`、互斥 suffix dispatch、`L0021` / `L0014` 与 Lexer poison 归属，以及十三套 fixture |
