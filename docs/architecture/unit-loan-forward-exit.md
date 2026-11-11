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
