# M1A 多文件参数报告工具：Spec 起草材料

> **性质**：待编号 Spec 起草材料 · **状态**：draft / 等待治理交接 · **读取时机**：评审首个后继程序的范围、源码与验收时 · **唯一真源**：本页维护 M1A 候选合同；启用后迁入正式 Spec

## 1. 状态与 Goal

| 字段 | 草案值 |
|---|---|
| 状态 | draft；尚未注册正式 Spec ID |
| 关联计划 | [治理完成后的开发里程碑](post-governance-milestones.md)的 M1A |
| 所属 Phase | Phase 2–4 的事实/执行及 Phase 6 的 project 验收 |
| 起草依据 | 用户于 2026-10-04 要求文档起草，代码开发等待另一位 agent 完成治理计划 |
| 规范入口 | [Guide v0.40](../guide/README.md)、[名称与文件](../guide/02-names-files-packages.md)、[所有权](../guide/10-ownership-borrowing-drop.md)、[顺序集合](../guide/12-collections-destructuring.md) |
| 阻塞项 | 计划 G0–G3 未完成；真实红测、最终实现入口和正式编号待交接后核定 |
| 语义变更 | 目标限于现行语义；若复核发现需要新规则，停止对应实施并另行决策 |
| 文档落点 | 当前留作起草材料；编号确定后迁入 `docs/specs/drafts/`，避免与并行治理分支冲突 |

Goal：一个三文件参数报告工具通过真实 CLI project 入口编译并运行，直接消费跨文件 class
可变字段、字段 Borrow 实参及 unit 顺序迭代，输出与清理符合现行规范。
本草案不包含生产实现、可执行测试文件或已通过声明。下列源码是待实测的候选验收输入。

## 2. 候选源码与项目形态

文件内容只保存在本节。正式实施时由现有教程/fixture 机制提取，或迁移为唯一 fixture，
不能在文档和测试中长期手工维护两份源码。优先接入治理交接后的现有多文件教程能力。

```text
project.toml
src/app/model.ko
src/app/processor.ko
src/app/main.ko
```

`project.toml`：

```toml
schema = "koven.project"
version = 1

[project]
name = "argument-report"
source-roots = ["src"]
```

`src/app/model.ko`：

```kotlin
package app

class Report(var text: String)
```

`src/app/processor.ko`：

```kotlin
package app

fun reportArguments(args: Array<String>): Unit {
    val report = Report("start")
    for (argument in args) {
        val previous = replace(&report.text, argument.clone())
        println(report.text)
    }
    println("processed")
}
```

`src/app/main.ko`：

```kotlin
package app

fun main(args: Array<String>): Unit {
    reportArguments(args)
    println("done")
}
```

`argument.clone()` 显式从 Borrow 参数取得一个可存入字段的独立 owner，属于程序需要的操作。
字段更新复用现行 `replace`；`previous` 接收旧 owner，并按未再使用的局部值规则清理。
普通 `report.text = ...` 在起草基线还受 unit 字段赋值 lowering 限制，属于独立后继需求，
本片不将其隐藏为三个已选缺口的一部分；必须另外验证未使用的旧 owner 恰好清理一次。
`println(report.text)` 必须直接 Borrow 字段；改成 `println(report.text.clone())` 会绕开本合同。
class 定义必须继续位于另一个 source，程序必须使用 unit `for`，不改写成 while 或单文件。
上述限制是本验收输入的覆盖要求，不改变语言对其他合法写法的接受规则。

## 3. 用户可观察结果

所有成功场景执行 build、生成的产物及 CLI run；CLI 命令遵循当前 project/entry/argv 合同：

```sh
kovenc build --project project.toml --entry app.main -o report
./report alpha 你好 tail
kovenc run --project project.toml --entry app.main -- alpha 你好 tail
```

| 输入 argv | artifact 与 run 的预期 stdout（UTF-8，`\n` 为换行） |
|---|---|
| 空 | `processed\ndone\n` |
| `alpha` | `alpha\nprocessed\ndone\n` |
| `alpha`、`你好`、`tail` | `alpha\n你好\ntail\nprocessed\ndone\n` |
| 一个空字符串参数 | `\nprocessed\ndone\n` |

