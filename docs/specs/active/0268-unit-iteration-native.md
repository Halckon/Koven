# SPEC-0268: unit 顺序迭代 native 与 M1A 程序贯通

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施 unit for 后端及 M1A 总验收时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-268` |
| 所属 Phase | Phase 4 SSA/native；Phase 6 CLI project 验收 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[顺序容器 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider)、[程序入口](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户要求持续实施里程碑；前置 PR43 已合并后从 main 新建 feature/spec-0268 |
| 前置 Spec | SPEC-0182、SPEC-0263、SPEC-0264、SPEC-0265 |
| 前置 ADR | ADR-0006、ADR-0018、ADR-0019、ADR-0022 |
| 基线 | main `6377f2b4dee9e186610092bd0c51a113ab098c8c` |
| 影响范围 | unit SSA/native、直接回归、真实 CLI 多文件教程及文档 |
| 语言语义变更 | 否 |

## 1. Goal 与范围

承接 [M1A 起草材料](../../development/multifile-program-spec-draft.md) A5–A10：消费已完成的
unit typed/ownership 计划，使三文件参数报告程序通过真实 project build、artifact 和 run，
保留跨文件 class、字段直接 Borrow 与 unit for。A2–A4 验收仅按各已归档合同追溯。

必须支持现行 Array/List/MutableList 的 owned named、Borrow 参数、temporary source，
Name/Discard/concrete value-class borrowed 解构，正常/continue/break/return/Abort及嵌套控制流。
Inout/field source 保持 §37.4 的 native 延后边界；前端先成功，后端明确拒绝且无产物污染。
所有身份、清理顺序及能力选择均消费已验证事实，不从 AST 重推所有权。

## 2. 当前证据与实施调整

前置0265已发布source-qualified descriptor、完整iteration cleanup动作和同轮schema，
包括conditional receiver在provider内外与嵌套形成位置的独立顺序义务；unit lowering仍将for拒绝。
真实M1A源码经已重编译frontend进入CLI，build返回native UnsupportedSource/UnsupportedNode，
没有产物。该红测只证明当前首个后端缺口，不替代后续程序或资源验收。

复用 `ssa/provider.rs` 的snapshot/header/guard/advance算法及unit现有CFG owner/loan运输，
不复制single driver。现有pending call使用CFG稳定槽位，结束时机需要与公开清理动作对应；
旧平面drop或conditional事实不能在完整动作序列后再次执行。
source access表示权限而非形状；Field symbol即使是无projection root也不能误判为named source。
源求值自身return/Abort发生于AcquireProvider之前，不要求一个不存在的provider退出计划。

## 3. 实施合同

- source只求值一次，长度快照一次；provider状态无分配，cursor和元素place按既有SSA primitives运输。
- binding只Borrow元素；Copyable读取形成copy，MoveOnly和value-class分量不隐式移动或析构。
- 按控制点一次消费完整有序动作；call/receiver/capture loan、ordinary/conditional drop均保持事实身份。
- normal/continue清理当前元素后推进；break/return清理对应provider，exhaustion仅在false edge清理，
  不在共同exit重复清理。temporary到EndSource后释放；外部Borrow能力属于caller；Abort不unwind。
- 保持ordinary/constant能力与输入同轮身份；缺必需descriptor/状态必须明确失败，不能静默忽略动作。
- 保留source、Span与结构化失败；不能用放松verifier、忽略native负例或改写验收源码取得成功。
- 三文件源码只有一个长期真源：已从起草材料迁入现有教程提取机制；四组argv已在开发快照实际通过，最终实现与双宿主验收仍待执行。
  四组argv共享同一源码，复用既有project发现/入口/link/run；不增加生产CLI配置。

## 4. 非目标

不增加语言语义、开放provider、Map/Iterator、借用返回、普通字段赋值、Inout/field native source。
不以全面合并single/unit或清理旧尺寸欠账作为前置，不扩大现有closure动态实例支持边界。
不实施M1B新增API、发布版本或宣称M4 sanitizer/全部语言支持已完成。

## 5. 单一验收账本

下表逐项记录当前证据；开发快照与旧测试通过不等于本片最终交付。

| ID | 合同与独立观察 | 接入与实际结果 |
|---|---|---|
| N1 | 有效红测：合法unit for在SSA/native被拒绝；保留现有身份/错误原子性负例 | 合法temporary Array跨文件for有效红→native绿；最终unit native128项包含旧原子性/身份与真实for成功恢复 |
| N2 | 三容器×owned named/Borrow参数/temporary，0/1/多元素、源一次、长度快照、Name/Discard/跨文件value-class分量 | 最终基底unit native128项通过，含54组provider/source/长度组合、借用解构及独立SSA单次length快照检查 |
| N3 | normal/continue/break/return/Abort、nested for/while、源求值提前分歧、pending调用与conditional receiver清理位置 | 最终基底128项包含72组退出、conditional三位置24组、嵌套for/while、源动态return/Abort和pending调用 |
| N4 | owner/指针唯一释放与顺序，source/element/field owner分开核，旧字段previous不遗漏；Abort无unwind | 最终基底128项包含资源/field replace/借用复用及原Report两入口×四argv逐pointer/drop顺序oracle；Abort单独核无unwind |
| N5 | ordinary/constant、同轮/混轮/foreign source、Inout/field精确拒绝、失败保留旧artifact及目录状态 | 最终基底field/Inout/captured Borrow、ordinary身份/来源及constant混轮负例通过，均拒绝LLVM/产物污染，旧原子性子进程含reservation及成功恢复 |
| P1 | 原三文件程序四组argv各执行build、artifact、run，完整UTF-8 stdout/空stderr/exit0 | 最终本机CLI完整教程17个执行案例通过，含四argv逐组build/artifact/run；planned-thread未执行，两宿主PR待验 |
| R1 | single sequential for及直接受影响unit borrow/closure/control-flow/constant消费者不回归 | unit lowering138、unit native128、single sequential/provider67、sanitizer2、boxed4通过；精确选择与未运行项见§8 |
| E1 | fmt、受影响all-targets严格Clippy、workspace all-targets、docs/尺寸、独立review与双宿主PR最终门禁 | 本机fmt/Clippy/workspace、Python132/docs510/尺寸及独立审阅通过；精确head双宿主PR CI待执行 |

P1源码只见[教程 parameter-report](../../tutorials/koven-tour.md#parameter-report)，
完整输出合同只由教程metadata执行；不可绕过的原程序路径可从起草记录第2–4节追溯。
两宿主实际执行是总退出条件；本机通过、结构门禁或IR断言均不替代。

## 6. 顺序与交付

1. 固定红测与源码；按已批准现行合同实现unit lowering/cleanup与失败边界，测试先红后绿。
2. 后端实现与CLI教程提取可按文件职责并行；只允许一个Cargo进程使用共享target。
3. 接通真实三文件build/artifact/run及资源验证，按影响扩展回归，更新Architecture与本账本。
4. fresh-context审阅最大剩余问题，修复后复审；按授权分逻辑提交、PR、精确head CI、归档、最终CI和合并。

本机LLVM21.1.8、Rust1.96.0，Cargo使用 `--locked --offline` 与既有共享target。
切换工作树后touch相关crate入口以确保实际源码重编译；不新建target、不cargo clean。
发现规范冲突时停止对应语义实施；发现前置实现遗漏时补最小事实合同和回归，不要求后端猜测。

## 7. 实施记录

已从基线草稿迁移三份Koven源码到教程，逐字节比较一致；原路径/entry保持，manifest沿用
教程既有project形态。草稿只保留链接，JSON只保存四组argv与完整输出，不复制源码。
教程驱动旧实现对新cases的有效红测为 `KeyError: args`；加入每case独立build/artifact/run后，
`python3 -m unittest scripts.tests.test_tutorial_contracts` 9项通过，覆盖四组命令/输出、
源码唯一性、非法空/重复cases和旧single/project/diagnostic路径。测试mock执行只证明编排，
不算P1真实CLI验收；源码、运行输出及native资源仍须后端完成后实跑。
`python3 -m unittest discover -s scripts/tests` 119项通过（此基线尚未包含0266新增检测脚本）；
`python3 scripts/check_docs.py` 当前509页通过，diff检查通过；独立教程/合同review发现旧起草材料仍有冲突真源声明，已改为历史要求与正式承接映射；独立复核确认P2已关闭。

开发中首个native切片让真实parameter-report四组argv全部通过：CLI构建1.33s成功，随后
`python3 scripts/check_tutorial.py --cli <共享target>/debug/kovenc --example parameter-report`
实际执行四组build/artifact/run，均匹配完整输出/退出码；非mock。
该结果来自未提交开发快照，CLI二进制SHA256及命令已记录在本机evidence；
N2–N5/R1、最终实现的复跑与双宿主CI尚未完成，不能据此宣称M1A已全部交付。


后续开发快照已取得N2/N3首轮5项native通过（0 failed/797 filtered/0 ignored），含三provider×
三source形态×0/1/多元素的54组ordinary/constant组合及72组退出组合；资源5项通过
（0 failed/802 filtered/0 ignored）。container element与concrete value-class字段的现有deinit
调度可达性缺口由最小递归补齐，未扩大generic/interface/nullable/Rc等既有拒绝边界。

原教程三source的资源oracle另有1项通过（0 failed/807 filtered/0 ignored）：两入口×四组argv，
8次instrumented native逐pointer核对previous/final字段、Report、逆序argv String及argv buffer释放；
String drop入口同时核动态owner唯一drop、空String次数及start/processed/done各一次，完整stdout/空stderr/exit0。
conditional receiver定向1项通过（0 failed/809 filtered/0 ignored），24组覆盖implicit/explicit、
Copyable/MoveOnly、循环内/外/两provider之间及两入口，精确分配ID释放次序与deinit输出共同观察。
Inout、this.field、implicit field、普通field projection负例1项通过（0 failed/809 filtered/0 ignored），
均先有合法前端事实，再核两native入口的精确Unsupported span、无LLVM调用及目录/旧artifact保持。
这些均为未提交开发快照证据；完整N5身份/原子性回归、最终基底R1/E1与双宿主仍待完成。

整合已完成0266的main `44cb312f1539b817d5fbabacc5713d2aa5f8b5f6` 后保留所有开发改动；
共享counter同时保留sanitizer产物留存与本片可选free顺序观察，需直接回归两类调用方。

整合后 `python3 -m unittest discover -s scripts/tests` 132项通过（28.83s），文档结构510页通过；
共享counter冲突已独立审阅，确认旧普通/留存调用行为及新增顺序断言完整。实际Rust调用方回归待执行。


整合后codegen lib的 `unit_for_` 定向选择首轮19 passed/0 failed/798 filtered/0 ignored，220.07s，包含后续补入的源求值return/Abort、
元素receiver与argument借用复用、pending field replace、resource value-class嵌套return。
最终生产逻辑与测试独立full-pass审阅通过，明确检查完整cleanup一次消费、CFG槽位、
continue/break temporary集合、共享provider advance保持single顺序，以及非真空的指针/输出oracle。
审阅没有运行Cargo，不能替代随后补入的边界/SSA测试、R1/E1或双宿主CI。


## 8. 最终本机验收（PR CI前）

基底为main `44cb312`。Rust均使用既有共享target与LLVM21.1.8，命令带 `--locked --offline`，
同一时刻只有一个Cargo进程。以下选择互有重叠，不将通过数相加称为独立测试总量。

| 命令选择（`cargo test -p lang-codegen`） | passed / filtered / ignored | 观察 |
|---|---|---|
| `native::unit_tests -- --nocapture` | 128 / 692 / 0 | 484.34s，包含最终22个unit_for_新增用例、旧actual-for原子性及所有现有unit native下游 |
| `ssa::unit_lower -- --nocapture` | 138 / 682 / 0 | unit scalar/control/while/Borrow/closure/container/receiver/field/deinit |
| `sequential_for -- --nocapture` | 67 / 753 / 0 | 221.83s，共享provider的single SSA/native及资源/原子性下游 |
| `native_sanitizer_tests -- --nocapture` | 2 / 818 / 0 | M4a IR接线及counter编译/运行失败证据保全，故意invalid LLVM为测试预期 |
| `native_tests::boxed_enum_tests -- --nocapture` | 4 / 816 / 0 | 共享counter原普通调用行为 |
| `iteration_boundaries -- --nocapture` | 3 / 817 / 0 | 两native视图明确拒绝、constant混轮；也包含于上方128项 |
| `unit_for_snapshots -- --nocapture` | 1 / 819 / 0 | length只在preheader取一次、header guard及provider无allocation |

各命令另启动integration binary为0 matched/2 filtered，未计为其覆盖；所有实际命中均0 failed。
未运行全量frontend、全量codegen或完整CLI Rust套件；上述定向选择不能替代PR必需组合CI。

`cargo clippy -p lang-codegen --all-targets -- -D warnings`、
`cargo check --workspace --all-targets`通过；fmt、diff检查通过。
尺寸门禁比较base44cb312，通过772个手写Rust文件，45个历史超限只报告旧欠账；
三处接线/签名格式增长按1263/1055/2016精确额度登记，baseline未提高。新production文件
iteration.rs344行、iteration_cleanup.rs150行，新native helper/tests均低于250行，call.rs985行。

独立full-pass与窄复审已闭合，含共享provider、temporary集合、完整有序cleanup、资源oracle、
counter整合、capture负例、两Architecture及尺寸例外。capture fixture的Borrow事实已证实，
其生命周期在CallReturn结束，不将它写成已执行EndCaptureLoan；captured Borrow native仍明确Unsupported。


最终本机 `cargo build --locked --offline -p lang-cli` 成功（1.92s），随后
`python3 scripts/check_tutorial.py --cli <共享target>/debug/kovenc` 完整执行17个案例通过，
含15个成功运行及2个精确诊断案例；parameter-report四组均实际build/artifact/run且完整输出匹配。
planned-thread仍为候选，明确未执行。CLI SHA256为
`9c20ae9b4311fd3678d3b1f8215d3e1d4135ee04e369697cc58f3185af3f91e2`，
本机证据标明是基底44cb312上的未提交开发树，不能冒充clean PR head的执行证据。
至此本机N1–N5/P1/R1/E1的所选检查及独立审阅完成；精确head双宿主CI仍是归档和M1A总退出前置。
