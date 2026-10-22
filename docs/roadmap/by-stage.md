# Koven v1 Roadmap：按实施阶段

> **性质**：生成视图 · **状态**：current（生成自 [v1.json](v1.json)） · **读取时机**：查看 v1 已完成内容、当前位置与下一步时 · **唯一真源**：[v1.json](v1.json)；语义归 Guide，验收证据归各 Spec

<!-- 生成文件：请编辑 docs/roadmap/v1.json 后运行 python3 scripts/gen_roadmap.py，不要手改本页。 -->

基线：PR #120（D10 统一管线，基于 main c0c5156a，未合并）· Guide v0.46 · 更新：2026-10-22 · 其他视图：[按领域](by-area.md) · [按 Phase](by-phase.md) · 维护规则见 [README](README.md)

**v1 的定义**：能写真实命令行工具的语言：单进程、同步 IO；被接受的程序内存安全、资源确定性释放；合法程序要么正确运行，要么得到明确的“未实现”诊断；AArch64 macOS 与 x86_64 Linux glibc。

## 总览

| 阶段 | ✅ 已交付 | ◐ 部分 | ▶ 进行中 | ☐ 未开始 | ❓ 待决策 | ⏸ 延后 | 合计 |
|---|---:|---:|---:|---:|---:|---:|---:|
| S0 已交付基线 | 23 | 0 | 0 | 0 | 0 | 0 | 23 |
| P0 止血 | 2 | 0 | 1 | 4 | 0 | 0 | 7 |
| P1 结构 | 0 | 0 | 1 | 1 | 0 | 0 | 2 |
| P2 补齐现有 Guide 能力 | 0 | 4 | 1 | 5 | 0 | 0 | 10 |
| P3 语言增量 | 8 | 0 | 0 | 19 | 0 | 0 | 27 |
| P4 文本与 IO | 0 | 0 | 0 | 5 | 0 | 0 | 5 |
| P5 验收与发布 | 0 | 4 | 0 | 2 | 0 | 0 | 6 |
| X v1 之外 | 0 | 0 | 0 | 0 | 0 | 11 | 11 |

v1 范围内 80 项（必做 70，其中已交付 32）；X 阶段只列边界，不计入 v1。

## 最近一次实测

口径：2026-10-22 · PR #120 头 aef3f4f8 · 154 probe（2026-10-22 增量审计）

| 指标 | 结果 |
|---|---|
| 74 个合法基础程序：project / 单文件 / 两种模式都可 | 40 / 40 / 40 |
| 8 个应拒绝程序：两种模式都得到前端诊断 | 8 |
| 154 probe：project / 单文件可运行 | 92 / 92 |
| 154 probe：两种模式结果不一致 | 0（main c0c5156a 为 37） |
| 154 probe × 2 中的 InvalidSsa | 10（p14b、q05、y01、y03、y04 各两模式） |
| 原单文件成功、现在两模式失败 | 5（enum when 分支结果为 Boolean/String/Unit） |
| 六个验收程序可运行 | 0 / 6（源码尚未入库，见 V0） |

数字为未合并 PR #120 头的实测；合并后以 main 复测为准。下一次增量审计更新此处。

## 决策

