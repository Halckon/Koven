# Koven 仓库 Review 与里程碑差距

> 日期：2026-10-13 · 基线：main `614a7a7f`（PR #89 合入后），Guide v0.43
> 姊妹文档：[Guide v0.43 语言设计审计](2026-10-13-guide-v0.43-design-audit.md) · [v1 范围：必做与延后](2026-10-13-v1-scope.md)
> 方法：读代码与文档，并在 Linux x86_64（LLVM 21.1.8，rustc 1.97 stable）上从源码构建 `kovenc` release，用 85 个小程序实测单文件与 project 两种模式。
> 未做：没有运行 `cargo test`、CI 或 sanitizer；macOS 未测；rustc 不是仓库固定的 1.96.0。
> **2026-10-14 更新**：已在 main `b43a475b`（PR #99）上复测，变化详见 [增量 Review](2026-10-14-incremental-review.md)。§0、§2 的 M2B 行、§3 的方法行与汇总已按新结果更新；其余结论不变。
> **2026-10-15 更新**：已在 main `3104baa4`（PR #105）上复测，见 [10-15 增量审计](2026-10-15-incremental-review.md)。§4.3 前端放行与 §4.5 文档漂移大部分已修，§4.4 标准环境未变；原生能力 project 35/71、单文件 28/71。
> **2026-10-17 更新**：已在 main `a2a7e4b6`（PR #118，Guide v0.44）上复测，见 [10-17 增量审计](2026-10-17-incremental-review.md)。内部错误清零；按 v0.44 口径 74 个合法程序 project 38、单文件 32。

## 0. 结论

1. **计划内的里程碑完成了一半左右，剩下的是更难的一半。** M0、M1A 已完成；M1B、M2B、M3A、M3B、M4、M5 都只完成首片；M6 未开始。
   主线下一站 M1B-b（文本输入）连所需的语言决策都还没有做。
2. **实际可用能力明显落后于文档给人的印象。** 按 Guide 应当合法的 71 个基础小程序里，project 模式能编译运行 35 个（49%），单文件模式 25 个（35%），两种模式都能跑的只有 23 个。（10-14 复测：单文件 26 个、两种模式 24 个，project 不变。）
   字段赋值、`inout` 标量参数、Elvis、函数引用、把 lambda 直接作实参、`when (Int)`、`when (String)`、nullable 参数、浮点、`Char`、`Box` 访问都不能原生编译。
3. **进度慢的主因是结构性的，不是工作量不够。** 三个原因叠加：单文件与 compilation-unit 两套并行管线；按"AST 节点 × 类型 × 控制流"组合逐个特判的 lowering；
   每个切片都要走完整的 Spec/账本/归档/双宿主证据流程。10 月前 13 天用掉了约 80 个 Spec 编号（0228→0310），但很多 Spec 的范围是"single Map 条件 break"这一级。
4. **前端有放行漏洞**（10-15：SPEC-0319 已修复主体，见 [10-15 增量审计](2026-10-15-incremental-review.md)）。 不存在的成员（`a.fooBar()`、`s.length`）、对 nullable 用 `?`、`for (i in 0..<5)` 都不会得到前端诊断，而是落到后端报 `BlockingDeferred`/`MissingFact`，或以 `ownership checking failed: invalid compilation-unit argument …` 形式泄漏内部错误。
   这违反 Guide 03 "deferred 不得进入所有权检查或 codegen" 和 AGENTS"非法源码必须返回结构化诊断"两条规则。

建议：先停下横向扩功能，用 2–3 个结构性工作换取后面所有切片的速度（§5）。

## 1. 仓库快照

| 维度 | 数值 |
|---|---|
| 提交 | 1,126（8 月 457、9 月 119、10 月 1–13 日 550） |
| Rust 生产代码 | 约 17.6 万行：frontend 10.7 万、codegen 6.2 万、cli 0.38 万、lsp 0.30 万 |
| Rust 测试代码 | 约 19.3 万行，`#[test]` 约 3,600 个 |
| Koven 标准库源码 | 96 行（`prelude.ko` 39 行、`algorithms/ranges.ko` 57 行） |
| 文档 | 663 个 Markdown，约 12.9 万行；Spec 归档约 295 份，active 0 份 |
| 10 月 1–13 日改动 | docs +16.8 万/−11.6 万行；crates +17.8 万/−6.1 万行 |
| 构建 | release 构建约 11.5 分钟（2 核）；`hello` 级程序编译 30ms，生成 16KB 可执行文件 |

