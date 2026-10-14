# SPEC-0317: 同根范围绑定内的有界正常短路

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施范围绑定与正常短路组合时 · **唯一真源**：本页合同与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0317` |
| 所属 Phase | Phase 4 typed SSA 能力门与 Phase 5/6 正常 native 验收 |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Boolean 类型](../../guide/03-types-generics.md)、[N1a](../../guide/12-collections-destructuring.md#n1a-单来源连续范围-carrier) |
| 批准依据 | 用户“按建议顺序继续实施”的主线及小阶段提交授权；先 Spec/真实红测审阅，合并仍单独确认 |
| 前置 Spec | [SPEC-0298](../../archive/specs/0298-range-binding-local-control-flow.md)、[SPEC-0315](../../archive/specs/0315-range-int-comparisons.md) |
| 基线 | official fetch 确认 actual main `b1165b0f430b5e8c3bcd669d5129fce6b4a9e1a2`；独立 `feature/spec-0317`，不依赖0316 |
| 影响范围 | range binding 能力门、两入口 typed call effects 验证、普通源码及公开 project CLI 测试 |
| 语言语义变更 | 否 |

## 1. 冻结合同

仅接通同一直接词法 Block 内已建立的同 root NewRangeDescriptor 上的单层 `&&`/`||`。
全部直接 borrow binding 在表达式前，逐 binding 有相同精确 origin、source continuation
和唯一 `AfterStatement(Block)` end。源码选择不重建 lifetime，不从同型 TypeId 猜来源。
两个 operand 和结果必须由 typed facts 证明为 Boolean；保留 LHS 一次、RHS 选中一次/
跳过零次。使用位置限 Boolean 局部 initializer、同步 Boolean 实参、既有 bounded statement-if
的条件；所有路径正常返回，合流继续读取既有范围。

操作数可用 Boolean name/literal、Group/`!`、简单 operand 的既有 Int 比较，以及正常 owned Boolean 的
只读同步 call（只读指 mode/loan 与交付效果，不禁止 println 等可观察 IO）。call 还核实参与 receiver 的传递 mode/copyability/effects，不能仅凭
Owned 结果放行：拒绝 owning MoveOnly、Inout/mutating receiver、borrow/carrier result 和
新范围构造。比较内的 checked 算术及一元 `+/-` 仍留独立后继。允许现有 metadata 的同步 Borrow 读取；其临时 read loan 在调用返回结束。

复用既有普通短路 CFG 和 carrier transport，逐边证明 root owner、descriptor、protecting
loan 与 source continuation 精确配对、恰一次携带和重绑。RHS 的 read loan 只在选中边建立/
结束；skip 不执行 RHS 工作。结束消费原 facts，child/sibling 先于 parent，最后恢复根权限。

## 2. 非目标与后继

binding 前/已结束后短路、嵌套或链式短路、普通 borrow/metadata alias、多 root、分支新 binding、
复杂 RHS、nullable/传播/索引/插值、when/value-if、loop/lambda 内短路和 return/break/continue/
Nothing 仍明确拒绝。它们留独立后继，因为当前合同仅证明既有同根词法 scope 的正常合流，
不能外推一般 NLL、条件 move/borrow、borrow 存储逃逸或任意控制流。Map return operand 独立处理。
不改 Guide、frontend 所有权语义、ABI/verifier、尺寸 cap 或测试 aggregate。single CLI 不加载 range
标准库，不由 single codegen 的测试内可信 provider 注入推定该 CLI 已支持。

## 3. 唯一验收矩阵

| 验收项 | 精确目标/过滤器 | 实际状态 |
|---|---|---|
| 新真实源码红测：2 op × 3 用法 × 5 入口；动态四组 LHS/RHS | codegen lib `range_short_circuit_`，single、unit basic/constant、provider 正逆序；String/MoveOnly/Resource 同根父子兄弟 | tests-only `50fda166`：43目标精确UnsupportedNode/Span红；循环首例阻断后续fixture的边界见下文。最终1976通过 |
| literal/const Never/Always、纯 Boolean/!Group/简单 Int 比较 | 同过滤器 `fixed_`、`scalar_`；single constant 是同一入口的 fixture | 11目标通过，包含在新增57项目标；不另算single入口 |
| SSA identity/cleanup 与原版 LLVM/native | `short_circuit_transport` 与 `short_circuit_prefix_transport`；正常合流read、逆序析构stdout | 35动态/前缀目标通过；精确root/descriptor/source/参数loan，stdout不称动态释放计数 |
| 保留边界与精确诊断 | `boundary::`：nested、scope/roots、owning/Inout、alias、新范围、loop/lambda、比较内部额外CFG | 7项合法源码backend边界通过；另1项frontend精确L0085/L0164，不冒充backend拒绝 |
| 旧拒绝逐例迁移 | `local_control_boundary`、`local_comparison_boundary`原flag &&/|| Bool实参；`scalar_counts`原initializer | plain operand正例及scalar initializer正例通过；其余拒绝保留并随完整受影响模块复验 |
| 公开 CLI basic/constant 与旧拒绝迁移 | `cargo test --locked --offline -p lang-cli --test range_project_cli -- --nocapture` | 完整46/46通过，0 filtered/ignored；含7项新增目标和旧scalar拒例迁移 |
| 共享回归与总数 | codegen lib `ssa::range_lowering_tests::local_control::` 280；`scalar_counts::` 7；原single/unit/constant短路6，普通borrow call/boundary 9 | 去重302/302通过，0 ignored；新增57项是其中子集，非全codegen/全frontend |
| 工程与文档 | fmt；`cargo clippy --locked --offline -p lang-codegen -p lang-cli --all-targets -- -D warnings`；CLI build；docs/diff/size | 全通过；669 Markdown；1098 Rust文件、38历史超限未增长，未改尺寸cap |
| 双宿主交付 | 独立 Draft PR 精确 head CI 与最终归档 | 待发布/CI；本地仅Linux x86_64，macOS未运行；未跑云fault/calibration/sanitizer/aggregate或frontend全量 |

## 4. 实施与证据

1. Spec 和真实源码 tests-only 先冻结小提交/bundle，记录生产零改与实际红测，交审。
2. 审阅后最小实现；同一选择重跑，并反查全部被选中的旧拒绝与公共 CLI。
3. 更新 Architecture 的实际边界和本表结果；实现、CI、归档与 main 合并分别记证。

原始日志/JSON/source hashes 留仓外；Git 只留此简账和真正读取的 fixtures。Rust 1.96.0、LLVM
21.1.8 已核；所有 Cargo 串行、独立绝对 target，执行前后冻结源码 hash。只运行静态审过的
正常 source/native 与合法诊断，云上不运行故障注入、校准、sanitizer、aggregate 或泛 discover。

### Tests-only 检查点（2026-10-14）

生产仍为基线 b1165b0f。codegen `range_short_circuit_` 最终实际50项：43项新增正例在各自
精确短路 Span 以 UnsupportedNode 红，7项保留边界/前端精确诊断通过，0 ignored。每个循环
首个源码红后不继续，尚未执行的 MoveOnly/Resource 与后续真值 fixture 不算已红或已运行。
原 scalar initializer 迁移另1项真实 UnsupportedNode 红。公开 CLI `boolean_short_circuit`
实际7项：6项正例红、1项边界通过，均frontend成功后到达UnsupportedSource。

初次边界将 direct Nothing 与 borrow Boolean result 错列 backend 拒绝：实际分别为 L0085
与 L0164；现已独立前端诊断/Span测试。两次新增诊断测试的Rust accessor编译错误已修正，
均保留仓外原日志，不计成功。最终 codegen/CLI 各次执行前后完整源码hash一致；docs、fmt、
diff及尺寸检查的精确结果随冻结提交记录。尚未生成正例 native 产物，未实施生产放行。

### 首次实现检查点

共享 selector 只放行直接 Boolean initializer、直接 statement-call 的 Boolean 实参或既有
statement-if 的完整条件；`val x=wrap(lhs&&rhs)`、非 scalar 外层 call 与其它嵌套使用仍拒绝。
只新增选择与 typed-call effects 判定，原 CFG/carrier/end/verifier/LLVM 均未修改。

codegen `range_short_circuit_` 实际51/51通过，0 ignored，1841 filtered；原版LLVM/native
已遍历动态测试的全部String/MoveOnly/Resource、五入口和真值组合，以及literal/const首片。
此前31项失败中30项是断言把缺省Borrow Boolean实参误认为Value，已按真实Loan形状修正；
另1项揭示single既有普通receiver能力差异：其精确`gate.test()`仍UnsupportedNode，unit独立
正例通过，single的own Copyable参数另验。该差异不由0317补齐，不依赖0316。
生产首次编译遗漏两个旧consumer闭包的签名适配已修正；原日志保留，不计通过。
尚待更早Borrow实参pending-loan组合、旧baseline receiver探针、共享回归、公共CLI转绿及clippy。


### 补强检查点

更早Borrow View实参的pending metadata loan已增加精确逐边identity、合流单一调用交付、
call-return早于RangeEnd结束断言；RHS缺省Boolean实参另核shared Boolean type、原right参数
来源与每边重绑，不结束继承loan。56项新增目标通过；随后outer owning/Inout/新metadata
prefix拒绝也通过。旧50fda生产另用无短路receiver源码实际验证1/1精确UnsupportedNode；首次
短名`--exact`零命中不计通过。错误新增Boolean loan结束断言已修正并保留失败记录。

最终输入为 `1976cc2bd386759df32627f332709842903e7de2`，tree
`7d6bfd4fe12a83340c4dea2b7fba46306df8b202`；两条比较内部算术/负号合法边界已纳入。
在此精确snapshot重跑上表全部选择，302 codegen与46 CLI逐名去重、所有工程门禁通过，
0 ignored；选外测试均未执行，不据定向通过推定全仓完成。前后完整源码清单SHA256同为
`be2afa50eecfc6c84672c68782c1254886131a8bec581c433cedacd824853035`。
随后只更新本页验收与索引，不改变已验Rust/fixture/依赖/feature输入。仓外原日志、完整
源码清单、逐名名单和机器摘要已随恢复检查点交付；远端CI必须另核精确PR head。