| ID | 决策 | 推荐 | 状态 | 影响条目 |
|---|---|---|---|---|
| D1 | 相等/哈希/排序/显示的机制（[依据](../proposals/v0.46-language-decisions.md)） | 预声明 interface + 结构自动推导 | ✅ 2026-10-22：v1 定 Equatable/Hashable/Display；Comparable 只给内建类型；value class/enum 自动推导相等与哈希，Display 显式实现 | L1 |
| D2 | 字符串插值是否在 v1 落地（[依据](../proposals/v0.46-language-decisions.md)） | 是，基于 Display | ✅ 2026-10-22：v1 落地，`"${x}"` 要求 x : Display | L2 |
| D3 | when 异类分支 | 报错，与 if 一致 | ✅ Guide v0.44 A：封闭 if/when join | L9a |
| D4 | 顶层 val/var（[依据](../proposals/global-and-static-state.md)） | v1 只允许 const val | ✅ 2026-10-22：v1 只允许 const val；顶层运行时 val/var 改为 Phase 2 语言诊断（G1） | L10 |
| D5 | 线程是否在 v1.0（[依据](../proposals/v0.46-language-decisions.md)） | 受限版本进 v1 | ✅ 2026-10-22：受限版本进 v1（建议级）：spawn/join/channel，只转移所有权；drop 隐式 join；可落 v1.1 | T1 |
| D6 | Guide 13 的 unsafe/FFI 草图（[依据](../proposals/v0.46-language-decisions.md)） | 移到 proposals，v2 再议 | ✅ 2026-10-22：unsafe/extern/RawPtr 移出 Guide 正文到 proposals（v2） | L14 |
| D7 | 闭包可变捕获（[依据](../proposals/v0.46-language-decisions.md)） | v1.0 做非逃逸版本 | ✅ 2026-10-22：非逃逸闭包以独占借用捕获 var；move/逃逸闭包不允许 | S1 |
| D8 | Map 迭代顺序 | 插入顺序 | ✅ Guide v0.44 F：插入序 | L13 |
| D9 | 退出码（[依据](../proposals/v0.46-language-decisions.md)） | main 可返回 Result；abort 保持 SIGABRT；exitProcess 随 IO | ✅ 2026-10-22：正常退出 0；main 返回 Err 为 1；abort 保持 SIGABRT；kovenc run 透传子进程退出码；exitProcess(code): Nothing 随 L6 | L6, L21 |
| D10 | 管线合并与通用 lowering 是否优先于新功能（[依据](../specs/active/0341-shared-lowering-core.md)） | 是（P1 在 P2–P4 之前） | ✅ 2026-10-22：统一 compilation-unit 管线为终态（Guide v0.45、ADR-0031、SPEC-0341）；E3 先以 nullable 家族试点 | E2, E3 |
| D11 | Guide 拆分为语言参考与编译器合同 | 是 | ✅ Compiler Contracts 已建立，按边界渐进迁移 | B25 |
| D12 | Abort/error() 的 stderr 输出保证（[依据](../proposals/v0.44-language-completion.md)） | 保证打印消息与源码位置 | ✅ 2026-10-22：stderr 输出 abort 消息与源码位置，写失败忽略，随后 SIGABRT | L12 |
| D13 | companion 语法（[依据](../proposals/v0.46-language-decisions.md)） | 改为 `companion { }`（借鉴 Kotlin 2.5 companion block） | ✅ 2026-10-22：Guide v0.46 采纳，语义不变；不保留 `companion object` 别名 | L19 |
| D14 | 未使用的 Result 返回值（[依据](../proposals/v0.46-language-decisions.md)） | 丢弃即报诊断（借鉴 Kotlin 2.3 未使用返回值检查） | ✅ 2026-10-22：Guide v0.46 采纳 | L17 |
| D15 | value class 解构匹配方式（[依据](../proposals/v0.46-language-decisions.md)） | 按字段名匹配（借鉴 Kotlin 多字段 value class 提案） | ✅ 2026-10-22：Guide v0.46 采纳 | L18 |

## S0 已交付基线

