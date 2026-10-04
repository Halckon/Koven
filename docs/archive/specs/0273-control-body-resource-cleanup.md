# SPEC-0273: 单文件控制体正常退出的资源清理

> **性质**：有界修复合同 · **状态**：done · **读取时机**：修复或验收嵌套 if 资源 InvalidSsa 时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-273` |
| 所属 Phase | Phase 4 typed SSA/LLVM/native；关联 Phase 6 CLI 验收 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[资源词法析构](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户报告嵌套 if 内声明两个资源构建 InvalidSsa；此前持续实施、提交、PR 与合并授权 |
| 前置 Spec | SPEC-0245、SPEC-0270 |
| 前置 ADR | ADR-0006 |
| 影响范围 | single-file control body lowering、SSA/native 回归及实现快照 |
| 语言语义变更 | 否 |

## 1. Goal 与复现

合法的嵌套 if 在分支内声明多个资源，正常离开该词法作用域后能继续构建和执行；
资源只按前端发布的清理事实逆序析构，外层 guard 与分支返回值仍保持各自责任。
基线 main `299469bd7234149eaa233e49e6a90011cdedfd50`，分支 `fix/spec-0273`。

用户尚未提供原始源码；基于描述建立以下最小复现，使用该主干的真实 release CLI：

```koven
class Resource(val name: String) { deinit() { println(name) } }
fun main(): Unit {
    if (true) {
        if (true) { val a = Resource("a"); val b = Resource("b"); println("inner") }
        println("outer")
    }
    println("done")
}
```

`kovenc build <source> -o <program>` exit2，报 `frontend lowering failed with InvalidSsa`。
动态条件、内外层各一资源同样失败；内层显式 return 的对照成功。
四份原源码及实际命令/退出码保存在 `/private/tmp/koven-nested-resource-repro`。

## 2. 根因与最小修复边界

前端 drop planner 已在 `AfterStatement(ControlBody)` 发布资源正常 scope 退出事实。
单文件 `lower_control_body` 求值 prefix 和尾语句后直接返回，遗漏此 body 边界；
if 合流只运输分支前的 bindings，局部资源未消费导致 SSA 验证失败。
具体内部 `MissingOwnedExit` 当前仅由 verifier 代码静态推导，不冒充已观察的错误 dump。

复用 unit lowering 的既有顺序：保存尾语句结果，非 Diverged 时消费该 body 的
`AfterStatement` drop，再原样返回结果。所有权与析构顺序仍由 frontend facts 决定；
不通过 AST 推断需要释放谁，不补造 drop，不放宽 verifier。
ControlTransfer 已清理的 return/break/continue、Abort 不再执行正常出口清理。
共享此 helper 的 if/when/nullable 控制体按其现有支持范围验证，不能扩展未支持语义。

非目标：资源条件 owner 合流、nullable resource/泛型 wrapper、新借用规则、全量 frontend
改写、unit lowering 重构或 M4b 故障校准。SPEC-0270 的局部移动别名修复保持不变。

## 3. 验收与验证顺序

| ID | 必须完成 | 当前状态 |
|---|---|---|
| C1 | 新 SSA/native 用例先以 InvalidSsa 失败，再在最小修复后通过 | SSA首轮3失败/1通过、native失败；修复后SSA4通过/native1通过 |
| C2 | 动态 outer/inner 四组合；内层两个资源、每层各一资源，精确逆序清理与外层存活 | native单测完整执行3形状×4组合，实际stdout与逐指针计数通过 |
| C3 | 显式 return 不重复清理；MoveOnly 分支尾值交付保留结果 owner | SSA直接验证return与tail，native return四组合通过 |
| C4 | 共享 helper 的 when 与既有 nullable/control 回归、frontend facts 不改 | 新SSA两类when通过；lower_frontend_tests 70项通过，0 ignored；frontend未改 |
| C5 | 原始复现实际 CLI build/run，精确 stdout/stderr/退出码；native 逐指针释放账本 | 原始4份源码重新build/run全通过，3种形状逐指针计数通过 |
| C6 | 独立完整审阅、Clippy/fmt/尺寸/docs；双宿主 PR CI、归档最终 head CI 与合并 | 实现head双宿主CI及审阅/本地门禁已通过；归档后最终head CI与合并待完成 |

先直接失败测试，再最小实现，再定向 `resource_deinit_`、lower_frontend 控制与 nullable
相关测试；不默认运行 frontend 全量。多个过滤器有重叠，实际数量不相加。
native 四组合用同一源码动态执行，必须实际经过 inner/outer/done 输出，计数成功不能代替流程正确。
本地 Cargo 只占一个窗口；远端 CI 由轻量 agent 只读监视，主 agent 核对精确 head 后合并。

## 4. 尺寸与交付

`lower_frontend/control.rs` 基线 1443 行，存在历史尺寸欠账；本次只在已有控制体边界
消费缺失事实。登记最终精确增长及独立审阅，不提升历史 baseline，不机械拆分无关控制流。
测试继续放入现有低于软上限的资源回归文件，复用现有 lowering 与 native counter helper。

一个提交一个逻辑变更；实现与已执行证据同步，完成全部所选验收后再归档。
归档提交必须独立 CI 通过后合并，未执行或忽略的检查准确记录。

## 5. 当前记录

2026-10-04：只读调查已对照 frontend planner、single lowering、unit lowering 和 verifier，
确认两个入口的正常控制体清理差异；实现与新增测试进行中。尚未宣称修复完成。

实现定向证据：`cargo test --locked --offline -p lang-codegen --lib single_resource_deinit_control_body_`
首轮 3 failed/1 passed，修复后4 passed/826 filtered/0 ignored；新native单测首轮InvalidSsa，
修复后1 passed/829 filtered/0 ignored，其内部完整执行3形状×4动态条件组合。
`ssa::lower_frontend_tests::` 70 passed/760 filtered/0 ignored。实际总数以当前测试树为准，过滤范围有重叠。
生产仅 control.rs 1443→1449，六行增长例外已登记；完整独立审阅与综合回归进行中。

综合资源回归 `cargo test --locked --offline -p lang-codegen --lib resource_deinit_ -- --test-threads=1`
37 passed/793 filtered/0 ignored；codegen all-targets Clippy（--no-deps、-D warnings）通过。
Docs/尺寸 Python 合同84项通过，尺寸门禁报告772手写Rust、45历史超限、0生成登记，
本次精确六行例外通过、历史baseline未变；512页docs与diff检查通过。
独立完整审阅未发现生产或测试正确性阻塞；指出Phase标注错误，已按Guide15修为Phase4，
真实CLI工具验收关联Phase6。旧测试中无关空行亦已移除。

新构建 debug CLI 重放 `/private/tmp/koven-nested-resource-repro` 原始四份源码，8条实际
build/run命令全部通过：三个原失败场景转绿，显式return对照保持正确；完整stdout、空stderr、
exit0符合手写预期，证据与binary SHA256在 `/private/tmp/0273-cli-fixed/results.json`。
完整fmt通过；Phase归属和无关diff两项审阅发现均已窄复核关闭。
用户要求先通过PR同步本地main的23个独有提交再从最新main开worktree；本修复先保存独立提交，
待主干同步后重基/集成及执行正式PR双宿主验收。当前不归档、不声明远端CI通过。

## 6. 最新主干上的继续实施

PR49 已经最终归档 head CI 通过并合并为 `881253c3f2ddaf4c9833d7ff091fc6d9d1496609`。
本地 main 与 origin/main 已快进对齐0/0，原工作区6份未提交文件内容和状态逐项保全。
按用户要求，从该提交新建 `/private/tmp/koven-spec0273-main`（`fix/spec-0273-main`），
接入原独立修复 `26c4e1d`；只解决Spec索引与生成依赖图冲突，Rust生产和回归内容未冲突。
该新主干的教程原始 `gap-scope-branch` 已由旧release CLI复现exit2 InvalidSsa，
修复debug CLI build/run通过，输出 `inner/second/first/after/outer`（各一行）；
完整源码hash、binary hash与原始输出在 `/private/tmp/0273-tour-gap-replay/results.json`。
后续将同一源码从planned提升为实际CLI合同，保留0271归档的历史失败记录。

新主干 worktree 的教程接线已先执行红测（planned != executable），随后更新状态/完整输出合同
及提取器数量，教程+docs Python48项全部通过；完整fmt、尺寸与515页docs通过。
重新构建当前工作树CLI（Cargo locked/offline，13.47s），真实完整教程23项全部通过、
1 planned线程未执行，共61条真实子进程，完整stdout/stderr/exit与手写oracle比较；
证据在 `/private/tmp/0273-main-tutorial-evidence`，运行日志 `/private/tmp/0273-main-tutorial.log`。
25个源码fence与新main完全相同，旧0271失败源码hash保留；编排mock不计实际CLI验收。
接线独立审阅指出证据入口混用旧22/新23项，已修正并窄复核关闭。Rust生产与回归文件
逐字节等于已审阅测试的原修复提交，无新生产改动；原37资源/70SSA与Clippy证据按此内容复用。
当前准备正式PR；本轮双宿主执行尚未发生，Spec保持active。

## 7. 双宿主实现验收与归档

[PR50](https://github.com/Halckon/Koven/pull/50) 的实现 head
`7ef443f67bab0808b75da5596e7c9fab363bdb4b` 经
[PR CI 37213487681](https://github.com/Halckon/Koven/actions/runs/37213487681) completed success。
全部必需job成功；Tree-sitter按editors=false路径规则跳过。四个新增
`single_resource_deinit_control_body_` SSA测试及新增nested_control native测试在Ubuntu和macOS
逐项实际ok；两宿主完整教程23 executed/1 planned，原gap-scope-branch实际passed。
Codegen Linux830 passed/0 ignored，Mac829 passed/1 ignored，唯一忽略仍为既有LLDB
受debugserver task-port权限限制用例。线程planned没有执行，不计通过。

本次仅消费前端已有正常出口清理事实，已验证多个资源、外层存活、尾值交付和return不重复清理。
按上述实现证据完成有界修复并归档；最终归档head必须独立CI成功后才合并。
此前未运行的记录保留为历史，当前证据不扩张条件owner合流或资源wrapper支持，也不关闭M4b。