成功 build 的 exit 为 0、stdout/stderr 为空；artifact 与 run 的 exit 为 0、stderr 为空。
这些是预期，不是已取得的结果。实际命令与完整输出在实施后的唯一验收表记录。
`processed` 与 `done` 分别证明循环后的函数体和 caller 继续执行，不能仅凭资源计数判定控制流正确。

## 4. 边界与必须经过的路径

| 领域 | 本片需求 | 不能替代的证据 |
|---|---|---|
| 跨文件可变性 | 字段声明身份与使用 source 不同；通过 replace 更新真正的 var 字段，保持 val/共享权限拒绝 | 将 class 挪回同文件、按字段文本名或裸局部 ID 查询 |
| 字段 Borrow | 正常 class 的 String 字段直接交付 Borrow callee，父 owner 覆盖整个调用 | clone 字段或先 replace 提取 owner 后调用 |
| unit 迭代 | 按现行封闭容器合同发布 source-qualified typed 与 ownership 计划，交给 SSA/native | 仅消除 Deferred、从 AST 猜清理、只让 single 路径成功 |
| 宿主交付 | 实际 project discovery、普通及 const 能力边界、object/link/run 与 argv | 单元测试直接构造 SSA 后称整个 project 已通过 |

起草基线中的代码入口：

- 可变性：[收集](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs)
  与[查询](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/places.rs)。
- unit for：[类型](../../crates/lang-frontend/src/type_checking/compilation_unit/bodies/checker/control.rs)、
  [ownership](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs)、
  [lowering](../../crates/lang-codegen/src/ssa/unit_lower.rs)。

这些是复核入口，不预定未来 helper 或公共 API。复用 source-qualified declaration/field facts，
优先补同轮事实查询与消费者；不通过放宽能力 gate 或复制整套 checker 达到运行目标。

## 5. 非目标

- 新增 Vec、Map、Set、Option、开放 Iterator/Iterable 或用户自定义 for provider。
- 借用返回、借用视图、完整 NLL、Str、通用 toString/格式化或文件 IO。
- 全面合并 single/unit driver、const/basic capability 或 concrete-type resolver。
- 普通 class 字段赋值 lowering；本片通过既有字段 replace 完成更新，不宣称 `report.text = ...` 已可执行。
- 借此扩展所有 nested/index/receiver/Inout/closure 组合；被直接改动影响的既有支持仍需回归。
- 改写已归档的 0182 目标、把原单文件验收直接算作 unit 验收。
- 运行 P2 成本实验、全部 frontend 测试或发布 release。

## 6. 单一验收矩阵

下列 ID 同时作为实施步骤的验证引用；全部尚未执行。已有 suite 为候选接入点，
最终过滤器和缺失测试的名称在代码实施前确定，不把计划中的名字写成已经存在的 target。

| ID | 要证明的合同 | 候选测试位置 / 执行入口 | 验收要求 |
|---|---|---|---|
| A1 | 源码真实性与首轮红测 | 上述三文件，经 CLI project；各缺口另用最小输入定位 | 记录失败阶段、code/kind、source/Span；不能把首个错误当成其他路径已验证 |
| A2 | 跨文件字段可变性 | `multifile_ownership_checking`、`ownership_field_replace`及直接受影响的 typed suite | var 正例、val/共享权限负例；同名旁源、同局部 ID、输入置换保持身份与诊断 |
| A3 | 字段直接 Borrow 与父 owner | unit Borrow/native 现有测试及所需相邻字段用例 | callee 实际读取；父对象不早 drop；结束后可继续合法使用；借用冲突仍拒绝 |
| A4 | unit 迭代 typed/ownership 事实 | `multifile_type_checking`、`multifile_ownership_checking`，缺少独立目标时再按职责确定 | source 求值一次、binding/projection 身份、loan/provider/清理顺序和最近 callable 边界；包含 Inout/字段 source 的前端正反例 |
| A5 | unit 迭代 SSA/native | codegen 相应 unit lowering/verifier/native suite | 三种现行容器；owned/Borrow 源、temporary；0/1/多元素；继续、break、continue、return及 Abort；Inout/字段 source 按首轮 Phase 4 边界明确拒绝 |
| A6 | 精确资源与控制流 | A3/A5 的动态计数用例 | 按 owner/指针核唯一释放与顺序；结束 provider 后才能结束 source loan；Abort 不 unwind；必须检查循环后或 caller 输出 |
| A7 | 能力/身份 gate 与失败原子性 | ordinary/const native 交接合同和 project CLI | 源与 facts 混轮/混 source 拒绝；失败保留旧产物和目录状态；成功路径确实发射/链接/运行 |
| A8 | 三文件公开行为 | `project_cli` 或交接后的多文件教程门禁 | 第3节所有完整输出通过，两受支持宿主实际执行；保持跨文件/直接字段/unit for 路径 |
| A9 | 不回归已有能力 | single 迭代、受影响字段/借用/捕获及能力消费者 | 依赖影响面选取；unsupported 的变化经合同批准，不能静默消失或扩大 |
| A10 | 工程与文档交付 | fmt、受影响 crate Clippy、公开交接变化时 workspace check、docs/尺寸门禁 | 按 testing.md 执行；更新真实 Architecture/Spec 账本，最后以精确 head CI 收口 |