P0 之前已合入 main 的编译器骨架与有界能力；不表示每项都覆盖全部组合。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ✅ 已交付 | B01 | Cargo workspace、索引式 AST、诊断与 fixture 骨架 | v1 必做 | 编译器结构 | — | 0 | [pipeline-and-workspace](../architecture/pipeline-and-workspace.md) | — |
| ✅ 已交付 | B02 | Lexer/Parser 与错误恢复：完整文件组合、换行分句与续行、上下文 TypeRef | v1 必做 | 词法与语法 | 已定义 | 1 | [0231](../archive/specs/0231-contextual-type-ref-trials.md) [0234](../archive/specs/0234-block-newline-continuation.md) [0242](../archive/specs/0242-automatic-borrow-call-migration.md) | 同行缺分隔诊断与独立 lambda 尾表达式另列。 |
| ✅ 已交付 | B03 | 名称与类型事实：单文件与 compilation-unit、package/import、名义/泛型/interface、单态化 | v1 必做 | 类型、泛型与声明 | 已定义 | 2 | [0197](../archive/specs/0197-multifile-type-checking.md) [0247](../archive/specs/0247-multifile-baseline.md) [names-and-types](../architecture/names-and-types.md) | 前端事实已接入；各类型的 native 覆盖见 P2。 |
| ✅ 已交付 | B04 | 所有权事实：move/loan/drop/capture、结构移动、容器 element place、跨文件所有权 | v1 必做 | 所有权、借用与析构 | 已定义 | 3 | [ownership](../architecture/ownership.md) | — |
| ✅ 已交付 | B05 | typed SSA → LLVM → 本机可执行文件；AArch64 macOS 与 x86_64 Linux glibc 双宿主 | v1 必做 | 编译器结构 | — | 4 | [0228](../archive/specs/0228-linux-x86-64-native-host.md) [0239](../archive/specs/0239-linux-ci-gates.md) [ssa-codegen-runtime](../architecture/ssa-codegen-runtime.md) | — |
| ✅ 已交付 | B06 | const val 封闭求值、数值字面量（radix/下划线）与整数具名位运算 | v1 必做 | 类型、泛型与声明 | 已定义 | 2,3,4 | [0226](../archive/specs/0226-unit-constant-materialization-ownership.md) [0227](../archive/specs/0227-unit-constant-native-lowering.md) [0229](../archive/specs/0229-extended-numeric-literal-values.md) [0240](../archive/specs/0240-integer-bitwise-execution.md) | — |
| ✅ 已交付 | B07 | String：字面量、拼接、内容比较、String.clone、println(String) | v1 必做 | 文本与 IO | 已定义 | 2,3,4 | [0189](../archive/specs/0189-standard-println-output.md) [0236](../archive/specs/0236-explicit-string-clone.md) [string-clone](../architecture/string-clone.md) | — |
| ✅ 已交付 | B08 | replace/swap：owned root 与一级字段 | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | [0232](../archive/specs/0232-ownership-primitive-type-facts.md) [0244](../archive/specs/0244-root-ownership-primitives.md) [0246](../archive/specs/0246-direct-field-replace.md) | nested/index/field swap 与 Inout ABI 未扩大。 |
| ✅ 已交付 | B09 | deinit 双轨析构：body→字段逆序、控制退出与唯一回收 | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | [0245](../archive/specs/0245-resource-deinit.md) [0273](../archive/specs/0273-control-body-resource-cleanup.md) [resource-deinit](../architecture/resource-deinit.md) | — |
| ✅ 已交付 | B10 | receiver 两阶段借用 | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | [0243](../archive/specs/0243-receiver-two-phase-borrows.md) [receiver-borrows](../architecture/receiver-borrows.md) | — |
| ✅ 已交付 | B11 | 非泛型递归 Box enum 的构造、运输与析构 | v1 必做 | 标准库与运行时 | 已定义 | 2,3,4 | [0230](../archive/specs/0230-recursive-boxed-enum-native.md) | Box.value/unbox 见 N07。 |
| ✅ 已交付 | B12 | M1A 多文件程序贯通：跨文件字段可变性、直接字段 Borrow、unit for | v1 必做 | 编译器结构 | 已定义 | 2,3,4,6 | [0263](../archive/specs/0263-unit-field-mutability.md) [0264](../archive/specs/0264-unit-direct-field-borrow.md) [0265](../archive/specs/0265-unit-iteration-facts.md) [0268](../archive/specs/0268-unit-iteration-native.md) | — |
| ✅ 已交付 | B13 | M2B 普通借用返回：borrow T from param、多参数、混合 Value、同步 Borrow 实参、value class 字段/只读方法 | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | [0305](../archive/specs/0305-multi-parameter-borrow-result.md) [0306](../archive/specs/0306-mixed-value-borrow-result.md) [0307](../archive/specs/0307-synchronous-borrow-result-arguments.md) [0309](../archive/specs/0309-borrow-value-class-fields.md) [0316](../archive/specs/0316-single-readonly-value-methods.md) | M2B 已冻结；新组合需另立 Spec。泛型类 borrow from this 见 N09。 |
| ✅ 已交付 | B14 | Array/List/MutableList：构造、索引、Borrow 遍历、add/clear/removeAt/insert 等 | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0182](../archive/specs/0182-sequential-for-lowering.md) [0279](../archive/specs/0279-runtime-length-container-native.md) [0280](../archive/specs/0280-mutable-list-add.md) [0281](../archive/specs/0281-mutable-list-clear.md) [0282](../archive/specs/0282-mutable-list-remove-at.md) [0283](../archive/specs/0283-mutable-list-remove-last.md) [0284](../archive/specs/0284-mutable-list-remove-first.md) [0285](../archive/specs/0285-mutable-list-insert-at.md) [0321](../archive/specs/0321-mutable-list-element-delivery.md) | — |
| ✅ 已交付 | B15 | Map/MutableMap 基础：put/get/remove、scoped 借用访问、条件 break/continue/return | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0287](../archive/specs/0287-m2b-and-map-type-system.md) [0288](../archive/specs/0288-map-native-execution.md) [0294](../archive/specs/0294-map-put-control-exit.md) [0297](../archive/specs/0297-map-key-return-cleanup.md) [0299](../archive/specs/0299-owned-map-result-borrow.md) [0300](../archive/specs/0300-single-map-loop-owner-carry.md) [0310](../archive/specs/0310-single-map-conditional-break.md) [0311](../archive/specs/0311-single-map-conditional-continue.md) [0313](../archive/specs/0313-single-map-conditional-return.md) [0318](../archive/specs/0318-map-return-operand-last-use.md) [map-native-execution](../architecture/map-native-execution.md) | — |
| ✅ 已交付 | B16 | N1a 连续范围 carrier：take/drop/dropLast 与范围绑定局部控制流 | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0289](../archive/specs/0289-n1a-range-carrier.md) [0298](../archive/specs/0298-range-binding-local-control-flow.md) [0315](../archive/specs/0315-range-int-comparisons.md) [0317](../archive/specs/0317-range-normal-short-circuit.md) | 已冻结；N1b/filter 视图延后到 X。 |
| ✅ 已交付 | B17 | Rc<T> 核心路径与 Koven prelude（error、Pair、Result 源码） | v1 必做 | 标准库与运行时 | 已定义 | 2,3,4 | — | CLI 用户环境尚未注入 Result/Pair，见 E4；Rc 字段投影见 N07。 |
| ✅ 已交付 | B18 | kovenc build/run、本地 project、JSON Lines 诊断、LSP、formatter、TextMate 与 Tree-sitter | v1 必做 | 工具链 | 已定义 | 6 | [0187](../archive/specs/0187-multifile-lsp-diagnostics-definition.md) [0251](../archive/specs/0251-lsp-unit-name-snapshot.md) [0267](../archive/specs/0267-editor-corpus-gate.md) [tooling](../architecture/tooling.md) | — |
| ✅ 已交付 | B19 | 可执行 tour 与 Guide Litmus 门禁 | v1 必做 | 验收、发布与文档 | — | 6 | [0238](../archive/specs/0238-guide-litmus-gate.md) [0262](../archive/specs/0262-current-tutorial-plan-coverage.md) [0271](../archive/specs/0271-tour-combination-coverage.md) [README](../tutorials/README.md) | — |
| ✅ 已交付 | B20 | Linux ASan/LSan 有界接线与资源程序随机生成首片（M4b） | v1 必做 | 安全验证 | — | 4 | [0266](../archive/specs/0266-native-sanitizer-wiring.md) [0269](../archive/specs/0269-generated-resource-programs.md) | — |
| ✅ 已交付 | B21 | 双宿主 preview 包与独立安装验收 | v1 必做 | 验收、发布与文档 | — | 6 | [0272](../archive/specs/0272-preview-candidate-package.md) [0322](../archive/specs/0322-preview-libxml2-host-compatibility.md) | — |
| ✅ 已交付 | B22 | argv 词频程序（Map 版）与 main(args) | v1 必做 | 文本与 IO | 已定义 | 2,3,4,6 | [0274](../archive/specs/0274-argv-word-frequency.md) [0295](../archive/specs/0295-map-argv-word-frequency.md) | — |
| ✅ 已交付 | B25 | Guide 拆分首片：Compiler Contracts 独立真源（D11） | v1 建议 | 验收、发布与文档 | — | — | [0233](../archive/specs/0233-parser-compiler-contracts.md) | 混合语义/诊断/Span 段继续渐进迁移。 |

