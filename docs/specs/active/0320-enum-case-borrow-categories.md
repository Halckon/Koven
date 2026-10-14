# SPEC-0320: Enum case 构造值的 Borrow 实参分类

> **性质**：有界修复合同 · **状态**：in-progress · **读取时机**：审查 enum case 值与真实 place 的调用期借用分类时 · **唯一真源**：本合同；语言规则仍由现行 Guide 定义

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2P3-0320` |
| 所属 Phase | Phase 2 expression/call argument category；Phase 3 调用期 loan；Phase 6 两入口结果 |
| 语言规范 | [构造身份与参数](../../guide/11-copyability-layout-construction.md#可构造目标与唯一身份)、[调用期loan](../../guide/10-ownership-borrowing-drop.md#所有者参数绑定与调用期-loan) |
| 批准依据 | 2026-10-14 用户批准A诊断封闭→B enum构造实参分类→C List MoveOnly追踪；合同与最小实现独审通过；在实际main正常同步后，按逐名授权完成普通源码本地验证；远端CI仍待 |
| 前置 Spec | [SPEC-0319](../../archive/specs/0319-final-type-admission-diagnostics.md)；最终类型准入保持，不混入其PR |
| 分支基线 | `fix/spec-0320` 显式依赖0319本地最终 `6c1a11ca87b642849c6ba7c3be008a7aebece759` / tree `e2d1a5179c7102336154570c95cbd6e6774b5799`；初始依赖；随后保留真实双父正常合入0319实际main `a0f5f8ac46a33dc291b814a8bdeae4dae5eed64b` / tree `d28e30ad91e2f02d6bc04cb7ffb0605e8798630b` |
| 官方核验 | 初次只读fetch main `18865460`时全局最大编号0319，0320按批准保留；本次官方fetch与正常merge确认上述actualmain，未改写已发布历史 |
| 语言语义变更 | 否；既有Guide已规定无payload case为构造值、Borrow temporary合法、Inout temporary非法 |
| 阻塞项 | 最小实现与46唯一普通Rust目标、四工程门禁已在固定输入通过；双宿主物理CI、归档与主干交付待完成 |

## 1. 单一 Goal

已成功选择的 enum case construction 是新值，不能只因其 AST 写作 `Name`/`Member`，
就把它当作可回溯到 binding/field/element 的 place。single 与 unit 必须发布一致的
expression category 和外层调用 argument category，使 Borrow 临时值建立真实调用期 loan，
不在 ownership 抛 `InvalidUnitArgumentPlace`，也不以“无诊断但缺loan”冒充成功。

修复依赖实际 construction expression/target 身份，不依赖 `E.C` 拼写、结果 enum 类型、
case 名称或同类型比较；已绑定 enum local、参数和字段仍是 place。只修构造值角色、对应分类/loan与诊断，
不放宽ownership place查找、不允许借用逃逸，也不新增enum when或泛型enum native能力。

## 2. 原附件与当前静态证据

输入取已核最新95-probe双矩阵包，tar SHA256
`a2eed0228ac3b43aa41eceb17f71f96d173d29560762fcc2867f3031e5df1bd3`。
下表是作者b43增量矩阵，不是本轮6c输入的动态结果；原源码需原字节保留。

| 原probe | SHA256 | 作者single / project | 要分离的根因 |
|---|---|---|---|
| `p49_when_enum_no_payload.ko` | `5679bacd7fe7b110811b816f75352b0bc7538c9c9cbf688e3d483c543fc96cb6` | MissingFact / ownership内部错误 | Borrow实参`Color.Green` `[177,188)`；后续unit enum表达式条件另受限 |
| `q08_enum_string_payload.ko` | `4a9c35ed04b945fd9dc0b144ca8c8e90aa5c0174b194a0d22827fa5fb984f12e` | UnsupportedNode / ownership内部错误 | Borrow实参`Msg.Quit` `[187,195)`；String使root enum为MoveOnly，when另受限 |

实现前6c源码只读疑点（后由§6真实基线分别确认）：

- single `checker/construction.rs::check_bare_enum_construction` 和 unit同名方法已在真实
  Name/Member expression上发布零operand descriptor，结果为root enum类型。
- single `checker/callable.rs::classify_expression_category` 仅先识别constant等，已知Member默认Place；
  unit `bodies/checker/expression_facts.rs::expression_category` 最后落到syntactic-place规则。
  unit的enum短名Symbol也不能当作local binding；分组必须沿内层真实category传播。
- unit `ownership_checking/compilation_unit/dataflow/call_contract.rs::apply_contract` 对SharedLoan
  的Place分支必须找回真实place或temporary projection，否则返回内部错误。single
  `checker/loan.rs` 无place且category非Temporary时可能不发布临时loan，故不能只断言API返回Ok。
- single短名case另有待确认风险：类型category已是Temporary，但ownership引用表收录EnumVariant，
  `place(Name)`可能把它当binding，Borrow又先查place；因此category正确不等于loan target正确。
  必须用enum体内短名普通源码单独核真实construction与Temporary target，不把它归为同一个Member红测。
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
   payload copy/move与普通constant category保持。不新建诊断码，不改变raw recovery接口与失败表示或type trial回滚。
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

此外，既有`ssa::lower_frontend_tests::return_control_tests::direct_enum_case_call_argument_remains_a_precise_missing_fact`
使用同Goal源码`take(Shape.Point)`并锁MissingFact(Shape.Point)，属于待修bug的旧oracle，不是when保留边界。
先保留其真实baseline，再以同源码将它迁移为expected-success的临时loan/verified SSA合同；
不得删除该测试、漏CI或继续把旧失败断言算修复成功。迁移和两个when拒例分别记账。

## 5. 拟精确红测与安全边界

以下7个tests-only目标已落地并完成helper独审，按固定名单实跑为行为红；另3个既有codegen
拒绝合同保持绿，见下节。源manifest与预期随输入冻结，新增目标仍须逐名审查授权；普通源码只读facts，不构造伪descriptor/IR或强制observer失败。

| tests-only target / 精确函数 | 要锁的合同与helper |
|---|---|
| frontend `enum_case_borrow_categories` / `selected_case_values_are_temporary_but_bindings_and_fields_are_places` | 普通lex/parse→single/unit names/types；先采raw、不以前置final成功为条件；零operand EnumCase descriptor、root类型、表达式与外层Borrow实参category；直接/Group/短名及place反例 |
| 同target / `selected_case_qualifiers_keep_final_admission_precise` | 独立调用0319 final，采真实结果；已选case与Package/Type qualifier、runtime同名String/field对照，防全链或同类型豁免；与raw分类红分账 |
| 同target / `borrowed_case_values_publish_call_scoped_temporary_loans` | 原check_ownership/check_compilation_unit_ownership；Copyable与MoveOnly空case、payload对照；精确临时loan、call Span/结束点及唯一owner消费，短名case单独断言Temporary而非EnumVariant假Place，不只看无诊断 |
| 同target / `cross_file_case_aliases_preserve_source_qualified_identity` | 两文件普通unit、真实正逆inputs、qualified/import alias、basic/constant；不按同类型合并目标 |
| 同target / `inout_case_temporaries_keep_l0122_and_mutable_places_stay_valid` | 普通非法`&E.C`/Group与合法mutable local；精确诊断码、marker与parameter label，成功case不得提前错误 |
| CLI `native_cli` / `enum_case_borrow::single_direct_case_borrow_builds_and_runs` | 现有run→原版kovenc，固定无when正例，build/run原输出、退出与产物 |
| CLI `project_cli` / `enum_case_borrow::project_direct_case_borrow_builds_and_runs` | 原TestProject/project_build/run→kovenc，同源码及单独constant路径；不改IR或runtime |

既有边界与共享回归另取精确安全名单：source construction的parameter mapping/expected inference、
ownership construction临时owner与drop、字段/元素place、原两个unit when拒例及0319 final保护。
必须静读每个实际helper后再运行；不继承0319的81计数、不跑suite/discover或云故障、counted、
sanitizer、calibration、stage/aggregate。双宿主CI继续按真实工作流验收，新增target须显式接线。

## 6. 真实基线（未修复）

固定`fc68e0e8332cb7014b99a9b44fbd7d2f82a3b79d` / tree`cf94e411`，生产与实际main `a0f5f8ac`相同。
Rust/Cargo1.96.0、LLVM/Clang21.1.8；独占绝对target、`--locked`及完整`--exact`串行10项，
7行为红/3既有拒绝合同绿，无编译接线、源码前置或零命中失败。1433源文件前后NUL摘要均为
`ba706c021df158f6df2522c46636cb5235286a3131de9af62d0748d725edc1c2`。

| 分层 | 真实结果与断言范围 |
|---|---|
| raw category | 首断言unit短名Borrow为Place而非Temporary；跨文件qualified同样Place；single短名category虽Temporary，真实loan却为EnumVariant的Place |
| final资格 | 正例`Flag.On`的静态`Flag` `[87,91)`在single/unit分别为Error根因缺失/表达式类型缺失Internal；它们由独立结果采集证明，不能冒称已执行全部positive oracle |
| 原L0144保护 | `Maybe.None`在raw已有`None` `[86,90)` L0144；final反而在静态`Maybe` `[80,85)`报Internal，成为该目标首个失败；需按真实构造根因窄恢复，不能全局压制Internal |
| Inout | `&Flag.On`首断言raw诊断为空，未产生预期L0122；mutable对照已采结果，后续oracle未执行 |
| CLI2 | single/basic/constant的build/run共6进程均已采；首失败是上述final缺事实，single exit2、project exit1，constant偏移为`Flag` `[125,129)`；0产物运行 |
| 旧codegen3 | 同源码`take(Shape.Point)` MissingFact与两个when UnsupportedNode各1/1绿；前者只是旧bug拒绝，尚未迁成功合同，后两项不扩能力 |

FE共32组源码分析（28single+32unit）均完成分采；每目标首个断言后的oracle未执行，不能把采集数当通过数。
原始10日志、完整argv/结果/manifest与收据保留仓外`spec0320-tests-evidence/baseline1/`，不把作者矩阵倒写成本轮结果。
同qualifier目标随后两次观察复跑仍为1/1红（唯一目标数仍10）：确认single失败Member保留case.root_type模板、unit为Error，两者无descriptor。
`907974a8`加入同source独立`n.fooBar`及同名String receiver对照，9组双入口已采、raw表示断言已过；原L0144 final先失败，混合三诊断oracle未执行。

## 7. 最小实现与本地验证

固定实现/测试输入`e26e203cc2a05de4f9818ec3417d9daf12003cec` / tree`6f974652`，
1434源文件四批运行及工程门禁前后NUL摘要均为
`29bdf70c5db671726493e0da5d255f8cba015241eb777c4300473cdd024d9fa6`。

- 两入口只把同expression的真实EnumCase descriptor归为Temporary；single place查询排除同一构造，
  Group沿原传播，真实binding/field保持Place。已有Borrow loan与CallReturn清理复用，Inout沿既有L0122拒绝。
- final仅为真实EnumCase选择开放静态qualifier；失败bare case须本Member的原typed L0144、
  精确name_span、真实case Value引用、零payload及已观测raw表示共同成立。unit再锁同source/Symbol；
  原失败保持不完整，独立`fooBar`与同名runtime receiver仍各自诊断。
- category方法完整迁至专责文件，旧分支原字节保留后作上述窄修；未提高尺寸cap，未改变when或Guide。

| 验证 | 真实结果 |
|---|---|
| 新FE5 | 33组（29single+33unit）全部oracle通过；含MoveOnly Group内construction/外operand的loan与drop、精确place/case身份、L0144混合独立根及Inout对照 |
| 新CLI2 | single/basic/constant共6次kovenc build/run及3次普通产物运行通过，输出`enum-borrow`；原基线0产物运行记录不变 |
| codegen3 | 原`take(Shape.Point)`同源码已迁`direct_enum_case_call_argument_has_temporary_loan_and_verified_ssa`并通过真实Temporary loan、CallReturn与原SSA/LLVM验证；两个unit when拒绝边界保持 |
| 共享36 | 0319 final13、原CLI3（48诊断进程输出）、原enum native2、原邻项SSA2、构造/loan16逐exact原oracle通过；与首10零交集，总46唯一＝FE34、公开CLI5、SSA5、原版native2 |
| 工程/静态 | fmt；workspace all-targets check；frontend/codegen/CLI all-targets strict clippy；CLI build全部exit0。纯只读CI选择Python exact1/1、Rust尺寸与diff检查通过；不以编译all-targets声称测试全量执行 |

新增FE target显式进入双宿主stage；静态完整选择104→105唯一frontend integration，唯一增量为
`enum_case_borrow_categories`，Cargo调用仍10。Composition故障/聚合合同只交远端CI，本轮云端未执行；
脚本进入代码CI指纹，未声称docs-only复用。原始红/观察/绿日志、固定名单与完整argv保留仓外
`spec0320-tests-evidence/`，本片仍active；本地通过不能替代双宿主新增FE5实际执行与完整CI验收。

## 8. 状态与停止条件

- [x] 官方main/编号静核，保留0320；新worktree基6c，0319依赖显式记录。
- [x] Guide构造身份、Borrow temporary与Inout限制核对；原两probe字节与作者矩阵分账。
- [x] 草案独审，批准迁active并准备tests-only。
- [x] 实际main正常merge与tests-only固定；该阶段未改生产。
- [x] frontend5、CLI2与旧codegen3安全逐名独审，固定10 exact真实基线完成。
- [x] 真实expected-positive及诊断保护红测，前置final/短名loan根因分账。
- [x] 最小分类/loan/构造qualifier修复及不误杀、旧oracle迁移和原when边界回归。
- [x] 本地46唯一普通Rust目标、四工程门禁与Architecture事实同步。
- [ ] 双宿主物理CI、验收归档及主干交付。

若必须修改Guide、重写通用place解析、放宽所有权或扩展when才能达成目标，停止并报告。
任何fixture前置失败、已绿对照、未执行后续oracle与真正行为红分账；C保留原“List MoveOnly追踪”授权名，当前待查为p38的`MutableList<String>.add`后`for`临时owner清理，独立于本片，
不得借本片编号或后端拒例夹入实现。
