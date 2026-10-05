# SPEC-0275: Unit 泛型函数直接容器签名的具体类型替换

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施 M3A 的既有泛型容器签名支持时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-275` |
| 所属 Phase | Phase 4，消费 Phase 2/3 已有类型与所有权事实 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[类型与泛型](../../guide/03-types-generics.md)、[集合](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行、按实际情况调整草稿的站立授权 |
| 前置 Spec | SPEC-0274 |
| 前置 ADR | ADR-0008、ADR-0016、ADR-0018 |
| 影响范围 | unit concrete resolver、直接 SSA/native 测试与事实快照 |
| 语言语义变更 | 否 |

## 1. Goal、基线与启动证据

普通 compilation-unit 泛型函数签名中的直接 `Array<T>`、`List<T>`、`MutableList<T>`
能够消费前端已发布的 concrete identity 实例化，既有 `.size` 和 owner 参数/返回可执行到 native。
不新增语言规则、公共 API、表示或 runtime ABI；本片承接
[M3A 草稿](../../development/sequential-collections-spec-draft.md) C07 的既有支持补齐。

前置0274实现head `3151690` 的 PR CI37246195048、最终归档head
`cdb9cba803c401c3f6d246459ad8a5e10a50dd1f` 的
[CI37247799491](https://github.com/Halckon/Koven/actions/runs/37247799491)分别15/15成功，
两轮双宿主各37条词频公共命令及各12条独立候选消费者命令已逐条复核完整输出。
[PR52](https://github.com/Halckon/Koven/pull/52)已合并为
`c5c4a8df271858b23898e0bcf4b55d823c760bb9`；本片worktree基于该最新main，分支`feature/spec-0275`。
原main与remote 0/0，原有6个dirty文件及状态逐项哈希保全。
main push CI37248672159启动时仍在执行，尚不计作通过；正式生产实施前确认该基线结果。

## 2. 缺口与现有接口

`unit_plan/concrete_types.rs::resolve_concrete_type`已处理direct T、Nullable与有限Nominal
替换，但仍含T的Intrinsic落入UnsupportedNode。前端按真实callsite实例化参数和返回，
已发布签名对应具体容器；不重新实例化泛型body。planner在runtime demands为空时可能成功，
不能以此替代SSA/native验收。

预计唯一生产改动是为三个容器、恰好一个类型实参加入封闭分支，复用
`resolve_direct_type_argument`与`typed.types().find`；不能backend intern或直接搬用更宽的
`specialize_preflight_type`递归替换。size descriptor、current SharedLoan、container header、
元素表示与drop继续消费已有事实。缺substitution/identity仍MissingFact；错误沿用传入的
实际source Span（参数路径为声明参数Span），不改变诊断来源。

## 3. 范围与普通函数入口

双文件首片函数形态如下；不是新增标准库表面，三容器按同一合同验证：

```kotlin
package p
fun <T> sizeOf(items: Array<T>): Int = items.size
fun <T> pass(own items: Array<T>): Array<T> = items
```

具体元素至少Int、String及有真实deinit的普通资源class；空/非空、显式/推断实参、Borrow
重复读取后再用、own pass返回后唯一清理。直接T可以替换为前端已发布的具体List<Int>，
但模板List<List<T>>的递归替换不加入。不得用unused声明补种canonical类型以掩盖缺口。

## 4. 必需验收

| ID | 场景与定义完成 | 当前状态 |
|---|---|---|
| G1 | 三容器×Int/String；空/非空、显式/推断、Borrow重复读取后再用、own返回；SSA参数/返回/ContainerLength active Loan的具体身份一致 | 本地通过；4项SSA及6个native Int/String夹具，CFG核对current loan；三项目9条公共build/artifact/run命令通过 |
| G2 | 三容器×资源元素；Borrow不提前drop，pass后每元素唯一逆序清理；正常allocation/free逐指针和次数匹配，无clone/retain | 本地通过；三资源夹具完整逆序stdout及各3次allocation/free逐指针相等；所有新native正例禁止StringClone/SharedRetain |
| G3 | 双文件同名T的来源隔离、重复实例去重、输入顺序确定性；backend前后typed arena不增长；所有canonical来自真实调用/签名 | 本地通过；5项plan边界测试和SSA canonical身份检查；无unused seed |
| G4 | 下述三联边界及缺substitution诊断；精确kind/实际source Span、arena不增、产物写出前拒绝；已有CLI原子性消费者证明旧产物保全 | 本地通过；三nested具体T native夹具；两拒绝×新/旧目标共4次native emit，文件名与全部bytes不变，无sibling temporary |
| G5 | 最近unit plan/recipe/cycle/error-order、unit容器和直接native；CLI/M1A/词频消费者；独立全审、Architecture、双宿主必需CI、归档最终head与合并/main闭环 | 本地unit plan 53、container 68、CLI 9+12、M1A 4 cases/12命令、词频37命令通过；严格clippy/fmt/尺寸/docs通过；独立production/native全审通过，最终文档复审通过；实现head PR CI必需14job成功、编辑器合法skip；最终归档/main交付门禁待 |

G4是必需三联，不能仅让正例变绿：

1. 实际调用已发布Container<List<Int>>，直接Container<T>、T=List<Int>成功，验证外层长度与完整清理。
2. 即使已发布List<List<Int>>，递归模板List<List<T>>仍UnsupportedNode及精确实际Span，
   防止把缺canonical误当替换深度保护；不拓宽独立owner recipe许可。
3. body-only `fun <T> probe(own x:T):Int { val xs=listOf(x); return xs.size }`及probe(1)，
   lowering前明确find(List<Int>)==None、无seed；失败为MissingFact及精确实际Span，
   arena不增，无object/新产物。该缺口留待前端实例body需求发布的独立后继。

## 5. 验证范围与接口复用

新增独立resolver/SSA/native测试模块，不继续增长2022行native/unit_tests.rs。
复用unit test analysis、source identity、existing verifier/native harness与资源counter，
本地Cargo由root串行；不以全量frontend取代相关合同，也不以mock替代实际object/link/run。

最小充分序列：定向红测→最小resolver分支→G1–G4→unit_plan_tests与unit_lower_container
相关消费者→CLI native/project、M1A/词频实际公共命令→严格clippy、fmt/尺寸/docs→双宿主PR CI。
CI的bounded composition继续执行全部codegen tests及现有消费者，新模块必须实际命中。
旧尺寸baseline不提高，模块注册优先在有空间的相关模块内完成；发现真实范围扩张先更新合同。

## 6. 非目标

body-only新canonical发布、single-file resolver统一、任意递归模板替换、inherited/delegated
owner recipe扩张、runtime-length initializer、一般Inout/投影、增长/删除API、短路owner
last-use合流、M4b真实故障校准、公开Release与性能不在本片。
现行语言允许的未实现行为仍是独立缺口，不将本片后端拒绝提升为语言规则。

## 7. 顺序与提交

1. 正式合同/前置/接口核对 → docs/DAG及独立准备审阅；未记任何G项通过。
2. 在本分支建立真实失败SSA/native测试，保留缺canonical与递归模板边界 → 确认命中及原Span。
3. 最小生产替换 → 直接/共享/下游验收、独立完整审阅 → 单一逻辑实现提交。
4. 精确head首轮CI → 真实账本、Architecture与归档 → 最终归档CI全部成功才合并，再核main CI。

合同与实现可分别提交，信息含SPEC-0275；测试文件可独立委派，production及Cargo由root
控制，不能争用target或在前置未确认时生成“已完成”记录。

## 8. 准备与失败证据（2026-10-05）

- 原315生产CLI在checkout外实际双文件project build：三容器generic Borrow size均exit1，
  native UnsupportedSource含UnsupportedNode，无产物；命令/源码/编译器SHA与完整输出保留
  `/private/tmp/m3a-direct-container-precheck/results.json`，不是长期交付验收。
- 同组仅移除函数<T>并将Container<T>改为Container<Int>：三容器build/native共6命令
  exit0，完整stdout为ok LF；记录`concrete-controls.json`。正式新分支仍需重建失败测试。
- 初次夹具包路径错误L0146/L0080与error缺参数L0121分别纠正并另存原记录，不能当backend红测。
- 独立准备审阅发现G4缺canonical负例原为可选，已升必需三联；Span沿用真实传入来源，
  两项修正窄复核通过。尚未运行正式Rust/双宿主G1–G5，main CI待实际结果。


2026-10-05 基线门禁续记：main CI37248672159已15/15终态成功，root独立读取两宿主各37条
词频完整bytes及候选各12命令/HEAD-tree-status三checkpoint/cleanup-sentinel通过；
[0274最终PR与merge/main闭环证据](../../development/evidence/word-frequency-0274-delivery.json)
保留精确run/head/tree/artifact与原账本哈希，原archive初次验收历史未重写。生产实施前置已满足。
native定向3函数实际命中，0passed/3failed/0ignored/843filtered；均在真实泛型参数Span
返回NativeObjectError UnsupportedSource、detail UnsupportedNode，日志为本机临时0275-native-red.log。
该记录是正式失败证据，G1/G2/G4不因此记通过。

定向全filter初次实际12项：1passed/11failed/0ignored/843filtered；recursive template在已有
canonical的条件下明确拒绝，其余UnsupportedNode及MissingFact精确分类对照确认缺口。
仅新增三个单参数容器含T的resolver分支，继续复用resolve_direct_type_argument/find。
修复后结果待记录，尚不记G1–G5完成。

## 9. 本地实现验收续记（2026-10-05）

最小resolver修复后首次定向为11passed/1failed：CFG测试错误地禁止println独立String
temporary清理。仅收窄测试到容器loan/owner身份，current非entry block parameter要求保留；
单项重跑1/1通过，production没有因该夹具调整。container filter实际命中全部13项新增函数。
独立全审发现G4缺native目标保全证据，补4次真实emit后1/1通过；窄复审确认该P2已关闭。

实际命令使用`--locked`、LLVM21.1.8和共享target，本地Cargo串行：
`cargo test -p lang-codegen --lib unit_plan_tests`为53/53；同crate `--lib container`为68/68；
`cargo test -p lang-cli --test native_cli --test project_cli`为9/9及12/12，全部0failed/0ignored。
`cargo build -p lang-cli`重建后，三项目9条公共命令完整输出及源码/编译器SHA保留
[public账本](../../development/evidence/generic-containers-0275-public.json)。
`check_tutorial.py --example parameter-report`执行4组argv、12命令；
`check_word_frequency.py`执行37命令，正常完整bytes、预期Abort及非法UTF-8出口均按原oracle通过。
临时原始日志/词频账本位于`/private/tmp/0275-*.log`和`/private/tmp/spec0275-word-frequency/results.json`，
长期双宿主交付证据待精确PR head实际CI，不将临时路径当永久存档。
`cargo clippy -p lang-codegen --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
`check_rust_sizes.py --base c5c4a8d`及`check_docs.py`（519 Markdown）通过；尺寸baseline未增加，
旧45项超限欠账仅报告，新测试模块均小于1000行。未运行本地全量frontend、真实故障校准或性能测量。