## P0 止血

与 P1 并行、不依赖 E3 的小件：abort 可观测、Result 进入内建环境、顶层运行时值改为语言诊断、回归语料入库，以及 Guide v0.46 决策批。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ✅ 已交付 | E1 | 前端硬门禁：Deferred/Error 不进入所有权与 codegen；未知成员报诊断；内部错误结构化；L0165 用户措辞 | v1 必做 | 编译器结构 | — | 2 | [0319](../archive/specs/0319-final-type-admission-diagnostics.md) [0323](../archive/specs/0323-final-admission-static-recovery.md) [0324](../archive/specs/0324-final-admission-user-messages.md) | 内部错误（ICE/ERR）为 0；但 lowering 生成非法 SSA（InvalidSsa）的路径仍有 10 次观察，并入 E2 门槛。 |
| ▶ 进行中 | E1b | 声明/函数终止性：val 初始化全分支 return 与 L0088 入口一致 | v1 必做 | 控制流 | 已定义 | 2 | [0326](../archive/specs/0326-closed-control-branch-join.md) | 0326 作者已认领，尚未分 Spec。 |
| ☐ 未开始 | E4 | Result/Ok/Err 注入内建环境（单文件与 project 相同）；kovenc check 与 --help | v1 必做 | 标准库与运行时 | 已定义 | 5,6 | — | 已决（2026-10-22）：Result/Ok/Err 进入内建环境，需改写 v0.45 Guide 02 的“不额外提供”条文；Pair 随 S6（to / mapOf）进入。验收：fun f(): Result<Int, String> 直接可用。 |
| ☐ 未开始 | L10 | 顶层声明收窄：只允许 const val；顶层运行时 val/var 结构化拒绝（D4） | v1 必做 | 类型、泛型与声明 | 需新增 | 2 | [global-and-static-state](../proposals/global-and-static-state.md) | D4 已决：v1 只允许 const val（候选 G1）。现状为 L0165 加后端 UnsupportedSource，需改为 Phase 2 语言诊断，help 给出改 const val 或在 main 中构造两条迁移路径。 |
| ☐ 未开始 | L12 | error()/!!/越界/溢出 abort 时向 stderr 打印消息与源码位置（D12） | v1 必做 | 空安全与错误值 | 需新增 | 4,5 | — | D12 已决：stderr 输出 `abort: <message>` 与 `at <file>:<line>:<col>`；!!、越界、溢出、转换失败使用固定消息；写失败忽略；随后 SIGABRT。规则进 Guide v0.46。 |
| ☐ 未开始 | V0 | 154 个 probe 与六个验收程序源码入库，作为 conformance 种子与切换门槛的固定集合 | v1 必做 | 验收、发布与文档 | — | 6 | — | 目前只在本地 .local-artifacts；SPEC-0341 因缺源码只能用 22 个附件与 39 场景验证。 |
| ✅ 已交付 | G46 | Guide v0.46 决策批：E4、D1、D2、D4、D7、D9、D12–D15、L14 与错误模型 | v1 必做 | 验收、发布与文档 | 需新增 | — | [v0.46-language-decisions](../proposals/v0.46-language-decisions.md) | 已启用（迁移账本 archive/migrations/v0.46-enablement.md）；各规则的实现按条目推进。 |

