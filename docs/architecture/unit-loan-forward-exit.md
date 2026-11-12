# Unit wrapper 的正常 live 出口

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 wrapper 出口覆盖证明时 · **唯一真源**：`loan_forward_regions.rs` 与 `drop_planner/action_source/forward_regions.rs`

## 有界合同

单表达式体、唯一 Shared String 参数的直接声明调用可发布独立 `forward_regions()` 区域。
真实入口、`drop_all` 前全部义务、实际 handoff 句柄、退出后状态与完整未过滤 Begin→Handoff
区间共同认证局部 Complete；不以清表后的空状态替代义务证明。Group/Block 等旧形状仍不认证。

source/callable 仍 Partial，live 交付不是 End 或权限恢复，不新增清理责任或旧 oracle 退休。

## 真实生产 receipt

`refine/forward_receipt` 对上述 Complete 区域验证同 catalog Begin/Handoff、实际来源参数及
callee/caller 映射。原 BorrowCall producer 印记贯穿真实 SSA append，BorrowReturn 在实际
set_terminator 后绑定出口 block 与返回 LoanId；最终复核唯一 call/return，不扫描 SSA 补造身份。
entry/source/result 保持当前函数本地身份，不生成额外 Begin/End/Drop。无区域及 observer-off
不启用新验证；已准入后缺失、重复或身份破损是错误，不静默回退。
该区域复用[实际动作源](unit-loan-action-source.md)，旧 raw/off/Eq 投影不变；失败或 deferred 撤销新旧产品。
验收与未覆盖范围见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#单wrapper正常liveforward出口覆盖合同2026-11-11)。

## 有界多继续出口的前端集合证明

独立多继续出口产品将多个真实隐式false入口与唯一公共tail交易分开保存。深度≤3、
结构叶≤8、同根String及独立Boolean参数内，每次真实join消费两侧closed/live token，
核来源互斥与精确并集；原merge输入句柄在合流后再次匹配，根须覆盖全部hole。
公共tail只引用一对真实Begin/Handoff，不按到达路径复制交易；旧single/terminal协议
独立保留。该产品仅表示局部前端Complete，catalog/source仍Partial。独立后端
multi receipt保留真实Goto与多前驱join，只按实际单前驱fold重放canonical映射；
每个来源逐入边核source block parameter，均到唯一tail call/result/return。原final
verifier成功后原子发布，失败撤销本批pending；observer-off产物保持一致。实际边界见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#多继续出口的真实后端收据实施中)。

## 独立双根辅助退休关系
唯一source与独立aux owner的真实pending acquisition/committed handoff另有独立关系；
支持typed source参数0/1，不扩大旧同根资格。实际OwnedValue身份锚与独立owner事件窗口
证明source原实例存活；aux可仍live，或由同次CallReturn的唯一真实owner Drop和raw
cleanup共同确认ASAP退休。旧owner Unsupported/目录清空不变，新窗口不授予旧资格。
关系可定位已发生的aux Drop，但不是整生命周期Complete或新receipt；source/result及
两owner后续结束仍靠旧见证，证据见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#独立双根辅助退休关系实施中)。

ASAP四语句双根形状另发布独立Complete区域：binding入口承接两真实owner，完整9动作
覆盖aux Drop与source退休后的实际source Drop，独立removed-owner cause及两Drop事件
窗口与真实空义务出口共同验证。辅助根后续另行消费仍不获此资格。独立双根receipt
消费此区域，实际11角色核两RootPlace、source/aux End与两Drop，role0冻结两owner、
全7实体身份不变；原verifier成功后原子发布。细节见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#双根asap区域的独立真实后端收据实施中)。

独立LaterUse五语句区域覆盖结果消费后再消费aux：source参数0/1，完整11动作含
source退休/Drop及新aux Begin、原EndCallLoan、aux Drop。真实pending移除时保存
acquisition、活owner及返回cursor，再经独立cause核实际removed owner和Drop，
不靠Begin+Drop倒推结束；真实入口义务、出口清空和两owner事件共同决定Complete。
旧ASAP区域不扩展；独立前端边界及验证见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#双根辅助owner后续使用的独立区域实施中)。

LaterUse独立receipt再核同轮11个action IDs与实际15连续SSA角色/9实体。aux两次
RootPlace/Loan各有身份，首次aux End与后续End不互代；source Drop完成后方准入
后续operand，aux Drop必须在新loan End后。两owner在role0冻结，后续append/alias
均复核；仍经原final verifier后发布，失败撤销pending，不退休旧见证。见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#后续辅助借用的独立真实收据实施中)。