几个数字放在一起看：编译器和测试已经有 37 万行，标准库只有 96 行，文档改动量与代码改动量相当。

## 2. 里程碑状态

里程碑定义来自 `docs/archive/development/post-governance-milestones.md`（M0–M6）。状态按代码实测和各 Spec 复核，不照抄账本。

| 里程碑 | 退出条件（摘要） | 现状 | 判断 |
|---|---|---|---|
| M0 交接与基线 | 交接可追溯、阻塞项处理 | 完成；P2 成本预算延期 | ✅ |
| M1A 多文件程序 | 三文件参数报告程序双宿主 build/run | PR46 完成。该程序用 `replace(&report.text, …)` 更新字段，刻意绕开了普通字段赋值，后者至今不能原生编译（probe p12b） | ✅（范围窄） |
| M1B 文本处理 | a：argv 词频；b：选定输入方式的文本版 | a 完成（0274、0295 Map 版）。b 未启动：String 长度/切片/分词、数字格式化、stdin/文件、IoError、退出码都没有 Guide 规则 | ◐ 约一半，剩下的需要先做语言决策 |
| M2B 借用结果 | 单来源借用结果、局部绑定、受控转发 | 普通 `borrow T from param` 可用（p26）；Map scoped access、多 Borrow 参数、同步实参已交付。receiver 来源 `borrow T from this` 在泛型类上被拒（q17，L0164）。10-14：PR #95 冻结"M2B 实用化收尾"5 项清单，0316–0318 已合入，清单尚未勾选 | ◐ |
| M3A 顺序集合 | 选定访问/增长/删除/清理合同 | add/removeAt/索引/运行时长度构造/take/drop 可用；但单文件模式下 `MutableList` 写入失败（p19），只有 project 模式可用 | ◐ |
| M3B Map | 键/查询/更新/扩容/迭代合同 | put/get/remove 可用并有 ASan/LSan；迭代、只读 Map 构造（0291 草案）未做 | ◐ |
| M4 安全验证 | 检测接线、随机生成、外审 | M4a Linux ASan/LSan、M4b 首片完成；macOS 动态检测、UBSan、外审未做 | ◐ |
| M5 度量与分发 | 成本基线、preview 包 | 候选包首片（0272）完成；性能基线、正式发布未做 | ◐ |
| M6 线程 | 转移线程 spawn/join | 未开始；`thread`/`channel` 在标准环境中未绑定（p27，L0080） | ❌ |

主线是"治理交接 → M0 → M1A → M1B → 实用版本"。按这条线，**下一站 M1B-b 在语言层面还没开工**；实用版本还需要 §3 中的基础能力大面积补齐。

## 3. 原生能力实测矩阵

每个 probe 是 1–6 行的独立程序。"单文件"为 `kovenc run x.ko`，"project"为同一源码放进单文件 project 后 `kovenc run --project`。
结果记号：✅ 运行输出正确；FE 前端诊断；BE 后端 lowering 拒绝；ICE 内部错误泄漏。