## P1 结构

D10 统一管线收尾（两模式差异 0、InvalidSsa 0、原成功能力无损失、删除旧 checker 与 lower_frontend）；随后以 nullable 家族试点 E3 通用 lowering。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ▶ 进行中 | E2 | 管线合并：单文件走 compilation-unit 管线，删除重复的 checker/lower_frontend（D10） | v1 必做 | 编译器结构 | — | 2,3,4 | [0341](../specs/active/0341-shared-lowering-core.md) | D10 已决：统一 compilation-unit 管线为终态（Guide v0.45、ADR-0031、SPEC-0341，均在未合并的 PR #120）。单文件 CLI 与 LSP 已切换，154 probe 两模式差异 0；剩余：enum when 回退 5 个、InvalidSsa 10 次、删除旧 checker 与 lower_frontend。 |
| ☐ 未开始 | E3 | 通用所有权细化层：typed + ownership facts → 显式 move/drop/loan 指令 + CFG → SSA（D10） | v1 必做 | 编译器结构 | — | 3,4 | — | 先以 nullable 家族（p14、p14b、p14c、p15、q05、v03、v13、x02）试点：全部通过且不新增按组合拆分的 lowering 模块；成功后推广到 P2 各家族。 |

## P2 补齐现有 Guide 能力

E3 推广到 when、闭包、inout/字段赋值、泛型、Result `?`、Double/Char 等家族；门槛：74 个合法基础程序 ≥ 70。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ☐ 未开始 | N01 | 赋值与可变性：字段赋值 obj.f = v、inout 标量参数赋值、inout class 参数、泛型 class 的 inout 方法 | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | [0325](../archive/specs/0325-simple-variable-assignment-types.md) | 0325 只覆盖简单变量赋值的类型。 |
| ◐ 部分 | N02 | when：Int/String/Char/MoveOnly enum subject、enum 方法内 when (this)、project 模式 enum case 条件 | v1 必做 | 控制流 | 已定义 | 2,3,4 | [0241](../archive/specs/0241-return-control-operands.md) [0320](../archive/specs/0320-enum-case-borrow-categories.md) [0326](../archive/specs/0326-closed-control-branch-join.md) | enum subject 的 when 在分支结果为 Boolean/String/Unit 时 MissingFact（PR #120 中原单文件能力回退，属 E2 门槛）；Int/String subject、in range、语句位置混型仍失败。 |
| ◐ 部分 | N03 | nullable 全路径：标量 T?、nullable 参数、Elvis、?.、!! | v1 必做 | 空安全与错误值 | 已定义 | 2,3,4 | [0220](../archive/specs/0220-compilation-unit-pointer-nullable-storage-lowering.md) [0296](../archive/specs/0296-owned-null-comparison-cleanup.md) | nullable 家族 8 个 probe 全部失败，作为 E3 试点。 |
| ◐ 部分 | N04 | 闭包：lambda 直接作实参、函数引用 ::f、move 闭包返回与存储 | v1 必做 | 函数类型与闭包 | 已定义 | 2,3,4 | [0278](../archive/specs/0278-borrowed-closure-escape-verification.md) [borrowed-closure-escape](../architecture/borrowed-closure-escape.md) | 闭包家族 5 个 probe 失败；move 闭包（p18）是受限线程 T1 的前置。 |
| ☐ 未开始 | N05 | Double/Float 运算与 Char 比较的 native 支持 | v1 必做 | 类型、泛型与声明 | 已定义 | 4 | — | — |
| ◐ 部分 | N06 | interface bound 泛型调用、companion 函数调用、interface 实现与默认方法（单文件） | v1 必做 | 类、接口与成员 | 已定义 | 2,3,4 | [0323](../archive/specs/0323-final-admission-static-recovery.md) | companion 调用已由 0323 转为正确的静态角色；native 仍未通。 |
| ☐ 未开始 | N07 | Box.value / unbox() 与 Rc.value 字段投影 | v1 必做 | 标准库与运行时 | 已定义 | 2,3,4 | — | — |
| ☐ 未开始 | N08 | Result + 后缀 ? 的 Int 与 String payload 全路径 | v1 必做 | 空安全与错误值 | 已定义 | 2,3,4 | [09-nullability-errors](../guide/09-nullability-errors.md) | 文件 IO（L6）的硬前置。 |
| ☐ 未开始 | N09 | borrow T from this（泛型类 receiver 来源） | v1 必做 | 所有权、借用与析构 | 已定义 | 2,3,4 | — | 当前报 L0164。 |
| ▶ 进行中 | N10 | 单文件独有缺口：普通 class 方法、泛型 value class 方法、class inout 方法、if 作值 | v1 必做 | 类、接口与成员 | 已定义 | 4 | [0316](../archive/specs/0316-single-readonly-value-methods.md) | 单文件独有缺口随 E2 入口切换消失（PR #120 实测两模式一致）；泛型 value class 方法（t03）两模式仍失败，归 N06。PR 合并后改为 done。 |