BA^n动态后缀另有独立`auxiliary_sequence_regions()`证书，局部预算2≤n≤8，不限制
旧源码执行。每个use含实际pending返回对应的Begin/End ID与活aux OwnerId；完整
8条前缀加n对Begin/End及最终Drop逐项核验，最后真实返回后才连接removed owner。
私有Single/Sequence共用准入和因果，不将Sequence放入旧LaterUse getter；旧n1仍
严格11动作。预算外或多B/前置A不借用资格。前端边界见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#动态ba序列的独立前端证书实施中)。

动态BA序列独立CG receipt按use Vec验证11+4n个真实角色和7+2n实体，不逐n枚举。
每次实际operand授权对应同aux owner的新Place/Loan，前轮End后进入下一轮；source
Drop固定先于序列，aux Drop仅最后一轮End之后。完整9+2n前端动作与逐use Begin/
End/owner绑定，role0双owner冻结，后续append/alias维持全部身份；原verifier后
原子发布，失败回滚，旧见证保留。见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#动态ba序列的真实后端收据实施中)。

前置A的单B链另发布独立`interleaved_auxiliary_regions()`，总A1..8、B唯一。
真实A返回前B时保存活source owner与原Unused binding transport的Begin/Handoff锚；
后B时保存source实际retirement/Drop且确认binding义务已移除。按原声明事件顺序核
全部9+2n记录，覆盖AB及aux先Drop的反转次序，不归一化为旧sourceDrop前缀。
公开产品与Single/Sequence分流；对应独立CG receipt按B位置核11+4n连续真实角色
和7+2n实体，逐A新Place/Loan，双owner入口冻结且前B保留source/binding义务。
最后A后的aux Drop可先于B，动态角色严格对应两种次序；原verifier后原子发布，
旧见证保留。
细节见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#单b前后辅助消费链的独立前端证书实施中)。

后端实际身份与边界证据见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#单b前后辅助消费链的真实后端收据实施中)。

多B另有独立`binding_use_sets()` Partial证书：严格直线A1..8/B2..8，声明的所有B
与实际pending loan正常移除一一对应，原acquisition及return cursor随独立transport
lineage保存，真实BindingEnd关闭；merge即失效。cursor仅原记录边界，不伪造End。
旧Unique使用/退休资格不变，不证明source Drop或完整region，暂无对应新CG receipt。
详见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#多次binding使用集合的独立证书实施中)。

多B另发布`binding_use_set_retirements()` Partial：真实transport移除后，以原
BindingEnd关闭的集合token经独立cause匹配完整removed source快照及原named Drop。
新Drop证据失败不撤销既有use-set，旧cause优先；尚不证明aux全生命周期或完整region。
见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#多b集合退休与实际source-drop的独立证据实施中)。

多B直线另发布独立`multi_binding_regions()` Complete：沿真实全事件序列核每A
发生时已返回B数，同一transport进度及活source快照，或末B后的已证source Drop。
末A的原End经独立cause关联实际removed aux与原Drop；全7+B+2A记录、两owner
事件及真实空义务出口共同闭合，旧Partial不受新失败污染。
见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#多b动态辅助进度与完整区域实施中)。

多B独立动态CG receipt按原A/B序列核10+4A+B真实角色、7+2A实体。每B call均
共用原result loan，只有末B结束result/source并Drop；A各有fresh Place/Loan，
末A才Drop aux。完整FE区间/集合/退休、typed/sourcePlan及两raw Drop独立准入，
两owner入口冻结，append/alias逐项核验，原verifier后原子发布；旧见证保留。见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#多b完整区域的真实动态后端收据实施中)。

循环边界实测区分三层：循环内`borrow val`七形状（含立即break、return、false、
嵌套）typed成功但ownership L0164；循环外binding在while使用等五邻界已通过
ownership，却在CG为UnsupportedNode、原verifier未运行。循环内临时borrow call
也被ownership拒绝；普通`consume(source)`循环则有真实SSA/LLVM与verifier成功。
这些是现边界，不代表动态借用证明完成。见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#循环借用l0164的合同与安全邻界研究实施中)。

具名owner销毁由`drop_planner/owner_cleanup.rs`负责：保护检查、实际移除、capture
逆序清理、owner事件及一次性cause验证，随后规范化、去重并有序发布Drop事实。
退出点选择、scope/branch/loop策略、receiver清理及liveness仍在原planner，模块
拆分不改变任何执行或观察次序，也不扩大证书边界。

后续普通while已支持一个窄合同：同父直线借用块的最后简单调用拥有唯一真实
BindingEnd，且所有binding分别准入；while入口复核实际result/source义务已移除。
Borrow参数原loan继续存活，owned source的新loan则在resultEnd后结束；不支持
活loan跨循环或循环内借用，L0164保留。见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#闭合借用词法块之后的普通while实施中)。