| 领域 | 程序 | 按 Guide | 单文件 | project |
|---|---|---|---|---|
| 标量 | Long/UInt/Byte 运算 | 合法 | ✅ | ✅ |
| | Int 溢出 | 应 abort | abort（无消息） | abort（无消息） |
| | Double 运算 | 合法 | BE | BE |
| | Char 比较 | 合法 | BE | BE |
| | `byteVal == 3` | 字面量不随对侧定型，报错 | FE L0085 | FE L0085 |
| | `a.toLong()` | Guide 无转换族 | BE（前端未报未知成员） | BE |
| 控制流 | while / 嵌套 break / 递归 | 合法 | ✅ | ✅ |
| | `if` 作值、无 subject `when` | 合法 | BE | ✅ |
| | `when (Int)`、`when (String)` | 合法 | BE | BE |
| | `i in 0..<10`、`for (i in 0..<5)` | Guide 未定义 range；for 应报 L0159 | BE | BE / ICE |
| | `when` 异类分支（Int 与 String） | Guide 规定合并为 `Any` | BE | BE |
| 类与值类型 | value class 复制、泛型 value class | 合法 | ✅ | ✅ / BE |
| | value class 只读方法（非泛型，返回 Int/Boolean） | 合法 | BE → ✅（10-14，0316） | ✅ |
| | 普通 class 方法、泛型 value class 方法、value class 方法返回 String | 合法 | BE | ✅ / BE / ✅ |
| | class `inout` 方法 | 合法 | BE | ✅ |
| | **字段赋值 `c.count = 5`** | 合法 | BE | BE |
| | **`inout x: Int` 参数内赋值** | 合法 | BE | BE |
| | `inout c: Counter` 参数 | 合法 | BE | BE |
| | `replace(&obj.field, v)` | 合法 | BE | ✅ |
| | 泛型 class + inout 方法 | 合法 | BE | BE |
| | companion 函数 / companion const | 合法 | BE / ✅ | BE / ✅ |
| | interface 实现调用、默认方法 + `super<I>` | 合法 | BE | ✅ |
| | interface bound 泛型函数 | 合法 | BE | BE |
| | value class `==` | Guide 未定义 | BE | BE |
| enum | payload + `when` + smart cast（Int payload） | 合法 | ✅ | BE |
| | 返回 enum 后 `when`（Int payload） | 合法 | ✅ | ✅ |
| | **无 payload enum 的 `when (c) { Color.Red -> … }`** | 合法 | BE | ICE |
| | String payload 的 `when` | 合法 | BE | ICE |
| | enum 方法内 `when (this)` | 合法 | BE | BE |
| | 自声明 `Result` + 后缀 `?` | 合法 | BE | BE |
| | `is Result.Ok` 省略类型实参 | Guide 未明说 | FE L0091 | FE L0091 |
| nullable | `Int?`、nullable 参数判空、Elvis | 合法 | BE | BE |
| | `make()!!` | 合法 | BE | ✅ |
| | 对 nullable 用 `?` | 应为前端类型错误 | BE | ICE |
| 闭包 | lambda 绑定到局部再调用 | 合法 | BE | ✅ |
| | lambda 直接作实参（有无捕获都试） | 合法 | BE | BE |
| | 函数引用 `::inc` | 合法 | BE | BE |
| | move 闭包返回 | 合法 | BE | BE |
| | 闭包修改捕获变量 | 非法 | FE L0135 | FE L0135 |
| 集合 | `Array<Int>(n) { … }`、`List` 索引、Borrow 参数遍历 | 合法 | ✅ | ✅ |
| | `MutableList<Int/String>` add/索引赋值/for | 合法 | BE / InvalidSsa | ✅ |
| | `removeAt`、`MutableMap` put/get | 合法 | ✅ | ✅ |
| 所有权 | `deinit` 逆序、return 路径清理 | 合法 | ✅ | ✅ |
| | `borrow T from param` | 合法 | ✅ | ✅ |
| | 泛型类 `borrow T from this` | 合法 | FE L0164 | FE L0164 |
| | 链表迭代遍历 `cur = cur.next` | Guide 下不可表达 | FE L0133 | FE L0133 |
| | 链表递归遍历（nullable 参数） | 合法 | BE | BE |
| 堆与共享 | `Box<enum>` 递归 + `.value` | 合法（Guide 写明"计划中"） | BE | BE |
| | `Rc<Int>` share/value；`Rc<Data>` 字段投影 | 合法 | ✅ / BE | ✅ / BE |
| 文本 | String `+`/`clone`/`==`、`var` 重赋值 | 合法 | ✅ | ✅ |
| | `println(Int)` | 只有 `println(String)` | FE L0084 | FE L0084 |
| | 字符串插值 | native 明确拒绝 | BE | BE |
| | `s.length` | 未定义 | BE（前端未报） | BE |
| 标准环境 | 直接使用 `Result`、`Pair` | Guide 视为核心类型 | FE L0080 | FE L0080 |
| | `1 to 2` | 合法 | BE | BE |
| | `thread { … }` | Guide 13 定义 | FE L0080 | FE L0080 |
| | 顶层 `var` | Guide 未定义 | BE | BE |
| 诊断 | 不存在的成员 `a.fooBar()` | 应为前端诊断 | BE | BE |
| | `error("boom")` | abort | 退出码 1，**不打印消息** | 同左 |

