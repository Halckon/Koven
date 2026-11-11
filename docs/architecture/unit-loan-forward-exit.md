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
