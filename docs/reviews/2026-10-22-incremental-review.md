# Koven 增量审计：2026-10-19 → PR #120（D10 统一管线）

> 区间：D10 分支 `0a113082`（10-19 18:41，上次审计点）→ PR #120 头 `aef3f4f8`（10-22 09:23，未合并）。main 仍为 `c0c5156a`，本区间无新合并。
> 上一版：[10-19 增量](2026-10-19-incremental-review.md) · [v1 范围](2026-10-13-v1-scope.md)
> 方法：读 Guide v0.45 diff、ADR-0031、v0.45 启用账本与 Spec 0341 §27–50；用 `c0c5156a` 与 `aef3f4f8` 两个 release `kovenc` 跑 154 个 probe（原 144 个 + 本轮新增 `y01–y06`、`e01–e04`），两种模式逐条对比；另手工验证 v0.45 的 import、root 与 `--source-root` 驱动合同。
> 代码量口径：同 10-19，按 crate 模块图划分生产与测试。
> 未做：`cargo test`、CI、sanitizer、macOS。PR #120 新头的 CI 结果本轮无法从云端查询，未核实；按 0341 §50，旧远端头 `054a74d` 的 PR CI 双宿主各失败 25 项。

## 0. 结论

1. **D10 已按"统一管线"终态重新定义，并完成了最关键的一步：单文件 CLI 与 LSP 已切到 compilation-unit 管线。** 0341 §1 改写为"单文件和 project 共用名称、类型、所有权检查及代码生成，旧 single checker/`lower_frontend` 退役"。Guide v0.45、ADR-0031 与评审建议一致：单文件允许 import、从 package 反推 root、`--source-root` 可选、标准环境相同、零模式差异与零 InvalidSsa 作为切换门槛。
2. **实测：两种模式不一致从 37 个降到 0 个。** 154 个 probe 中，单文件可运行 71 → 92，project 86 → 92，两列逐条相同。v0.45 的驱动合同（import、L0148 未加载提示、L0146 路径校验、`--source-root`）实测符合 Guide。
3. **但有 5 个原来单文件能跑的程序现在两种模式都失败，属于 0341 明令禁止的能力回退。** 根因只有一个：enum subject 的 `when`，分支结果为 Boolean、String 或 Unit 时，Unit 管线报 MissingFact；分支结果为 Int 时正常（§3.1）。
   仓内的 39 场景矩阵记录"原成功损失 0"，因为它不含这些源码。
4. **InvalidSsa 未清零。** 原 144 个 probe 中从 6 条降到 4 条（p14b、q05 两种模式）。新增 probe 又发现一条：`for (x in take(xs, 2))` 直接遍历 temporary View 时报 InvalidSsa，两种模式都有，此前单文件不支持 import 所以没暴露（§3.2）。
5. **旧实现在生产路径上已不可达，但 6.8 万行还在。** `lower_frontend` 约 3.87 万行仍以普通 `mod` 编进生产，唯一出口是 `#[cfg(test)]`；整个 `ssa` 模块挂着一条 SPEC-0033 时期的 `#[allow(dead_code)]`，所以编译器不报。旧类型/所有权 checker 约 2.9 万行同样只剩测试与旧 lowering 在用。
   删除是下一步的主要工作，前提是先清掉 §3.1 与 §3.2。

## 1. 本区间交付（PR #120，未合并）

| 0341 节 | 日期 | 内容 |
|---|---|---|
| §18–26 | 10-19 | 继续抽共享核心（temporary/capture/result-source alias carry、incoming 列布局） |
| §27 | 10-19 | **目标纠偏**：共享核心只算迁移，终态为统一管线；22 个附件源码隔离接线实验 |
| §28–35 | 10-19 | 修 Unit 侧回退：x01 nullable class、enum case 条件、w09 temporary Map、concrete generic enum |
| — | 10-19 | **Guide v0.45 启用**（02/05/13/15 实改，其余页只改版本）、ADR-0031、v0.44 全文归档 |
| §34–40 | 10-19 | 统一 driver 前置：project 编排抽为共同入口、已解析名称续接、未加载 import 说明、entry 选择基线 |
| §41 | 10-20 | **单文件 CLI 切到 compilation-unit 管线** |
| §42–47 | 10-20 | CI 回退修复（generic value class、companion 能力诊断）、tutorial oracle 迁移、**LSP 单文档切到 Unit** |
| §48–49 | 10-20 | 退役独立 SingleFile 门面与旧 native 门面（`NativeEntry`、`emit_native_object`）；补 Unit 能力：pending 构造 owner、nullable Rc 判空、逗号 `when`、关联常量 qualifier；顶层 runtime 变量改为明确的 backend 能力门 |
| §50 | 10-22 | post054 修复（整数语句/Map 写角色、Resource Map 有界循环、nullableWhen 等），有限收尾检查点 |