## 10. 精确实现 head 双宿主 CI 与归档门禁（2026-10-05）

实现head `19ff893cd756ef8aa8d20439094d9cb9e6413eef`、tree
`bb8c0d19a19db17f4637e4d49cff153f32e47130` 的
[PR53 CI37251289329](https://github.com/Halckon/Koven/actions/runs/37251289329)已终态success。
15job中14项实际success，Tree-sitter CLI Corpus因editors=false合法skipped；
CI Passed实际验证changes与必需门禁，不能把该skip记为编辑器执行通过。
Linux完整codegen856passed/0failed/0ignored，macOS855passed/0failed/1ignored；
唯一ignored为既有LLDB runner权限项。两宿主均逐名核对13项新增generic-container函数全部ok，
不以总通过数替代新测试命中。两宿主各4个codegen doctests及所选下游均0failed。

root独立读取原始word-frequency与preview artifact：每宿主37条词频完整argv/bytes/exit
按独立参考验证，9个项目源码SHA来自真实教程；每宿主preview独立消费者12条命令完整bytes、
4项目、cleanup与sentinel通过；producer三个HEAD/tree/status checkpoint与manifest相同且clean。
默认测试checkout的synthetic commit/tree/parents及producer实际source head分别记录，
不得把synthetic commit写成源码head。永久[CI身份与原始词频账本](../../development/evidence/generic-containers-0275/ci.json)
保留全部job ID、artifact ID、SHA和13项命中；G1–G4代码验收与G5实现head门禁已满足。

按AGENTS分支交付规则迁移done至archive，以首轮精确head验收归档；PR仍OPEN，
G5剩余交付门禁是最终归档head双宿主CI、确认无未决状态后merge、实际merge/main CI。
尚未发生的结果保持待验证，合并后闭环证据在后继启动记录中补充，不回写本次历史。
后继body canonical发布、递归模板、M1B-b、真实故障校准、性能与全部M3A仍未完成。
