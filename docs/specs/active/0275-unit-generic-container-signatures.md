# SPEC-0275: Unit 泛型函数直接容器签名的具体类型替换

> **性质**：有界变更合同 · **状态**：approved · **读取时机**：实施 M3A 的既有泛型容器签名支持时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | approved |
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
| G1 | 三容器×Int/String；空/非空、显式/推断、Borrow重复读取后再用、own返回；SSA参数/返回/ContainerLength active Loan的具体身份一致 | 正式红测待运行 |
| G2 | 三容器×资源元素；Borrow不提前drop，pass后每元素唯一逆序清理；正常allocation/free逐指针和次数匹配，无clone/retain | 待运行 |
| G3 | 双文件同名T的来源隔离、重复实例去重、输入顺序确定性；backend前后typed arena不增长；所有canonical来自真实调用/签名 | 待运行 |
| G4 | 下述三联边界及缺substitution诊断；精确kind/实际source Span、arena不增、产物写出前拒绝；已有CLI原子性消费者证明旧产物保全 | 待运行 |
| G5 | 最近unit plan/recipe/cycle/error-order、unit容器和直接native；CLI/M1A/词频消费者；独立全审、Architecture、双宿主必需CI、归档最终head与合并/main闭环 | 待运行 |

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
