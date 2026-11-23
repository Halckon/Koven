# F3e：声明与完整文件切片参考文法与漂移校验

> **性质**：非规范试验账本 · **状态**：声明与文件头纯 CFG 已实跑零冲突与漂移注入，独立参考解析差分由 [SPEC-0484](../../specs/active/0484-f3e-file-difftest.md) 承接 · **读取时机**：扩展完整文件参考文法或核对类族/修饰符/分隔约束时 · **唯一真源**：本页维护 Guide 02/05/08 声明与文件规则 ID 映射、分隔符因子化发现与离线校验结果

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47。
接续[18 级表达式运算符层级切片](expression-grammar-slice.md)，面向 `parse_file` 与 `parse_declaration`
的文件头、顶层声明、五类 classifier、成员与分隔规则。本切片不含 D1–D3，不修改生产 Parser、Guide、依赖或 CI；
离线试验使用 macOS 自带 GNU Bison 2.3，探针在临时目录运行后已清理，未进入仓库。

## 1. Guide 02 / 05 / 08 规则 ID 映射（纯结构 CFG）

| 规则 ID | Guide 来源 | 覆盖范围与关键句法约束 |
|---|---|---|
| `D-FILE` | [02 完整文件与声明分隔](../../guide/02-names-files-packages.md#完整文件与声明分隔) + [Package 与 Import 文件头](../../guide/02-names-files-packages.md#package-与-import-文件头) | 空文件合法；可选单一 `package` 必须在最前；零或多个 `import` 紧随其后且在声明前；精确导入可选 `as ID`，通配符 `.*` 禁止别名；构造间强制 `NL` 或 `;` 分隔，文件尾允许单个可选 `;`，拒绝前导 `;` 与连续 `;;` |
| `D-VAL` | [05 声明语法](../../guide/05-declarations-callables.md#声明语法) | `[visibility] (val \| var \| const val) ID [: type_ref] = expr` |
| `D-FUN` | [05 函数返回类型](../../guide/05-declarations-callables.md#函数返回类型) + [声明语法](../../guide/05-declarations-callables.md#声明语法) | `[visibility] fun [<type_params>] ID ( [value_params] )`；省略 `: type_ref` 仅允许无体或 `block`，`= expr` 必须显式写出 `: type_ref`；参数模式仅限 `own`/`borrow`/`inout` |
| `D-CLASS` | [08 类族与成员](../../guide/08-class-family-members.md#声明头构造器字段与修饰符) | `value class` 强制非空主构造器字段列表；`class` 主构造器可选；`interface`/`enum class`/`object` 各自匹配封闭成员集；父类型列表支持 `type_ref [by ID]` 委托 |
| `D-MEMBER` | [08 Receiver Grammar](../../guide/08-class-family-members.md#receiver-grammar) + [`deinit`](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约) | 实例方法修饰符严格固定顺序 `[visibility] [override] [own\|borrow\|inout] fun`；`deinit() block` 仅允许普通 `class` 且禁带可见性修饰符；`interface` 方法仅允许可选 `public`；`object` 仅允许方法与 `const val`；`enum` 分号后必须至少含一个成员 |

## 2. 分隔符左因子化发现与未覆盖清单（不计入 G1）

- **LALR(1) 边界发现**：若直接将 `file_sep` 与 `file_trailing_opt` 并列定义，Bison 2.3 会在 `NL` 前瞻报
  **`8 shift/reduce` 冲突**（看到 `NL` 时无法在 1 个前瞻内区分“后继仍有声明/成员”还是“尾随换行抵达 EOF/`}`”）。
  将序列改写为等价的右递归尾部（`sep_head` + `after_package`/`after_import`/`after_decl`/`after_*_member`）后，
  同一语言在 `%expect 0` 下达到 **0 shift/reduce、0 reduce/reduce**。
- **未覆盖清单（不计入 G1）**：
  1. `borrow_return` / `carrier_return`（`from` 子句）与受限标准扩展头 `borrow fun Type.name`。
  2. 完整泛型/函数类型 `TypeRef`（嵌套 `<...>`、`move (...) -> R`）与完整表达式/块体（本切片用最小 `expr`/`block`/`type_ref` 占位以隔离声明层冲突）。
  3. 软关键字在普通 `Identifier` 位置复用（需外部词法/上下文谓词，如 F3d/F4b 已登记的 `value`/`by`/`own`/`borrow`/`inout`）。
  4. 跨声明恢复、owner-aware 扫描与诊断码（属恢复轨道 R1–R5，不由合法 CFG 替代）。

## 3. 离线实跑与漂移注入结果

环境：macOS AArch64，GNU Bison 2.3（`%expect 0`，无 `%left`/`%right`），`cc -Wall -Wextra -Werror`。

| 验证项 | 结果 | 代表性见证 |
|---|---|---|
| 未因子化对照 | 退出非 0，报告 `conflicts: 8 shift/reduce` | 记录 `NL` 在分隔与尾随位置上的 LALR(1) 边界 |
| 左因子化基准文法 | 退出 0，**0 shift/reduce、0 reduce/reduce** | 完整覆盖 `D-FILE`、`D-VAL`、`D-FUN`、`D-CLASS`、`D-MEMBER` |
| 12 个手写正例 | **12/12 通过** | 空文件、仅换行、`package`+`import as`+`.*`+尾 `;`、`val`/`var`/`const val`、`fun` 三种 body、五类 classifier、`by` 委托、`deinit`、`companion`、完整 `public override inout fun` |
| 14 个手写反例 | **14/14 拒绝** | `import a.b.* as c`、声明后 `package`/`import`、同行无分隔 `val a = 1 val b = 2`、`fun f() = 1`、空 `value class Empty()`、`override public fun`、`own override fun`、`public deinit() {}`、`interface` 中 `private fun`、`object` 中 `val`、`enum class E { A; }`、单独 `;`、双 `;;` |
| 漂移注入 1（函数省略返回类型） | 放开 `fun f() = 1` → 反例被误接受 | 证明反例见证可拦截 `D-FUN` 规则退化 |
| 漂移注入 2（通配符别名） | 放开 `import a.b.* as c` → 反例被误接受 | 证明反例见证可拦截 `D-FILE` 通配符约束退化 |
| 漂移注入 3（可空分隔符歧义） | 将 `sep_head` 改为可空 → **Bison 退出非 0**，报告 `conflicts: 14 shift/reduce, 14 reduce/reduce` | 证明 `%expect 0` 可拦截分隔符引入的歧义 |

## 4. 证据边界与未运行项

- **证据等级**：仅证明上述声明与文件头子集在左因子化后的纯 CFG 于 Bison 2.3 下零冲突，且 26 个正反例与 3 项漂移注入符合预期；
  **不构成全语言无歧义证明，不覆盖真实生产 Lexer 流、恢复语义或 D1–D3**。
- **未运行项**：当前沙箱 PATH 无 `cargo`，未重跑生产 `parser_file`/`parser_declaration`/`parser_class_family` 套件
  （既有基线见 [F0a](recovery-baseline.md#当前基点实跑) 与 [F0b](observation-baseline.md#4-本轮验证账本)）；未运行远端 CI。