汇总：85 个 probe 分三类。

- **按 Guide 应合法（71 个）**：

  | 模式 | 可运行 |
  |---|---|
  | project | 35 / 71（49%） |
  | 单文件 | 25 / 71（35%）；10-14 为 26 |
  | 任一模式 | 37 / 71（52%） |
  | 两种模式都可 | 23 / 71（32%）；10-14 为 24 |

- **按 Guide 应被前端拒绝（9 个）**：只有 4 个在两种模式下都得到前端诊断；其余 5 个（未知成员 ×2、未定义转换、for range、对 nullable 用 `?`）漏到后端或泄漏 ICE。
- **Guide 未定义或自相矛盾（5 个）**：顶层 `var`、`in` range、value class `==`、`when` 合并到 `Any`、`is` 省略泛型实参。这些应先在 Guide 里定规则。

样本是有意挑选的"普通 v1 代码"，偏基础，不代表全部规范的覆盖率。但基础程序都只有一半能跑，说明问题不在边角。

## 4. 结构性发现

### 4.1 两套并行管线，能力互相不覆盖

单文件和 compilation-unit 各有一套类型检查、所有权检查和 lowering：

| 阶段 | 单文件 | compilation-unit |
|---|---|---|
| 类型检查 | `type_checking/checker/` 约 1.28 万行 | `type_checking/compilation_unit/` 约 2.39 万行 |
| 所有权 | `ownership_checking/checker/` 约 1.91 万行 | `ownership_checking/compilation_unit/` 约 1.45 万行 |
| lowering | `ssa/lower_frontend/` 约 1.26 万行 | `ssa/unit_lower/` 约 1.76 万行 |

两套的能力并不嵌套：enum payload `when`、泛型 value class 只有单文件能跑；方法调用、`if` 作值、MutableList 写入、`replace` 字段只有 project 能跑。
每个新能力事实上要实现两遍、验收两遍。治理计划曾把"全面合并 single/unit"判为"不能作为无限前置任务"，但从实测看，它是后续每个切片成本翻倍的直接原因。

### 4.2 lowering 按组合特判，而不是通用降级

`ssa/unit_lower/control.rs:238–249` 中，`when` 有 subject 时只接受 Boolean 或非 MoveOnly 的 tagged enum，其余一律 `UnsupportedNode`。
文件清单里有 `map_loop_owner`、`map_with`、`map_require`、`field_replace`、`nullable_comparison`、`non_null_assertion` 等以"类型 × 结构"命名的模块。
近期 Spec 的范围也印证了这一点：0310"single Map 条件 break"、0309"普通 Borrow value-class 字段"、0308"普通泛型 String 回归"。
`while` 里的 `break` 能用，但"Map 在循环里遇到条件 break"要单独一个 Spec，原因是 owner 跨 CFG 的运输没有通用机制。

一个前端已经发布了完整 move/loan/drop 事实的编译器，lowering 本可以是通用的：先把所有权事实细化成显式的 move/drop/loan 指令和 CFG（类似 MIR），
再统一降到 SSA。现在的 typed SSA 已有 verifier，缺的正是这一层通用细化。

### 4.3 前端放行与内部错误

> 10-15 状态：SPEC-0319 新增共享最终类型准入与 L0165，下列前两条已修，原 ICE 全部消失；但伴生函数调用与 `?` 后的 `Result.Ok` 出现 3 处新的内部错误，第三条（`Any`）仍在。详见 [10-15 增量审计](2026-10-15-incremental-review.md) §3。

