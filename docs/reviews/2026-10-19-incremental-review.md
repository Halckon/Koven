# Koven 增量审计：2026-10-17 → 2026-10-19（含 D10 进行中分支）

> 区间：main `a2a7e4b6`（10-17 20:19，PR #118）→ `c0c5156a`（10-19 16:31，PR #119）；另审 D10 分支 `feature/spec-0341-shared-lowering-core` 至 `0a113082`（10-19 18:41，未合并，16 个提交）
> 上一版：[10-17 增量](2026-10-17-incremental-review.md) · [v1 范围](2026-10-13-v1-scope.md) · [里程碑 review](2026-10-13-repo-milestone-review.md)
> 方法：读 Spec 0331、0333–0341 与 D10 分支 diff；用三个 release `kovenc`（`a2a7e4b6`、`c0c5156a`、`0a113082`）跑同一套 144 个 probe（原 122 个 + 本轮新增 `w01–w19`、`x01–x03`），单文件与 project 两种模式逐条对比。
> 代码量口径：生产/测试按 crate 模块图划分（同 10-17 修订）；变更的生产文件里内联 `#[cfg(test)] mod {}` 块约 0.8k 行未扣除，生产行数是上限。
> 未做：`cargo test`、CI、sanitizer、macOS。D10 分支各片的测试结论引自 Spec 0341 账本，本轮未复跑。
> 后续：[10-22 增量审计（PR #120）](2026-10-22-incremental-review.md)。**2026-10-22 更新**：§4.3 已按选项 1（统一管线）定案，单文件入口已切换，两模式差异降为 0；§3.1 随之消失。

## 0. 结论

1. **D10 已经开工，但 Spec 0341 定义的 D10 与路线图里的 E2/E3 不是同一个目标。** 路线图（本地 `docs/roadmap/`）把 E2 定为"单文件走 compilation-unit 管线，删除重复的 checker/lower_frontend"，验收是"矩阵中单文件与 project 结果完全一致"。0341 则保留两个入口，把共同算法抽成共享核心、入口做薄适配，并明文写"共享不表示支持面取并集""不要求两入口生成完全同形 SSA"，范围也只限 `lang-codegen::ssa`，不含前端的两套 checker。按 0341 完成，两种模式的能力差不会因此消失。这一点需要先确认。
2. **D10 已完成的 5 片行为保持良好，但还只触及叶子算法。** 共享核心约 330 行生产代码（常量映射、整数 intrinsic、普通语句顺序、control-tail、checked 标量 CFG）。两条 lowering 各删约 150 行重复逻辑，又各加约 150–190 行端口实现，合计行数基本不变。
   144 个 probe × 2 种模式，D10 分支与 main 结果逐条相同。
3. **同一区间里，main 上的 0331（Map 借用遍历）又往两条 lowering 各加了一份大实现**：Map 遍历专用代码单文件约 12.4k 行、project 约 10.8k 行。区间生产代码约 +2.8–2.9 万行，其中两条 lowering 共 +2.36 万。D10 开工前，要合并的重复代码量增加了约 45%：单文件 lowering 2.61 万 → 3.88 万，project 2.58 万 → 3.67 万。
4. **Map 遍历是本区间唯一的用户可见新能力，但两种模式差距很大。** 11 个合法 Map 遍历 probe（v15、w01–w05、w07–w11）中，project 能跑 10 个，单文件只能跑 4 个。单文件模式下，只要在 Map 写入和 `for` 之间声明任何局部变量就失败（§3.1）。
   144 个 probe 中两种模式结果不一致的从 19 个增至 31 个（只有 project 能跑 13 → 24，只有单文件能跑 6 → 7）。原 85 个基础 probe 的数字没有变化。
5. **两个需要拍板的点**：D10 的完成标准（§4.3），以及 D10 期间是否冻结两条管线上的新组合（0333–0340 已登记 8 个后续片，其中 5 个是普通语言路径的缺口，不是 Map 专属的）。

## 1. 本区间交付

### 1.1 main

| PR | Spec | 内容 | 生产 / 测试（行） | 对应条目 |
|---|---|---|---|---|
| #119 | 0331 | Map/MutableMap 双分量借用 `for ((k, v) in m)`，插入序遍历 | 约 2.8–2.9 万 / 约 12 万 | v0.44 F，审计 L13；10-17 §3.1、§3.3 |
| — | 0333 | 静态 Map 返回调用（approved，未实施） | — | 0331 拆出 |
| — | 0334–0340 | 7 份草案：String? ABI、Borrow 结果的 caller continuation、函数值 Map 结果、条件 owner、Never 条件求值、Unit 实参、nullable 名义类型 retest | — | 0331 拆出 |

生产代码分布：单文件 lowering +12.7k，project lowering +10.9k，frontend +3.4k，SSA 共享层 +1.6k，LLVM +0.3k。两条 lowering 的增量中，非 Map 遍历的部分只有约 0.45k。
0331 的 Spec 本身有 1,752 行、75 节，§3 验收清单 7 项均未勾选，按 §74–75 的"冻结有限履约表"归档为 done。Guide 本区间无改动。

### 1.2 D10 分支（Spec 0341，in-progress，未合并）

| 片 | 新共享模块 | 收敛的责任 | 生产 diff |
|---|---|---|---|
| B | `constant_materialization` | 常量值到 `Operation` 的四路映射 | 核心 134 行（含约 100 行内联测试） |
| C/D-1 | `expression_core` | 整数 `inv`/checked 转换的事实校验与流程 | 核心 113 行；两端口各约 70–85 行 |
| C/D-2 | `statement_core` | 普通语句的 cleanup 与 Unit 序列 | 核心 32 行 |
| C/D-3 | `statement_core` | control-tail 顺序 | 核心 18 行 |
| C/D-4 | `expression_core/checked` | checked 标量的 failure/success CFG 构造 | 核心 134 行 |

合计：生产 +779/−344（共享核心 +444，单文件 +142/−157，project +193/−150），测试 +888。

## 2. 实测对比

### 2.1 新 probe

| probe | 内容 | a2a7e4b6 单 / project | c0c5156a 单 / project |
|---|---|---|---|
| v15、w08 | owned 局部 Map 遍历，`println(k)` / `println(v)` | L0159 | ✅ / ✅ |
| w05、w16 | Borrow 参数遍历，循环内 `return` 或外层 `var` 累加 | L0159 | ✅ / ✅ |
| w01、w02、w03 | 累加、插入序、覆盖保位 + remove 后重插置尾 | L0159 | **BE** / ✅ |
| w04、w10 | `break`、`continue` | L0159 | **BE** / ✅ |
| w07 | 同一 Map 嵌套共享遍历 | L0159 | **BE** / ✅ |
| w11 | 词频统计后遍历找最大值 | L0159 | **BE** / ✅ |
| w06 | 遍历中 `m.put` | L0159 | L0135 / L0135（正确拒绝） |
| w09 | `for (… in make())`，用户函数返回 MutableMap | L0159 | ✅ / **BE** |
| w17、w19 / w18 | 局部变量声明在 `put` 与 `for` 之间 / 声明在 Map 之前 | L0159 | BE / ✅、✅ / ✅ |
| x01 | `Node?` 判空后传给 `use(n: Node)` | ✅ / InvalidSsa | 不变 |
| x02 | 函数返回 `String?` | BE / BE | 不变（0334） |

单文件模式的 BE 全部是 `MissingFact`，以 "native object InvalidModel" 的形式报出。

### 2.2 原有 probe

原 122 个 probe 在 `a2a7e4b6` 与 `c0c5156a` 之间只有 v15 变化（L0159 → ✅/✅）。v03、v13、v16、v19、s01/s02、p45 均未变。

### 2.3 汇总

| 指标 | a2a7e4b6 | c0c5156a | 0a113082（D10） |
|---|---|---|---|
| 74 个合法基础程序：project / 单文件 / 两者 | 38 / 32 / 29 | 38 / 32 / 29 | 同左 |
| 8 个应拒绝程序得到前端诊断 | 8 | 8 | 8 |
| 144 个 probe：project / 单文件可运行 | 66 / 59 | 84 / 67 | 同左 |
| 只有 project / 只有单文件能跑 | 13 / 6 | **24 / 7** | 同左 |
| ICE/ERR | 0 | 0 | 0 |
| `InvalidSsa`（lowering 产出被 verifier 拒绝） | 6 | 6 | 6 |

D10 分支与 main 的 288 条观察完全一致。

**口径更正**：10-17 报告写"内部错误从 3 降到 0"，只统计了 ICE/ERR。`InvalidSsa` 表示 lowering 生成了非法 SSA，属于内部不变量失败，此前被归入 BE。当前有 6 条：p14b、p35、q05 双模式、q06 单文件、x01 project。

## 3. 新问题

### 3.1 单文件 Map 遍历：`put` 与 `for` 之间有局部声明即失败

```kotlin
fun main(): Unit { var m = mutableMapOf<String, Int>(); m.put("b", 2); val t = 1; for ((k, v) in m) { println(k) }; if (t == 1) { println("end") } }
```

单文件模式报 `native object InvalidModel: frontend lowering failed with MissingFact`，project 模式正常。把 `val t` 移到 `mutableMapOf` 之前就能通过（w18）。真实程序几乎都会在建表和遍历之间写别的局部变量，所以单文件模式下的 Map 遍历基本不可用：w01–w04、w07、w10、w11 都是这一个原因。
反过来，project 模式不支持用户函数返回的 temporary source（w09），单文件支持。
如果 D10 的方向是删除单文件 lowering（§4.3 选项 1），这里就不必在单文件侧修；否则需要单独修复。

### 3.2 lowering 生成非法 SSA 的路径仍有 6 条

见 §2.3。除 x01 外都是此前就存在的，但 10-17 报告没有列出。`InvalidSsa` 用的是 `InvalidModel` 措辞，用户看不出是编译器内部错误。建议纳入 E1：要么在 lowering 前用 UnsupportedNode 拦住，要么修复。

### 3.3 0331 拆出的后续片里，多数是普通语言路径的缺口

0334–0340 中，0334（`String?` 参数/返回 ABI）、0335（Borrow 结果的 caller continuation）、0338（Never 条件）、0339（Unit 实参）、0340（nullable 名义类型判空后读取）都与 Map 无关，只是在 Map 测试程序里先被撞到。x02 返回 `String?` 两种模式都失败，x01 在 project 模式出 `InvalidSsa`，都属于这一类。
这与 10-13 以来的判断一致：缺的不是某个组合，而是通用 lowering。照目前的做法，这些缺口会按"组合 × 管线"各自开片。

## 4. D10 审计

### 4.1 做得好的地方

- **方向已按用户决定启动。** 0341 的批准依据引用了"先合并两条管线、改用通用的代码生成"，D10 不再处于待决状态。
- **每片可独立回退，行为保持。** 144×2 条观察与 main 完全相同；各片保留原诊断、Span、能力门和清理时点，CI-only 的注入测试没有混进本地验收。
- **抽象边界克制。** 核心按领域做窄端口（例如 `IntegerLowering` 9 个方法），以 ID 类型为泛型参数，没有万能 context。核心不依赖 driver/planner/LLVM，并补了依赖方向的门禁。
- **checked CFG 片是第一次触及 CFG 构造**，删掉了两份完整的 failure/success 块构造，说明这条路走得通。

### 4.2 风险

1. **目标与路线图不一致（最重要）。**

   | | 路线图 E2/E3 | Spec 0341 |
   |---|---|---|
   | 终态 | 单文件走 unit 管线，删除 `checker` 与 `lower_frontend` | 两个入口长期保留，共享核心 + 薄适配 |
   | 范围 | 前端两套 checker + lowering | 仅 `lang-codegen::ssa` |
   | 能力差 | 验收要求单文件与 project 矩阵一致 | "共享不表示支持面取并集"，已知能力差保留 |
   | 通用 lowering（E3） | 用通用所有权细化层取代按组合特判 | 未列入；只把已有算法搬成一份 |

   0341 的终态仍有两个入口：前端保留两套 checker（单文件类型检查约 1.42 万行、所有权检查约 1.50 万行；unit 约 2.49 万行、约 1.67 万行），共享核心再配两份端口实现。以后每加一项能力，仍要在两侧各写端口、各自验收。
2. **收敛速度赶不上重复代码的增长。** D10 五片收敛了约 300 行重复逻辑；同期 0331 在两条 lowering 里新增约 2.3 万行 Map 专用代码。0333–0340 若在 D10 期间继续按两条管线实施，差距还会扩大。
3. **从叶子开始，难以触及主要的重复。** 重复主要集中在 `map_iteration`（单文件 12.4k、project 10.8k）、`loop_control`、`nullable_*_cleanup` 等按组合拆分的模块里。0341 的阶段 D 写明这些 CFG/cleanup 状态的差异要先"建立语义接口与状态角色映射"再抽取，难度和工作量远大于目前已完成的五片。
4. **单文件与 project 的能力差没有被当作指标。** 两模式不一致的 probe 从 19 增至 31。0341 的验收项里没有任何一条会让这个数字下降。

### 4.3 需要确认：D10 的完成标准

两条可行路线：

1. **入口合并（路线图 E2 原意）。** 让 `kovenc run file.ko` 把单文件包装成单源 compilation unit，走 `lower_unit_from_facts`。先修 unit 侧缺少、单文件已有的能力，再删除单文件 checker 与 `lower_frontend`（三者生产代码合计约 6.8 万行，是否还有其他消费者需先核实）。
   本轮矩阵中的 project 列，就是单文件包装为 project 后的结果：改走 unit 能多跑 24 个、少跑 7 个。少跑的 7 个是 p09、p49、u01、u04（enum payload/case 相关）、q18（泛型 value class）、w09（temporary Map source）、x01（nullable retest，`InvalidSsa`）。
   主要成本是现有大量单文件入口的测试需要迁移，这需要先盘点。
2. **共享核心（0341 现路线）。** 能消除算法重复，但终态保留两个入口与两套前端。若选这条路，建议在 0341 的验收里补两项可度量指标：(a) 两模式不一致的 probe 数逐片不增，最终为 0；(b) 两条 lowering 剩余的重复代码量。并把 `map_iteration` 列为阶段 D 的首个大域。

无论选哪条，都建议 D10 期间冻结两条管线上的新组合实现（0333–0340 先登记、不实施），否则 D10 永远在追赶。
还有一个可以先做的小实验：用约一天把单文件入口临时改走 unit 管线，跑一遍全部测试，看失败的数量和类别，再决定路线。

## 5. 上一版建议跟进

| 条目 | 10-17 | 10-19 | 说明 |
|---|---|---|---|
| E1 前端门禁、ICE 结构化 | ✅ | ◐ | ICE/ERR 仍为 0，但有 6 条 `InvalidSsa`（§3.2） |
| E2 管线合并 | ❌ | ◐ | 0341 开工，未合并；目标与 E2 原意不同（§4.2） |
| E3 通用所有权细化层 | ❌ | ❌ | 0341 未列入；0331 继续按组合实现 |
| E4 `Result`/`Pair` 注入、`kovenc check` | ❌ | ❌ | s01/s02 仍 L0080 |
| 10-17 §3.1 Map `for` 报 L0159 | 待修 | ✅ | 已实现；遍历中修改正确报 L0135 |
| 10-17 §3.2 语句 if/when 差异（待决设计点） | 待决 | 待决 | 未变 |
| 10-17 §3.3 Guide 走在实现前面 | 待修 | ✅ | 0331 交付 |
| 10-17 §3.4 README/教程 | 待修 | ◐ | 已补 Map 遍历说明；Range/`for`、转换、`consume` 仍未写进"当前原生子集" |
| 审计 L13 Map 迭代 | ◐ | ◐ | project 可用；单文件见 §3.1 |
| v03、v13、v16、v19 | BE | BE | 未变 |
| L12 Abort 输出消息 | ❌ | ❌ | p45 未变 |
| D10 | 未决 | 已决、进行中 | 完成标准待确认（§4.3） |

## 6. 下一步建议

1. **确认 D10 的完成标准**（§4.3 二选一），并据此修订 0341 或路线图 E2/E3 的验收条目。建议先做入口合并的一天实验，用测试失败数据做决定。
2. **D10 期间冻结两条管线上的新组合**：0333–0340 保持登记状态，不实施。
3. **把两模式不一致数和 `InvalidSsa` 数加进每片的验收记录**，作为 D10 的进度指标。
4. **README"当前原生子集"补上 Range/`for`、整数转换、`consume`**（10-17 遗留，成本很低）。

## 附：复现

- 新增 probe `w01–w19`、`x01–x03` 放入本地 `.local-artifacts/review-2026-10-13/probes/`。矩阵 `matrix-a2a7e4b6-144.tsv`、`matrix-c0c5156a.tsv`、`matrix-0a113082.tsv` 在同一目录。
- `bash stats.sh <tsv> v044` 输出 §2.3 的前两行指标；两模式不一致数按 `($2~/^OK|abort/) != ($3~/^OK|abort/)` 统计。
- 生产/测试划分脚本沿用 10-17 修订时的模块图方法：从各 crate 的 `lib.rs`/`main.rs` 沿 `mod`、`#[path]` 遍历，`#[cfg(test)]` 下的文件计为测试。