## P3 语言增量

实现 Guide v0.46 的语言增量：协议与插值、闭包可变捕获与 forEach、MoveOnly enum 消费式匹配、must-use、按名解构、companion 语法、不 abort 的 API 变体。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ☐ 未开始 | L1 | 值语义协议 Equatable/Hashable/Display（Comparable 只给内建数值、Char、String）；value class/enum 自动推导相等与哈希；Map 键 K : Equatable + Hashable（D1） | v1 必做 | 类型、泛型与声明 | 需新增 | 2,4,5 | — | — |
| ☐ 未开始 | L2 | 字符串插值降级："${x}" 要求 x : Display（D2） | v1 必做 | 文本与 IO | 需新增 | 2,4 | — | 依赖 L1。 |
| ✅ 已交付 | L3 | 八种整数之间的 checked 转换（越界 Abort） | v1 必做 | 类型、泛型与声明 | 已定义 | 2,3,4 | [0327](../archive/specs/0327-checked-integer-conversions.md) | — |
| ☐ 未开始 | L3b | 转换族其余部分：OrNull 变体、浮点↔整数 checked、Char 码点 | v1 必做 | 类型、泛型与声明 | 需新增 | 2,4 | — | 属于“可能 abort 的 API 必须有不 abort 版本”规则（L20）的一部分。 |
| ✅ 已交付 | L4 | IntRange/LongRange 可存储 Copyable 值、in / not in、升序基本 for | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0328](../archive/specs/0328-integer-range-values.md) [0329](../archive/specs/0329-integer-range-iteration.md) | 一般 Range? 参数/局部 null 等边界仍由作者跟踪。 |
| ☐ 未开始 | L7 | MoveOnly enum 的消费式匹配：owned subject 分支获得 payload owned 绑定 | v1 必做 | 控制流 | 需新增 | 2,3,4 | — | — |
| ☐ 未开始 | L8 | Box 访问面措辞修正：box.value 为只读 projection place、unbox(): T | v1 必做 | 标准库与运行时 | 需新增 | 2 | — | N07 的规范前置。 |
| ✅ 已交付 | L9a | 封闭 if/when 结果 join：不再合并到 Any（D3） | v1 必做 | 控制流 | 已定义 | 2 | [0326](../archive/specs/0326-closed-control-branch-join.md) | — |
| ✅ 已交付 | L9b | 简单变量普通赋值的类型、Unit 结果与收窄事实失效 | v1 必做 | 类型、泛型与声明 | 已定义 | 2 | [0325](../archive/specs/0325-simple-variable-assignment-types.md) | — |
| ☐ 未开始 | L9c | is E.Case 从 subject 推断类型实参；L0086→L0084 诊断码统一 | v1 建议 | 类型、泛型与声明 | 需新增 | 2 | — | v0.44 列为额外合同，未纳入。 |
| ☐ 未开始 | L11 | 泛型体内未约束 T 的析构轨道（按资源轨道词法作用域析构） | v1 必做 | 所有权、借用与析构 | 需新增 | 2,3 | — | — |
| ✅ 已交付 | L13 | Map 插入序借用遍历（D8） | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0331](../archive/specs/0331-map-borrowed-iteration.md) | Map 双分量借用遍历已交付；单文件“局部声明夹在 put 与 for 之间”失败随 E2 消失。 |
| ✅ 已交付 | L13b | MutableMap.consume 整体移交为只读 Map | v1 必做 | 集合与迭代 | 已定义 | 2,3,4 | [0291](../specs/active/0291-readonly-map-construction.md) | consume 已交付；0291 其余待项仍 active。 |
| ✅ 已交付 | L15 | while 保守流事实：条件为真的 body 事实与写入/回边/break 失效 | v1 必做 | 控制流 | 已定义 | 2 | [0330](../archive/specs/0330-while-conservative-flow.md) | while 保守流事实已交付；`while (x != null)`（Int?，v13）仍失败，归 N03。 |
| ☐ 未开始 | L16 | 同步、非逃逸 Borrow 回调的 forEach 与 List/Map 高层基础 API | v1 必做 | 集合与迭代 | 需新增 | 2,3,4,5 | [collection-api-followups](../proposals/collection-api-followups.md) | 依赖 S1 与 N04。 |
| ☐ 未开始 | L17 | 丢弃 Result 返回值报诊断（must-use）（D14） | v1 必做 | 空安全与错误值 | 需新增 | 2 | — | — |
| ☐ 未开始 | L18 | value class 局部解构改为按字段名匹配（D15） | v1 必做 | 类型、泛型与声明 | 需新增 | 1,2 | — | — |
| ✅ 已交付 | L19 | companion 语法改为 `companion { }`，语义不变（不产生对象）（D13） | v1 必做 | 类、接口与成员 | 需新增 | 1,2 | [0342](../archive/specs/0342-companion-block-syntax.md) | — |
| ☐ 未开始 | L20 | 可能 abort 的标准 API 必须提供不 abort 版本：getOrNull、checked 运算（返回 T?）等 | v1 必做 | 空安全与错误值 | 需新增 | 2,4,5 | [v0.46-language-decisions](../proposals/v0.46-language-decisions.md) | 错误模型决策（2026-10-22）：不引入异常与 try/catch。 |
| ☐ 未开始 | L14 | 把 v1 不做的内容移出 Guide：unsafe/extern/RawPtr、线程/channel 预声明 API（D6） | v1 必做 | 验收、发布与文档 | 需新增 | — | — | D6 已决：unsafe/extern/RawPtr 移到 proposals；线程 API 按 D5 保留受限版（T1）。 |
| ☐ 未开始 | S1 | 非逃逸闭包以 exclusive loan 可变捕获 var（D7） | v1 必做 | 函数类型与闭包 | 需新增 | 2,3,4 | — | D7 已决：非逃逸（借用）闭包以独占借用捕获 var；move/逃逸闭包不允许。L16 的前置。 |
| ☐ 未开始 | S2 | 泛型推导读取 lambda 返回值（两段式） | v1 建议 | 类型、泛型与声明 | 需新增 | 2 | — | map/fold 的前置。 |
| ☐ 未开始 | S3 | owned 集合算法：map/filter/fold/sort/contains/indexOf/reverse | v1 建议 | 集合与迭代 | 需新增 | 2,3,4,5 | [collection-algorithm-ownership-r3](../proposals/collection-algorithm-ownership-r3.md) | — |
| ☐ 未开始 | S4 | 无后缀整数字面量在二元运算中随对侧整数类型定型 | v1 建议 | 类型、泛型与声明 | 需新增 | 2 | — | — |
| ☐ 未开始 | S5 | while/for 循环标签 | v1 建议 | 控制流 | 需新增 | 1,2,4 | — | — |
| ☐ 未开始 | S6 | Map 带初值构造 mapOf("a" to 1) | v1 建议 | 集合与迭代 | 需新增 | 2,3,4 | — | 依赖 E4 的 Pair 可见性。 |
| ☐ 未开始 | S7 | 只读 borrow var 游标（链表/树的迭代遍历） | v1 建议 | 所有权、借用与析构 | 需新增 | 2,3 | — | — |