最小外层不变量while另允许：一次直接String借用返回acquire，body仅一次直接
同步借用消费；唯一真实End在包围[binding,while]块。实际entry/header/回边/false/
exit维持同一lease，SSA按边参数位置核source/result/owner来源；结束仅在出口，
own source为resultEnd→sourceEnd→ownerDrop，Borrow参数不伪End。仍拒循环内
acquire、jump、嵌套和多consumer，不新增通用循环receipt。见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#外层不变量借用while合同实施中)。

上述不变量while现推广为非空动态直线consumer序列：每个调用独立满足原身份与
同步Borrow合同，actual SSA逐call保持同一result loan并核callee顺序。无固定n
枚举、无每call acquire；混root、空body、嵌套及jump仍拒，其他循环边界不变。

不变量consumer序列尾现可接无条件continue：独立精确tail/while/frame检查包围
原jump清理与Goto，核回到原header且lease不变；SSA证明确为continue而非额外
normal回边。条件/嵌套continue、空consumer、break/return仍不在本合同内。

尾部无条件break亦按独立JumpKind核frame.exit而非header，原ControlTransfer与
LoopExit清理不绕过。实际无回边CFG中entry/header可折叠，break和false边携同一
lease到共同exit，随后才End/Drop；条件break及return仍未纳入合同。

尾部条件break现支持精确无else的`if(BooleanName){break}`，then仅break；true
实际退出、false正常回边，分别核原清理后lease并在共同exit结束。内外条件可同
symbol，独立资格只授予该binding的尾If，不泛放其他控制流。

尾裸Unit return另用真实双End合同：精确Return的CT End和外围Block End，return
实际清理完整区间与终结点由局部checkpoint核验；false仍依原sync_dead独立入口
恢复活lease。两路径分别正确End/Drop，不把Diverged当作跳过检查的许可。

尾条件continue复用精确ConditionalJump的显式kind，true实际Continue与false
正常回边都到原header。最终SSA实测保留entry加两回边三前驱，逐source/result
与条件参数身份核验；没有伪造折叠前边或新增独立循环receipt。

独立FE `loop_invariant_lineages()`首片仅为本地字面量String owner、无jump普通
while发布Partial关系：实际入口/被遍历body回交/正常退出/原BindingEnd及每consumer
真实pending移除回交。Borrow/own参数旧记录没有Handoff，不伪造身份；常量、jump
等新资格为空且旧行为保留。它不是循环每轮完整证明，也不是CG receipt；local
三语句形状的后端资格仍需另接，不能以该关系自动放行。

对应局部三语句的CG执行准入不依赖observer；独立loop receipt另限单Borrow Boolean
参数及普通直接callee。实际producer记录literal→binding唯一Alias、source/result槽、
条件和consumer、正常回边/false出口、最终End/Drop；实际fold仅合并false→exit。
emitter固定真实定义与各block参数，逐边按slot身份核SSA运输，原verifier成功后才发布。
owner模式条件或额外参数仍走原执行但无新receipt；此处仍引用FE Partial，不证明
循环内新借用、任意迭代完整性、跳转、多根循环或终止性，也不退休旧见证。

loop receipt另提供不依赖Plans/Slot/current的有限图复核：保留真实原边、fold和
Alias证据，核完整Function块/指令/定义元数据与可达集合；从真实entry建立点传播
owner/source/result/flag角色至固定点，并逐incoming及每块实际实体核保持关系。
它封闭CG运输证书的漏覆盖边界，不重新验证FE catalog，不替代原SSA verifier，也
不将一次planner遍历升级为完整FE循环生命周期；每轮fresh借用仍受L0164保护。

no-carry研究对照分别运行file-local与compilation-unit两套checker；普通Block
有同binding的唯一末consumer AfterStatement End，而相同body放入dynamic while
（包括内层lexical Block）仍精确L0164，未伪造binding/End。这只验证关闭原语与
门禁一致性，不证明回边无实例残留，也不启用每轮fresh借用。

显式test-support另提供普通Block frame研究capture：两core checker真实acquire、
最后使用移除的前后义务及原End，在任何scope cleanup前按独占frame身份对证；
正常非空闭合还要求外部义务投影不变（仅去无资源的空closure_results缓存）。
限定直接Name/String借用initializer与Unit消费者、无复杂捕获入口。End后合法
move源仍保持原执行，但不获恢复认证；未capture不复制状态，默认产品无此资格。
这只是普通scope的安全基础，循环实例、回边和CG每轮receipt仍未接入。

已有非空外层carried loan另有test-support回边观察：条件后body-entry至合流前
normal/尾continue分别核完整资源投影保持，break单列退出且不认证。仅动态Boolean
参数及无声明/嵌套/分支的直线Unit消费者；支持多个既有来源但不形成fresh循环实例。
观察无副作用，不替代逐consumer return、任意控制流归纳或后端每轮receipt。
