# SPEC-0320: Enum case 构造值的 Borrow 实参分类

> **性质**：有界修复草案 · **状态**：draft · **读取时机**：审查 enum case 值与真实 place 的调用期借用分类时 · **唯一真源**：本合同；语言规则仍由现行 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2P3-0320` |
| 所属 Phase | Phase 2 expression/call argument category；Phase 3 调用期 loan；Phase 6 两入口结果 |
| 语言规范 | [构造身份与参数](../../../guide/11-copyability-layout-construction.md#可构造目标与唯一身份)、[调用期loan](../../../guide/10-ownership-borrowing-drop.md#所有者参数绑定与调用期-loan) |
| 批准依据 | 2026-10-14 用户批准A诊断封闭→B enum构造实参分类→C List MoveOnly追踪；本阶段仅B合同与普通源码红测设计，未执行新目标或修改生产 |
| 前置 Spec | [SPEC-0319](../../active/0319-final-type-admission-diagnostics.md)；最终类型准入保持，不混入其PR |
| 分支基线 | `fix/spec-0320` 显式依赖0319本地最终 `6c1a11ca87b642849c6ba7c3be008a7aebece759` / tree `e2d1a5179c7102336154570c95cbd6e6774b5799`；该输入尚未合main |
| 官方核验 | 本轮只读fetch main仍为`188654603edde5ece3edab114458116c61510371`，全局最大编号0319；0320已保留。实施前正常合入0319实际main并重新冻结，不改写已发布历史 |
| 语言语义变更 | 否；既有Guide已规定无payload case为构造值、Borrow temporary合法、Inout temporary非法 |
| 阻塞项 | 草案/候选helper独审、实际main同步与真正expected-positive红测尚待；本片未实现、未验收 |

## 1. 单一 Goal

已成功选择的 enum case construction 是新值，不能只因其 AST 写作 `Name`/`Member`，
就把它当作可回溯到 binding/field/element 的 place。single 与 unit 必须发布一致的
expression category 和外层调用 argument category，使 Borrow 临时值建立真实调用期 loan，
不在 ownership 抛 `InvalidUnitArgumentPlace`，也不以“无诊断但缺loan”冒充成功。

修复依赖实际 construction expression/target 身份，不依赖 `E.C` 拼写、结果 enum 类型、
case 名称或同类型比较；已绑定 enum local、参数和字段仍是 place。只修分类与对应诊断，
不放宽ownership place查找、不允许借用逃逸，也不新增enum when或泛型enum native能力。

## 2. 原附件与当前静态证据

输入取已核最新95-probe双矩阵包，tar SHA256
`a2eed0228ac3b43aa41eceb17f71f96d173d29560762fcc2867f3031e5df1bd3`。
下表是作者b43增量矩阵，不是本轮6c输入的动态结果；原源码需原字节保留。

| 原probe | SHA256 | 作者single / project | 要分离的根因 |
|---|---|---|---|
| `p49_when_enum_no_payload.ko` | `5679bacd7fe7b110811b816f75352b0bc7538c9c9cbf688e3d483c543fc96cb6` | MissingFact / ownership内部错误 | Borrow实参`Color.Green` `[177,188)`；后续unit enum表达式条件另受限 |
| `q08_enum_string_payload.ko` | `4a9c35ed04b945fd9dc0b144ca8c8e90aa5c0174b194a0d22827fa5fb984f12e` | UnsupportedNode / ownership内部错误 | Borrow实参`Msg.Quit` `[187,195)`；String使root enum为MoveOnly，when另受限 |

当前6c源码只读链：

- single `checker/construction.rs::check_bare_enum_construction` 和 unit同名方法已在真实
  Name/Member expression上发布零operand descriptor，结果为root enum类型。
- single `checker/callable.rs::classify_expression_category` 仅先识别constant等，已知Member默认Place；
  unit `bodies/checker/expression_facts.rs::expression_category` 最后落到syntactic-place规则。
  unit的enum短名Symbol也不能当作local binding；分组必须沿内层真实category传播。
- unit `ownership_checking/compilation_unit/dataflow/call_contract.rs::apply_contract` 对SharedLoan
  的Place分支必须找回真实place或temporary projection，否则返回内部错误。single
  `checker/loan.rs` 无place且category非Temporary时可能不发布临时loan，故不能只断言API返回Ok。
- 0319前置还有独立待确认风险：`final_admission/adapters.rs`的construction选择只标static_callee，
  未标Member qualifier；`walk.rs`可能继续把`Flag.On`的静态`Flag`当值检查，而bare构造成功
  未为该receiver发布值类型。因此公开入口可能先被final缺事实阻断，尚无本轮动态证明。
- 以上是静态疑点；先用真实facts锁descriptor、category、call参数与loan，再最小修复。
  不预先把作者矩阵或尚未执行的候选写成本轮红测。

## 3. 冻结合同与反例

1. 成功的无payload case Name/Member及其Group作为值为Temporary；对应外层Borrow参数
   保留同一expression、参数映射及临时loan target，begin在operand求值后，end在该call返回。
2. Copyable enum不新增唯一drop义务；含String的MoveOnly root空case仍遵守原construction owner
   与CallReturn清理，不能在callee内提前drop、漏loan或双重消费。payload构造调用是既有Temporary对照。
3. 同一enum类型的local/参数、真实field、普通element及同名值receiver保持Place和原loan target；
   static construction不能遮蔽合法值名字，不能把所有Member或所有enum类型一律改Temporary。
4. 明确`&E.C`交付Inout应在类型阶段使用既有L0122，primary为真实`&` marker、label为参数声明；
   不以ownership内部失败或更晚不可变place诊断代替。合法`var value = E.C; touch(&value)`保持原成功合同。
5. 失败/未完成构造不得伪造descriptor或取得0319 final资格；原L0144推导边界、显式参数/Value模式、
   payload copy/move与普通constant category保持。不新建诊断码，不改变raw recovery或type trial回滚。
6. unit basic/constant、正逆source inputs按同一身份规则验证；泛型case仅做已有类型/ownership对照，
   不因前端修复宣称generic enum native已交付。single/project源级失败沿原exit/JSON协议。
7. raw descriptor/category与原ownership结果、0319 final资格必须分开采集，不能先expect final成功
   而丢失分类/loan证据。若真实红测确认静态qualifier缺口，已获准在本片修相同construction角色的
   窄接缝：必须同时绑定已选EnumCase construction与真实Package/Type qualifier身份；仍检查
   runtime receiver和所有值operand，不绕final门、不豁免整条Member链、不改0319已发布验收范围。

## 4. 代表源码与后续 when 边界

首个公开正例去掉when，用`Flag.On`直接交付普通Borrow参数并返回Int，main只比较Int后打印
固定String；相同源码分别build/run，记录每入口实际首个失败阶段，不通过修改callee为own或先存local绕开直接临时借用。
MoveOnly空case与payload调用另作前端loan/owner对照，原p49/q08仍原字节独立记录。

原probe不以“全文编译成功”为目标：unit `unit_lower/control.rs::lower_when_condition` 明确拒绝
有tagged subject的表达式条件；`lower_boolean_when`明确拒绝MoveOnly enum subject。
已有`guide_litmus_04_unit_enum_expression_condition_remains_an_explicit_boundary`锁
UnsupportedNode及`Shape.Point -> 0` Span，`move_only_enum_when_waits_for_branch_qualified_subject_drop_facts`
保留独立UnsupportedNode。后继真实baseline要确认原p49/q08经过0319的结果，分别锁根因/阶段与Span；
若更早出现独立准入边界则据实分账，不改0319或when gate凑绿，不把此对照当新增when能力。

## 5. 拟精确红测与安全边界

以下是尚未落地的候选名字，不是已编译目标或开测名单。先固定tests-only输入、helper链、
源manifest和预期，再交独审逐名授权；普通源码只读facts，不构造伪descriptor/IR或强制observer失败。

| 拟target / 精确函数 | 要锁的合同与helper |
|---|---|
| frontend `enum_case_borrow_categories` / `selected_case_values_are_temporary_but_bindings_and_fields_are_places` | 普通lex/parse→single/unit names/types；先采raw、不以前置final成功为条件；零operand EnumCase descriptor、root类型、表达式与外层Borrow实参category；直接/Group/短名及place反例 |
| 同target / `selected_case_qualifiers_keep_final_admission_precise` | 独立调用0319 final，采真实结果；已选case与Package/Type qualifier、runtime同名String/field对照，防全链或同类型豁免；与raw分类红分账 |
| 同target / `borrowed_case_values_publish_call_scoped_temporary_loans` | 原check_ownership/check_compilation_unit_ownership；Copyable与MoveOnly空case、payload对照；精确临时loan、call Span/结束点及唯一owner消费，不只看无诊断 |
| 同target / `cross_file_case_aliases_preserve_source_qualified_identity` | 两文件普通unit、真实正逆inputs、qualified/import alias、basic/constant；不按同类型合并目标 |
| 同target / `inout_case_temporaries_keep_l0122_and_mutable_places_stay_valid` | 普通非法`&E.C`/Group与合法mutable local；精确诊断码、marker与parameter label，成功case不得提前错误 |
| CLI `native_cli` / `enum_case_borrow::single_direct_case_borrow_builds_and_runs` | 现有run→原版kovenc，固定无when正例，build/run原输出、退出与产物 |
| CLI `project_cli` / `enum_case_borrow::project_direct_case_borrow_builds_and_runs` | 原TestProject/project_build/run→kovenc，同源码及单独constant路径；不改IR或runtime |

既有边界与共享回归另取精确安全名单：source construction的parameter mapping/expected inference、
ownership construction临时owner与drop、字段/元素place、原两个unit when拒例及0319 final保护。
必须静读每个实际helper后再运行；不继承0319的81计数、不跑suite/discover或云故障、counted、
sanitizer、calibration、stage/aggregate。双宿主CI继续按真实工作流验收，新增target须显式接线。

## 6. 状态与停止条件

- [x] 官方main/编号静核，保留0320；新worktree基6c，0319依赖显式记录。
- [x] Guide构造身份、Borrow temporary与Inout限制核对；原两probe字节与作者矩阵分账。
- [ ] 草案与精确源码/helper独审；实际main同步与tests-only固定。
- [ ] 真实expected-positive红测、最小分类修复及不误杀/原when边界回归。
- [ ] 工程门禁、双宿主CI、Architecture/验收归档及主干交付。

若必须修改Guide、重写通用place解析、放宽所有权或扩展when才能达成目标，停止并报告。
任何fixture前置失败、已绿对照、未执行后续oracle与真正行为红分账；C的List MoveOnly追踪独立，
不得借本片编号或后端拒例夹入实现。
