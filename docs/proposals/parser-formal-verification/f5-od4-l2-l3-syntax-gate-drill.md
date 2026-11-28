# F5 增量门禁演练：OD4（L2/L3 候选语法前三项）

> **性质**：非规范 F5 增量门禁演练账本 · **状态**：候选演练（未启用；不修改 Guide v0.47，不批准 Spec/ADR，不授权修改生产 Parser 或 CI） · **读取时机**：评审 OD4 约定的三项 L2/L3 候选语法拼写（`flows … into`、`view T`、`p.origins`）的纯 CFG 冲突、软关键字共存与恢复边界时 · **唯一真源**：本页维护 OD4 三项候选语法的 F5 六项门禁字段、GNU Bison 2.3 冲突实测与正反例见证矩阵；门禁模板见 [F5 增量门禁清单](syntax-gate-checklist.md)，OD4 讨论收口条件见 [裁决纪要 §5](../kernel-open-decisions-2026-11.md#5-2026-11-27-koven-design-三轮讨论收口纪要od3od6)

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权修改生产 Parser、Formatter、Tree-sitter 或 CI。

## 0. 演练范围与 OD4 排除项

按 2026-11-27 `#koven-design` OD4 三轮讨论收口条件：

- **本轮纳入演练（仅前三项）**：
  1. **`flows … into`**：函数/方法签名的参数到目标来源流向子句（并与候选拼写 `from s into v` 做一词两义冲突对照）。
  2. **`view T`**：泛型类型参数列表中的修饰前缀，语义为**解除默认可逃逸上界**（而非要求 `T` 必须不可逃逸），并与 `view class` 及普通标识符 `view` 共存检验。
  3. **`p.origins`**：仅用于合同来源位置（`from` / `flows` 左侧）的选择器后缀，**不得作为 `into` 写入目标**，并检验与真实同名字段 `origins` 的冲突边界。
- **本轮明确排除**：
  - **`borrow var` 不进本轮 F5、Guide v0.47 不变**；未来若就 R2.6 另案重启，须分列两项独立验收——① 换绑后旧别名不可再用，② 绑定槽不可取址或外露。
  - **`any I` 同名成员消歧**与“精确选中真实字段 `origins` 的逃生拼写”仍保持开放，不在本轮演练内闭合。

## 1. 受影响入口与规则映射

| 候选语法 | 受影响入口 | 对应 F3e 规则 ID | 目标 Guide 章节（唯一回链） |
|---|---|---|---|
| `flows … into` | `parse_declaration`、`parse_file`（顶层/成员/接口函数头） | `D-FUN`、`D-MEMBER`（[F3e 声明切片](declaration-file-grammar-slice.md#1-guide-02--05--08-规则-id-映射纯结构-cfg)） | [Guide 05 声明语法](../../guide/05-declarations-callables.md#声明语法)（函数签名的可选来源子句） |
| `view T` | `parse_declaration`、`parse_file`（`fun` 与 `class`/`view class` 泛型参数表） | `D-FUN`、`D-CLASS`（`type_parameter_list`） | [Guide 05 声明语法](../../guide/05-declarations-callables.md#声明语法)（`type_parameter`）与 [Guide 08 类族与成员](../../guide/08-class-family-members.md#声明头构造器字段与修饰符)（`view class` 声明头） |
| `p.origins` | `parse_declaration`、`parse_file`（仅 `from` 返回子句与 `flows` 来源列表） | `D-FUN`（`borrow_return`、`carrier_return`、`flows_clause`） | [Guide 05 声明语法](../../guide/05-declarations-callables.md#声明语法)（`origin_source` 与 `origin_target` 产生式） |

各规则均只回链唯一 Guide 章节，不在 Guide 03 / 05 / 08 多处复制互相矛盾的允许条件。

## 2. 纯 CFG 增量与 GNU Bison 2.3 冲突实测

### 2.1 候选核心增量产生式（不含优先级指令）

```ebnf
ident             = Identifier | "view" | "flows" | "into" | "origins" ;
non_view_ident    = Identifier | "flows" | "into" | "origins" ;
non_origins_ident = Identifier | "view" | "flows" | "into" ;

(* 1. view T（左因子化，允许 <view> / <view : Bound> / <view T> / <view view : Bound>） *)
type_parameter    = non_view_ident, [ ":", type_ref ]
                  | "view", [ ident ], [ ":", type_ref ] ;

(* 2. flows ... into（位于可选返回子句之后、函数体或 '=' 之前，同行附着） *)
flows_clause      = "flows", origin_source_list, "into", origin_target ;
origin_source_list = origin_source, { ",", origin_source } ;

(* 3. p.origins（仅允许作为来源后缀，文法层直接禁止进入 origin_target） *)
origin_root       = ident | "this" ;
origin_path       = origin_root, ".", non_origins_ident, { ".", non_origins_ident } ;
origin_source     = origin_root
                  | origin_path
                  | ( origin_root | origin_path ), ".", "origins" ;
origin_target     = origin_root
                  | origin_path ;
```

### 2.2 离线实测冲突报告（macOS AArch64，GNU Bison 2.3，无 `%left`/`%right`）

在仓库外临时目录构建与 [F3e](declaration-file-grammar-slice.md) 同源的最小声明切片文法，逐项施加增量与反面对照（临时目录已在实测后清理）：

| 实验编号 | 测试文法 | Bison 2.3 冲突报告 | 归因与裁决含义 |
|---|---|---|---|
| `0_baseline` | Guide v0.47 基准（`D-FUN` + `D-CLASS` + `type_parameter_list` + `from`） | **0 shift/reduce、0 reduce/reduce** | 确认增量前基线干净 |
| `1a_flows_comma` | `flows a, b into x, flows c into y`（`flows`/`into` 为软词，同行） | **0 shift/reduce、0 reduce/reduce** | 逗号分隔多条 `flows` 子句在同行位置无冲突 |
| `1a_flows_repeat` | `flows a, b into x flows c into y`（连续 `flows` 子句，同行） | **0 shift/reduce、0 reduce/reduce** | 连续 `flows` 同样零冲突且避免逗号在来源列表与子句间复用 |
| `1b_from_into_contrast` | **反面对照**：复用 `from s into v` 与 `: View from a` 并存 | **`1 shift/reduce` 冲突** | `: TypeRef` 后前瞻 `from` 无法在 LALR(1) 内区分「返回借用来源 `from a`」还是「跳过返回来源直接进入 `from s into v`」；即使多 token 前瞻也须扫完整个来源列表直至看到 `into` 或 `{`，错误恢复时极易混淆两类子句。实测直接支持 OD4 舍弃 `from s into v`、采用 `flows … into` |
| `A_flows_nl` | **换行边界对照**：在允许无体接口方法的文法里放开 `flows` 前可选 `NL` | **`1 reduce/reduce` 冲突**（State 42） | 无体方法后的 `NL` 既可作为成员分隔符 `sep`，又可引导跨行 `flows`。表明若未来允许跨行 `flows`，绝不能直接把 `opt_nl` 放进纯 CFG，必须像 D1 一样在条文中锁定显式外部谓词（仅当跳过 `NL` 后紧跟 `flows` 才附着，且不得越过 `;` 或 caller hard stop）；本轮默认采用**同行附着** |
| `2a_view_naive` / `2b_view_factored` | `view T` + `view class` + `view` 作普通标识符与类型参数名 | **0 shift/reduce、0 reduce/reduce** | `view` 作上下文软词不破坏 `<view>`、`<view : Any>`、`<view T>`、`<view view : Any>` 或 `class view` / `val view = ...` |
| `B_origins_collision` | **撞名对照**：允许 `origins` 同时作为 `origin_path` 普通字段段与 `.origins` 选择器 | **`2 reduce/reduce` 冲突**（State 12、14），且 Bison 警告两条 `.origins` 规则因冲突**永不归约** | 实测证明纯 CFG 无法区分“同名字段 `p.origins`”与“合同选择器 `p.origins`”。因此文法层必须把合同位置上的 `.origins` 唯一解析为选择器（且从 `origin_target` 排除），再由 Phase 2 签名解析对真实同名 `origins` 字段直接报 `SIG` 拒绝（对照[借用内核演算 §82](../loan-kernel-calculus.md)） |
| `3_combined_od4` | 三项合并（`flows … into` + `view T` + `p.origins`，`%expect 0`） | **0 shift/reduce、0 reduce/reduce** | 三项候选组合在纯 CFG 层零冲突，且已编译为 C 解析器通过 20 组正反例实跑 |

## 3. 软关键字、词法与换行影响

1. **零新增全局保留字**：`flows`、`into`、`view`、`origins` 全部保持上下文软关键字，不改动 [Guide 01](../../guide/01-lexical.md) 的保留字表与词法错误交接（R4）。
   - 普通表达式中的 `val view = origins`、`fun flows(borrow into: view)`、`obj.origins` 均继续作为普通 `Identifier` 合法解析。
2. **换行与分号边界**：
   - **同行默认**：`flows` 子句紧随 `)` 或返回子句（`: TypeRef` / `: borrow T from ...`）同行出现；`;` 立即终止函数头，绝不跨 `;` 附着 `flows`。
   - **跨行限制**：如上表 `A_flows_nl` 实测，无条件允许 `NL` 引导 `flows` 会与无体接口方法后的声明分隔 `NL` 产生 `1 reduce/reduce` 冲突；若正式提案要支持长签名换行写 `flows`，须在 Guide 05 写明无副作用前瞻谓词（仅在未遇到 `{` / `=` / `;` 且跳过 `NL` / 含换行块注释后下一非 trivia token 为 `flows` 时继续解析，CRLF 与 LF 一致）。
   - **尖括号与 `view T`**：`view` 在 `< ... >` 内仅当后接另一个标识符时作修饰前缀；若后接 `:`、`,` 或 `>`，则该 `view` 自身就是类型参数名。

## 4. 恢复边界与资源合同影响（R1–R5）

| 合同项 | 影响评估与约束 |
|---|---|
| **R1（调用方边界与 hard stop）** | `type_parameter_list` 内出现非法 `view`（如重复修饰 `fun <view view T> bad()`、末尾逗号 `fun <view, > bad()` 或上界缺类型 `fun <view T :> bad()`；注意带空格的 `<view >` 为合法的名为 `view` 的类型参数，不得误报缺名）继续沿用 Guide 05 统一 owner-aware 扫描，保留 `>`、`)`、`;`、`{` 与 EOF 作为 hard stop，不越过 `>` 消费函数名或参数表；`flows` 子句遇到缺来源或缺 `into` 目标时，在 `{`、`=`、`;`、`NL`、外层 `}` 或 EOF 前停止，不吞掉函数体或下一成员 |
| **R2（软声明边界）** | `flows` / `into` / `origins` / `view` 均不加入顶层或类体软声明同步起始词集合，避免合法标识符误触发跨声明同步 |
| **R3（owner stack）** | 三项增量均不引入新的词法分隔符或字符串插值模式切换，不改变 owner stack 深度 |
| **R4（词法错误交接）** | 遇到 Lexer poison / invalid token 时保持既有 `ImplicitUnitAbsent` 与原根因优先策略，不在 `flows` 位置重复追加伪造诊断 |
| **R5（线性预算）** | `view T` 与 `p.origins` 均为固定 1-token 前瞻（O(1)）；`flows` 同行形态为 O(1) 前瞻，若后续开放跨行前瞻也仅跳过连续 trivia 至首个非 trivia token，保持线性时间、零递归放大 |

## 5. 独立正反例与边界见证矩阵

下列 20 组句法见证由 `3_combined_od4` 经 `bison` + `cc -Wall -Wextra -Werror` 编译后实跑验证（**10/10 正例通过，10/10 反例拒绝**）；另附 1 组 Phase 2 语义撞名负例供签名检查衔接：

### 5.1 正例见证（10/10 通过）

| # | 见证输入 | 检验目标 |
|---|---|---|
| P1 | `fun push(inout b: Bag, borrow x: Item) flows x into b {}` | 省略返回类型的 block 函数 + 单源 `flows` |
| P2 | `fun merge(inout dst: Slot, borrow a: Item, borrow p: View) flows a, p.origins into dst.target {}` | 多来源逗号列表 + `p.origins` + 目标字段路径 |
| P3 | `fun attach(inout a: Slot, inout b: Slot, borrow x: Item, borrow y: Item): borrow Item from x flows x into a flows y into b {}` | 返回 `from` 与多条连续 `flows` 共存 |
| P4 | `view class Bag<view T: Display> {}` + `fun <view T> hold(borrow x: T): T from x {}` | `view class` 与 `view T`（带/不带上界） |
| P5 | `val view = origins` + `class view<view: Any> {}` + `fun <view > id() {}` + `fun <view view: Any> flows(borrow into: view) {}` | 四个软词全部作为普通标识符、类型名、带空格的合法 `<view >` 类型参数名与参数名共存 |
| P6 | `fun <view T> forward(borrow p: T): borrow Item from p.origins {}` | `p.origins` 用于返回 `from` 合同 |
| P7 | `fun extract(borrow p: Holder): borrow Item from p.inner.origins {}` | 嵌套字段路径后接 `.origins` 选择器 |
| P8 | `interface Store { fun push(inout b: Bag, borrow x: Item) flows x into b\n fun size(): Int }` | 接口体内的无体方法携带 `flows` 且正确与下一成员分隔 |
| P9 | `fun id(inout dst: Slot, borrow x: Item): Int flows x into dst = x` | 表达式体函数 `=` 之前携带 `flows` |
| P10 | `fun stash(inout dst: Slot) flows this.origins into dst.slot {}` | `this.origins` 作为来源流入参数字段 |

### 5.2 反例与撞名见证（10/10 文法拒绝 + 1 项 Phase 2 语义拒绝）

| # | 见证输入 | 预期结果与依据 |
|---|---|---|
| N1 | `fun bad(inout dst: Slot, borrow x: Item) flows x into dst.origins {}` | **文法拒绝**：`p.origins` 是只读合同选择器，禁止作为 `into` 写入目标 |
| N2 | `fun bad(inout dst: Holder, borrow x: Item) flows x into dst.inner.origins {}` | **文法拒绝**：嵌套字段路径末端也不得对 `into` 目标使用 `.origins` |
| N3 | `fun bad(borrow p: View): borrow Item from p.origins.origins {}` | **文法拒绝**：禁止重复链式 `.origins.origins` |
| N4 | `fun bad(inout dst: Slot, borrow x: Item) flows x into {}` | **文法拒绝**：`into` 后缺失写入目标 |
| N5 | `fun bad(inout dst: Slot) flows into dst {}` | **文法拒绝**：`flows` 后缺失来源列表 |
| N6 | `fun bad(inout dst: Slot, borrow x: Item): View from x into dst {}` | **文法拒绝**：拒绝 `from s into v` 一词两义写法 |
| N7 | `fun bad(inout dst: Slot, borrow x: Item) flows x, into dst {}` | **文法拒绝**：来源列表不允许末尾逗号 |
| N8 | `fun bad(inout dst: Slot, borrow x: Item); flows x into dst {}` | **文法拒绝**：分号阻断函数头，`flows` 不得跨 `;` 附着 |
| N9 | `fun bad(inout dst: Slot, borrow x: Item)\nflows x into dst {}` | **文法拒绝**（同行基准）：无显式跨行谓词时，`flows` 不跨 `NL` 附着 |
| N10 | `fun <view view T> bad() {}` | **文法拒绝**：禁止重复 `view` 修饰前缀 |
| S1（Phase 2） | `class Bad(val origins: Int)` 上写 `fun f(borrow p: Bad): borrow Int from p.origins` | **Phase 2 拒绝（`SIG`）**：纯 CFG 无法区分同名字段与选择器（见 `B_origins_collision`），由签名解析在目标类型存在真实 `origins` 字段时直接报错拒绝（与 `scripts/loan_kernel/cases/Z44.lk` 对齐） |

## 6. 下游同步范围与放行清单核对

- **下游同步范围（本轮均为范围外，进入正式 Guide 前须逐项补齐）**：
  - **F4c Formatter 往返投影**：新增 `flows … into`、`view T`、`p.origins` 的规范打印与往返测试。
  - **F4a 独立参考差分语料**：将上述 21 组正反例并入独立参考声明切片语料，并与生产 parser 做差分。
  - **F4b Tree-sitter 漂移账本**：在 `tree-sitter` 文法同步前，将三项候选登记为已知待同步漂移项。
- **F5 评审放行清单自检（对照 [syntax-gate-checklist.md §2](syntax-gate-checklist.md#评审放行清单进入-guide-前)）**：
  - [x] 规则增量回链唯一 Guide 章节，未在多章复制矛盾条件。
  - [x] 冲突变化已由仓库内可复跑套件 `scripts/loan_kernel/f5_od4_grammar_probe.py`（单测 `scripts/tests/test_f5_od4_grammar_probe.py`）固化全部 9 组基准/增量/对照 Bison 文法并实测通过（含 `from s into v` 1 S/R、跨行 `flows` 1 R/R、`origins` 撞名 2 R/R，以及 `3_combined_od4` 11 正例 + 10 反例编译实跑）。
  - [x] 软词（`flows`/`into`/`view`/`origins`）未破坏普通 `Identifier` 或词法错误交接（R4）。
  - [ ] 正反例已覆盖抽象 `NL` / 分号阻断、重复修饰、带空格 `<view >`、误作 `into` 目标与撞名边界，但**尚未在真实 Lexer/Parser 上验证 CRLF、含换行块注释、未闭合回退及恢复期调用方边界保留**；进入正式 Guide 提案前须补齐对应见证。
  - [x] 未将 Bison 或 Tree-sitter 提升为第二规范，未夸称有限用例为全语言证明。

## 7. 验证与未运行项

- **已运行**：
  - macOS AArch64 下 `python3 -m unittest scripts.tests.test_f5_od4_grammar_probe`（Python 3.12+ 与 Python 3.9 双版本均通过）：通过 GNU Bison 2.3（`%expect 0`，无 `%left`/`%right`）复现全部 9 组文法冲突计数，并用 `cc -Wall -Wextra -Werror` 编译 `3_combined_od4` 跑通 21 组正反例见证（`POS 11/11, NEG 10/10`）。
  - `/opt/homebrew/bin/python3 scripts/check_docs.py && /opt/homebrew/bin/python3 -m unittest scripts.tests.test_check_docs`。
- **未运行**：未修改 `docs/guide/`、`crates/`、Formatter、Tree-sitter 或 CI；未启用 OD4 任何候选语法；未在真实生产 Lexer/Parser 上运行 CRLF/含换行块注释/恢复边界测试；未运行 `cargo test`。