## P4 文本与 IO

文本 API、同步 IO、main 返回 Result 与进程接口；之后是受限线程（建议级，可落 v1.1）。门槛：六个验收程序 ≥ 3 个可运行。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ☐ 未开始 | L5 | 文本 API：size、Char 迭代、byteAt、substring、indexOf、split、trim、lines、startsWith、toInt(): Int?、StringBuilder | v1 必做 | 文本与 IO | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) | — |
| ☐ 未开始 | L6 | IO 与进程：readLine(): String?、File.readText/writeText: Result、eprintln、exitProcess(code): Nothing（D9） | v1 必做 | 文本与 IO | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) | 依赖 L5、N08、E4。D9 已决：exitProcess(code): Nothing 在本项定义。 |
| ☐ 未开始 | L21 | main 可返回 Result<Unit, E>（E : Display）：Err 时 stderr 输出 `error: …`，退出码 1（D9） | v1 必做 | 文本与 IO | 需新增 | 2,4,5 | — | 依赖 E4 与 L1。 |
| ☐ 未开始 | T1 | 受限线程：thread(move { }) : Thread<T>、join()、channel（多生产者单消费者）；drop 时隐式 join；worker abort 即进程 abort；只转移所有权（D5） | v1 建议 | 标准库与运行时 | 需新增 | 2,3,4,5 | [thread-transfer](../proposals/thread-transfer.md) | 依赖 N04（move 闭包）与 L6 runtime；用第七个验收程序验收，可落 v1.1，不阻塞 v1.0。 |
| ☐ 未开始 | L6b | time/sleep/math 基础 API | v1 必做 | 标准库与运行时 | 需新增 | 2,4,5 | [text-input-and-io](../proposals/text-input-and-io.md) | — |