- 不存在的成员、未定义的转换函数不在前端报错，而是以 Deferred 形式流入后端。
- `for (i in 0..<5)`（Guide 要求 L0159）、`x?` 用于 nullable（Guide 要求类型错误）、无 payload enum 的 `when` 在 project 模式下报 `ownership checking failed: invalid compilation-unit argument …`，这是内部不变量失败泄漏给用户。
- `when` 异类分支被前端按 `Any` 接受，后端再拒绝（对应审计文档 C1）。

### 4.4 标准环境

- `Result`、`Pair` 只定义在 `prelude.ko` 内部，用户程序直接使用得到 L0080。Architecture 写"Pair/Result 已接入"，容易误读为用户可用。
- `error(msg)` 退出码为 1 但不打印消息，Guide 13 明确不保证打印。对任何真实程序的调试都是负担。
- `kovenc` 只有 `format`、`build`、`run` 三个子命令，没有 `check`（只做前端检查）和 `--help`。

### 4.5 流程成本

- 一个 Spec 的标准动作：冻结合同 → 红测 → 实现 → 双宿主 CI → 验收账本 → 归档 → 同步 inventory → 交付账本 → 主干 CI 复核。
  对"给 `when` 加一种 subject 类型"这一级的改动，这套流程的固定成本占了大头。
- 语言语义先启用、实现后补，Guide 正文因此有四十余行在描述 native 范围、SPEC 编号和验收状态（见审计 §4）。
- 文档漂移（10-15：PR #100 已按实际能力改写 README 与 tour，本条基本关闭）：`README_CN` 仍写"合入时已接收 main `a83749f` 的 PR46"；教程标题仍是"Koven v0.40 可执行 tour"；`evolution-status.md` 停在 v0.40 基线；
  README 的"亮点"列出闭包、模式匹配、smart cast，而这些在原生编译上大多不可用。

### 4.6 做得好的部分

- 确定性和可复现性做得很扎实：诊断排序、Span、恢复都有严格测试，`check_docs.py` 一类门禁有效。
- 已支持的路径质量高：`deinit` 逆序、return 路径清理、借用参数、MutableList/Map 的资源释放都有 ASan/LSan 证据，实测行为正确。
- 编译速度快，产物小，Linux/macOS 双宿主 CI 稳定。
- M4b 的随机生成 + 缩减设施是少见的投入，后续可以直接用于回归。

## 5. 建议的下一步（按顺序）

1. **止血（1 个切片）**：在 ownership 入口前加硬门禁，任何 Deferred/Error 节点都必须先有前端诊断；未知成员报诊断；三处 ICE 转为结构化诊断。
   同时把 `Result`/`Pair` 注入用户可见的标准环境，`error()` 打印消息到 stderr。
2. **合并管线**：单文件 = 只有一个文件的 compilation unit。以 project 管线为准补齐单文件独有的能力，然后删除单文件管线。完成后每个新能力只做一遍。
3. **通用化 lowering**：引入所有权细化层（显式 move/drop/loan + CFG），让控制流 × 类型的组合不再需要单独切片。用 §3 的矩阵作为回归集，目标是"合法程序要么跑通、要么有明确的未实现诊断"。
4. **调整流程粒度**：一个 Spec 对应一个用户可见能力；验收证据以 conformance 矩阵为主，账本从简。Guide 只在语义变化时升版（见审计 §4）。
5. **然后再推进 M1B-b**：先做 v1 范围文档中的语言决策（文本、IO、转换、相等协议），再实现。

完成 1–3 之前，不建议继续扩展 N1a/N1b、Map 新访问形态或更多借用结果形态。

## 附：复现方式

probe 源码与脚本未入库，放在本地 `.local-artifacts/review-2026-10-13/`（已被 `.gitignore` 排除）：
`run_probes.sh` 跑单文件模式，`run_probes_unit.sh` 跑 project 模式，`matrix.sh` 生成 `matrix.tsv`。