规模：217 个提交；自上次审计点起，生产 +10.2k/−2.7k（frontend +4.5k，unit lowering +3.7k/−0.7k，CLI +1.1k/−0.7k，LSP +0.3k/−0.3k，旧单文件 lowering +0.2k/−0.2k 且只在 10-19 改过），测试 +23.5k。0341 本身 700 行。

## 2. 实测对比

### 2.1 汇总

| 指标（154 个 probe） | c0c5156a（main） | aef3f4f8（PR #120） |
|---|---|---|
| 单文件可运行 | 71 | **92** |
| project 可运行 | 86 | **92** |
| 只有 project / 只有单文件能跑 | 26 / 11 | **0 / 0** |
| 两列诊断类别有差异 | 49 | **0** |
| InvalidSsa（观察次数） | 9 | 10 |
| 原单文件成功、现在失败 | — | **5** |

按 v0.44 口径的 74 个合法基础程序：project / 单文件 / 两者 = 38/32/29 → **40/40/40**；8 个应拒绝程序仍全部前端拒绝。

### 2.2 变化条目

- **单文件补齐到 project 水平**（26 个）：p12、p19、p29、p44b、q01、q06、q13、q14、q16、t08、t10、v20、v22；10-19 §3.1 的 Map 遍历 w01–w04、w07、w10、w11、w13、w14、w17、w19；以及因 import 生效而可运行的 y05、y06。
- **project 补齐到单文件水平**（6 个）：p09 enum payload、q18 泛型 value class、u04、w09 temporary Map source、x01 nullable retest（原 InvalidSsa）、e04 enum `when` 返回 Int。
- **两种模式都失败的回退**（5 个）：p49、u01、e01–e03，见 §3.1。
- **诊断改善**：p28 companion 调用从 MissingFact 变成明确的 "companion function calls are not supported by native code generation"。p23 顶层 `var` 两种模式统一为 L0165。

### 2.3 v0.45 驱动合同

| 场景 | 结果 |
|---|---|
| 无 package 文件 + `import koven.algorithms.take`，`borrow val v = take(xs, 2)` 后遍历或传参（y05、y06） | ✅（旧单文件 L0080） |
| 未 import 直接用 `take`（y02） | L0080 ✅ |
| `import app.util.helper`（未加载用户 package） | L0148 + "package 'app.util' is not loaded" + `--project` 提示 ✅（旧单文件忽略 import，直接运行） |
| `x/a/b/F.ko` 声明 `package a.b` | ✅；`--source-root x` ✅；`--source-root x/a` L0146 ✅ |
| `x/c/G.ko` 声明 `package a.b` | L0146 ✅（旧单文件直接运行） |
| 在 `x/a/b` 目录内执行 `kovenc run F.ko`，或加 `--source-root ../..` | L0146（符合 Guide 字面，见 §3.3） |
| `import koven.nothing.here` | L0148，但提示 `--project`（见 §3.3） |

## 3. 新问题

### 3.1 能力回退：enum `when` 的非 Int 分支结果

```kotlin
enum class Color { Red, Green }
fun main(): Unit { val c = Color.Green; val b = when (c) { Color.Red -> true; Color.Green -> false }; if (!b) { println("e5") } }
```

`aef3f4f8` 两种模式都报 `InvalidModel: frontend lowering failed with MissingFact`。旧单文件（`c0c5156a`）能运行，旧 project 报 UnsupportedNode。
同形写法里，分支结果为 Int（`Color.Red -> 1`）或 `when` 内只赋值都能通过；分支为 Boolean、String，或语句位置的 `println`，都会失败。p49（返回 String 的 `= when`）、u01（返回 Boolean）都是这一类。
0341 §29/§35 修了 enum case 条件与 generic enum，但没覆盖这一形态。0341 和 Guide 15 都规定原成功能力不能回退，所以这一项应当在删除旧实现前修复，或者至少登记为阻断项。

### 3.2 InvalidSsa 仍有两类

- p14b（`val s: String? = …` 后判空）、q05（nullable 参数）：原 project 已有，现在单文件也一样。
- **新发现**：直接遍历 temporary View，例如 `for (x in take(xs, 2)) { … }`（y01、y03、y04），两种模式都是 InvalidSsa。先 `borrow val v = take(xs, 2)` 再遍历就正常。
  旧 project 也是 InvalidSsa，不是本 PR 引入的，但现在单文件也能写出这段代码，属于 v0.45 新可达的内部错误。