## P5 验收与发布

六个验收程序、conformance 矩阵、sanitizer 门禁与文档按实测重写，发布 v1.0。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ☐ 未开始 | V1 | 六个验收程序（词频、配置解析、表达式求值、RAII 状态机、二进制编解码、链表/栈）双宿主运行正确且 ASan/LSan 干净 | v1 必做 | 验收、发布与文档 | — | 6 | — | argv 词频（B22）是 A 的前身；另设第七个线程程序验收 T1，不计入 v1.0 硬门槛。 |
| ◐ 部分 | V2 | Conformance 矩阵：按 Guide 每章正反例，合法 100% 运行、非法 100% 前端诊断、0 ICE | v1 必做 | 验收、发布与文档 | — | 2,3,4 | — | 154 probe 目前只在本地 .local-artifacts；入库见 V0，并接 CI。 |
| ◐ 部分 | V3 | Linux ASan/LSan 覆盖整个矩阵与六个验收程序 | v1 必做 | 安全验证 | — | 4 | [0266](../archive/specs/0266-native-sanitizer-wiring.md) | — |
| ☐ 未开始 | V4 | M4b 随机生成扩展到 E3 之后的通用 lowering | v1 建议 | 安全验证 | — | 4 | [0269](../archive/specs/0269-generated-resource-programs.md) [native-safety-validation](../proposals/native-safety-validation.md) | — |
| ◐ 部分 | V5 | README、教程、Architecture 按实测能力重写；Guide 不含“当前实现到哪一步”的句子 | v1 必做 | 验收、发布与文档 | — | — | — | README 与 tour 已于 10-15 按实测改写。 |
| ◐ 部分 | V6 | v1.0 发行：双宿主安装包、使用材料与已知限制 | v1 必做 | 验收、发布与文档 | — | 6 | [0272](../archive/specs/0272-preview-candidate-package.md) [measurement-distribution](../proposals/measurement-distribution.md) | — |

## X v1 之外

明确延后到 v1.x / v2 / v3 的内容，列出以划清边界。

| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |
|---|---|---|---|---|---|---|---|---|
| ⏸ 延后 | X02 | macOS 动态检测、UBSan、外部审计（M4 其余） | v1.x | 安全验证 | — | 4 | — | — |
| ⏸ 延后 | X03 | 性能基线与正式分发（M5 其余） | v1.x | 验收、发布与文档 | — | — | [measurement-distribution](../proposals/measurement-distribution.md) | — |
| ⏸ 延后 | X04 | N1b、filter 视图、consume 链；step/downTo 与反向迭代 | v1.x | 集合与迭代 | 需新增 | 2,3,4 | [0292](../specs/drafts/v0.43/0292-immediate-consuming-sequence.md) | — |
| ⏸ 延后 | X05 | Weak<T> 与 Rc 环处理；Copyable 显式 opt-in/opt-out | v1.x | 标准库与运行时 | 需新增 | 2,5 | — | — |
| ⏸ 延后 | X06 | 静态不可变存储（顶层 static val，G2） | v1.x | 类型、泛型与声明 | 需新增 | 2,3,4 | [global-and-static-state](../proposals/global-and-static-state.md) | — |
| ⏸ 延后 | X07 | dyn/interface 值、匿名对象 | v2 | 类、接口与成员 | 需新增 | 2,3,4 | [v2-interface-values-and-dynamic-dispatch](../proposals/v2-interface-values-and-dynamic-dispatch.md) | — |
| ⏸ 延后 | X08 | 统一非逃逸类型：可存储/条件/多来源借用、inout 返回、用户迭代协议、Str/StringView 与切片 | v2 | 所有权、借用与析构 | 需新增 | 2,3,4 | — | — |
| ⏸ 延后 | X09 | Shareable、Arc/Mutex/原子、scoped threads；Lazy 与共享可变全局（G3/G4） | v2 | 标准库与运行时 | 需新增 | 2,3,4,5 | [global-and-static-state](../proposals/global-and-static-state.md) | — |
| ⏸ 延后 | X10 | unsafe、RawPtr、extern "C"、标准库自写（D6） | v2 | 标准库与运行时 | 需新增 | 1,2,4,5 | — | — |
| ⏸ 延后 | X11 | 泛型多 bound、where、associated type、扩展函数；const 泛型；默认参数/vararg/data class | v2 | 类型、泛型与声明 | 需新增 | 1,2 | [restricted-extension-functions](../proposals/restricted-extension-functions.md) | — |
| ⏸ 延后 | X12 | 协程/async 与自举 | v3+ | 编译器结构 | 需新增 | — | — | — |