正常循环元素仍属于容器，循环退出不额外析构元素；参数数组归调用者/entry owner 负责。
新字段 owner 的覆盖、借用、父对象清理和参数 owner 清理要分开验证，不能只看总 free 数。
字段 replace 返回的 `previous` 也须独立核对，不能遗漏旧字段或与父对象重复清理。
A4 对 Inout/字段 source 的语义覆盖不授予 A5 native 支持；此区别来自 Guide §37.4，
前端成功后仍按精确阶段和诊断验证后端拒绝，不把前端误拒绝当作后端负例通过。
A5 的拒绝与正例覆盖必须逐项映射当前 Guide；不能用矩阵格数代替实际命中。
若完整现行合同需分为多个 Spec，保留 M1A 的总退出条件，独立关闭已验收切片并明确剩余。

## 7. 实施顺序与后继迁移

1. 等待计划 G0/G1，接收确定 main；重新核对本稿各缺口、教程机制与并行 Spec 编号。
2. 完成 G2/G3，将材料迁成正式 draft/approved 合同，核定一项 Goal 的边界及分支名。
   必要拆分由实际阻塞决定，不在本轮预占多个编号；同步索引、inventory及依赖图。
3. 首先运行 A1 并建立各缺口最小红测，冻结预期和输入；若发现规范冲突先停止该项。
4. 完成 A2、A3，再按 typed→ownership→SSA/verifier→native 完成 A4–A7；每步先失败后通过。
5. 执行 A8，并按真实影响完成 A9/A10；相同输入和合同的成功证据可按工程规则复用。
6. 独立评审具体实现、最大剩余风险与验收表；修复发现并复审后，按授权提交和交付。

本起草分支只写 Markdown；不会把本节候选源码写入生产目录或现行可执行教程。
正式迁移时删除本起草材料并同批更新计划/索引链接，避免形成永久平行合同。

## 8. 待交接确认项与验证记录

- 治理交付 SHA、完成记录和允许保留的事项尚未取得。
- 编号、正式分支、具体测试过滤器及新增事实 API 待确定。
- 首轮最小复现可能显示额外的前置缺口；先说明是否属于本 Goal，再决定调整范围。
- draft 源码未编译；不得作为当前用户教程或已支持能力示例发布。

| 项目 | 状态 | 原因 / 后续处理 |
|---|---|---|
| A1–A10 实施与执行 | 未运行 | 按用户要求等待治理完成，当前只有文档起草 |
| 独立文档评审 | 两项发现修订后复核通过 | 核对治理门槛、规范与直接代码；明确字段 replace/旧 owner 清理，补齐 Inout/字段 source 阶段边界；未编译候选源码 |
| 首批 `python3 scripts/check_docs.py` | 当时 495 Markdown 文件结构检查通过 | 含两份新草稿；续写批次见[计划记录](post-governance-milestones.md#11-文档起草记录)，不代表 A1–A10 通过 |
| `git diff --check`、新文件空白/末尾换行检查 | 通过 | 新文件另行检查，因为尚未加入 Git 索引 |