Guide 15 把"InvalidSsa 降为 0"列为切换门槛。生产入口已经切换，这两类应当作为删除旧实现前的阻断项。

### 3.3 v0.45 驱动的两处体验问题（不是规范偏离）

1. **在 package 目录里运行会失败。** Guide 02 规定"package 匹配使用指定路径的逻辑目录组件"，所以 `cd x/a/b && kovenc run F.ko` 必然 L0146，加 `--source-root ../..` 也不行，因为显式 root 只能"确认"反推结果，不能改写逻辑路径。
   建议考虑：给了 `--source-root` 时，按"文件相对 root 的路径"计算逻辑路径，这正是 project 中 source root 的含义。如果维持现规定，至少在 L0146 的说明里提示"请从 source root 执行，或改用 `--project`"。
2. **不存在的标准包也提示 `--project`。** `import koven.nothing.here` 的 help 让用户改用 `--project`，但 project 模式同样不会加载它。建议对 `koven.*` 前缀且不在 Guide 02 允许清单里的 package，改为"不是可用的标准库 package"。

### 3.4 旧实现靠一条过期的 `dead_code` 豁免留在生产编译里

`lang-codegen/src/lib.rs` 对整个 `mod ssa` 和 `mod llvm` 加了 `#[allow(dead_code)]`，注释写的是"SPEC-0033/0034 接入后移除"。现在 `lower_frontend`（123 个生产文件、约 3.87 万行）只有 cfg(test) 出口，编译器却不会提示它已不可达。
建议在删除旧实现时一并去掉这两条豁免，让编译器帮忙确认没有遗漏的生产消费者。

## 4. 建议跟进

| 条目 | 10-19 | 10-22 | 说明 |
|---|---|---|---|
| D10 完成标准 | 待确认 | ✅ 已定 | 0341 §1、Guide 15、ADR-0031 |
| E2 管线合并 | ◐ | ◐（入口已切换，旧实现未删） | 模式差异 0；回退 5、InvalidSsa 10 未清 |
| E3 通用 lowering | ❌ | ❌ | 0341 §1 明确保持开放；本区间 Unit 修复仍按形态有界接入（如"own `Rc?` 根、顶层无 else `!= null`"） |
| 10-19 §3.1 单文件 Map 遍历 | 新问题 | ✅ | 随入口切换消失 |
| 10-19 §3.2 InvalidSsa | 6 | 4（原 144）+ 6（新 probe） | §3.2 |
| E4 `Result`/`Pair` | ❌ | ❌ | 按 v0.45 明确不变，s01/s02 仍 L0080 |
| 0333–0340 冻结 | 建议 | ✅ | 0341 §5 列为不自动关闭，本区间未实施 |
| README | ◐ | ◐ | 已删"单文件不加载标准源"；Range/转换/`consume` 仍未写进"当前原生子集" |

0341 §1 写道"原审计 E2/E3 编号正文尚未定位"。E2/E3 的原文在 [v1 范围](2026-10-13-v1-scope.md) 和本地 `docs/roadmap/` 中，两者都只在你本地、未提交，所以云端执行者找不到。§27 也提到 144 项矩阵"没有完整源"。建议把这两份文档和 probe 语料一起提交进仓库，否则零差异门槛只能在 22 个附件和仓内 39 场景上验证，§3.1 这类回退会漏掉。

## 5. 下一步建议

1. **把 154 个 probe 源码纳入仓内普通回归**（经安全筛选后），作为切换门槛的固定集合；同时提交 `docs/reviews/` 中的 v1 范围文档，便于 0341 对账 E2/E3。
2. **先修 §3.1 与 §3.2**，再进入删除阶段。§3.1 只有一个根因，成本应该不高。
3. **删除旧 checker 与 `lower_frontend`**，一并去掉 §3.4 的 `dead_code` 豁免；把仍有价值的旧测试改到 Unit 入口，不保留旧实现做对照。
4. **决定 §3.3 的 `--source-root` 语义**（若要改，属于 Guide 02 的规则修订，需要单独确认）。
5. 删除完成后，再启动 E3（通用 lowering）的设计；届时每项修复只需做一次。

## 附：复现

- 新增 probe `y01–y06`（标准库 import）、`e01–e04`（enum `when`）放入本地 `.local-artifacts/review-2026-10-13/probes/`；矩阵 `matrix-c0c5156a-154.tsv`、`matrix-aef3f4f8-154.tsv` 在同一目录。
- 驱动合同的手工场景（package 目录、`--source-root`、未加载 import）不在矩阵中，命令见 §2.3。
