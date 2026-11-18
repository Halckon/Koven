# 所有权生命周期审计边界

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改默认关闭的生命周期审计时 · **唯一真源**：代码、定向测试与 SPEC-0341 验收记录 · [细化层入口](ownership-refinement.md)

默认关闭的 D10 审计另消费同 catalog 入口 transport 旁证，只在三至五语句 String owner /borrow binding / 一或两个顺序 Unit consumer、唯一 Boolean 参数与原 AfterStatement 正常退休域，将实际 commit/use、binding/source map removal、原 Drop 和机械 Slot→SSA 身份连接。
最终 verifier 后核完整单块 caller 与全部实体、正常 Return 才消除该 lifetime gap；逐次保留原 call/loan/Begin 身份；两次消费间可跨一个独立不可变 Int literal 定义，原声明/值经真实 Constant/Alias Slot 映射核对。第三次、两个标量、var、非 literal 等未证域
仍保 gap；显式 return/条件消费保留原 Unsupported，不扩大生产准入。D10 主图还复用原 live-forward region/producer receipt，核 planned callable、同 catalogBegin/Handoff、最终完整单块 BorrowCall/BorrowReturn 与 StringOwner 两 loan 定义，认证
forward→callee 边和正常借用退出；仅原 expression-forward 域，不从最终形状放行 block 返回。原 Complete binding region 的零参三语句入口另复用七动作/七指令生产证据，连同实际literal 字节/owner 核完整八指令 caller、两 callee 映射和退休/Drop，再认证来源调用边；
额外 consumer 不按七指令窗口放行，不替代前述 Boolean 入口证明。原 Partial loop lineage 可另连同原 loop receipt 核完整有限 SSA CFG、回边参数传递与正常退出 cleanup，认证真实来源 BorrowCall；原 literal 字节、String/Boolean typed→SSA
身份及各 callee 映射独立核对。`certified_loop_callers` 不提升 coverage，项目 unsupported仍明确保留 Partial 的一般 fixed-point、任意迭代与终止未证明义务；owned flag 等无证域不认证。原 auxiliary-binding Complete region 的同 owner 两个独立借用可另消费原十角色证据：
非返回源 aux 在 BorrowCall 后先 End，返回源/结果留至 consumer 后退休和 Drop；source 参数前/后顺序均保原 chronology。最终核 literal+十指令全 caller、六实体及原retirement 三 ID 关系（关系本身不是 End 事件）。identity 叶证书限一或两个 Borrow String
参数的直接 Name 返回，按原 contract 源 index 核全部独立 Shared loan/参数定义及正常退出；三参数、混类型、group/block 等域不据最终形状放行，不改变原准入。完整 aux 生命周期证书可在同次 final audit 以私有 token 精确履行同一原 handoff 的 entry 义务；全 fact/
callable/映射/实际指令核对且无竞争才消费，缺失、重复、域外与未消费证明仍保 gap。不同 root 的 Complete aux 区间另复用原九动作/十一角色：aux End 后立即 Drop(aux)，source/result 留至 consumer 后退休。双原 literal→两个独立 owner 与 typed 映射分别核对，
最终完整十三指令/七实体才认证来源边；distinct 自有私有 token 可履行同一 entry 义务，与 same-root token 合并检查唯一消费/竞争；later-use 不获 eager-drop 证。独立 later-use Complete 域复用原十一动作/十五角色：aux 初借结束但 owner 保活，
source 清理后才新建 aux place/loan、独立消费并 End/Drop；完整十七指令/九实体与三个 dispatch、四 Begin 来源逐项核对；全证后自有 token 可精确履行原 entry 义务，统一三路唯一消费/竞争检查，第二 later-use 无证仍保 gap。interleaved 的 AB 窄桥含一个 BeforeBinding 辅助再借用：返回借用仍活时先 End/Drop(aux)，
再消费结果并退休来源；原 prefix=1、原因果状态与全十七指令/九实体共同核对。全证后自有私有 token 可精确履行原 entry handoff；四路证明统一检测竞争，图边仅发布一次。新增 ABA 精确 count=2/prefix=1，原 BeforeBinding/AfterBinding 引用跨来源退休边界，
完整二十一指令/十一实体及四个 dispatch 对账。AAB 精确 count=2/prefix=2 则两次BeforeBinding 独立 End 后先 Drop(aux)，再 B/来源退休；原 MAX8 不变，AAAB/ABAA 等仍保 gap。具体边界及实际验收见 [D10 生命周期消费](../specs/active/0341-shared-lowering-core.md#d10-有界入口生命周期实际消费2026-11-15)。
