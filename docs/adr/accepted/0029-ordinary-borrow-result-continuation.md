# ADR-0029：普通只读借用结果的来源与跨调用延续

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：实现普通借用结果、caller continuation 或 scoped Map 访问时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据与边界

2026-10-07 12:24 UTC，用户明确批准普通借用返回、显式 borrow val 和受检查的作用域访问。
语言唯一真源为 [Guide v0.42](../../guide/README.md)；本记录局部扩展
[ADR-0016](0016-interprocedural-borrow-abi.md)的结果身份和 call-return loan 终止规则。
既有 Value/Borrow/Inout 参数 ABI、owner ABI、非结果来源的同步 loan 边界保持。
不启用条件借用、一等引用、任意多来源、inout 返回／局部或未来语法。

## 决策

frontend callable 和实际调用 descriptor 运输独立的普通结果 mode 及唯一来源 identity，
目标 T 仍是既有对象类型。所有权阶段验证实际返回 origin，并发布 result binding、
caller source continuation 与明确的终止边。单文件与 compilation unit 使用相同合同；
缺失事实必须结构化拒绝，lowering 不从 AST 或 API 名称猜测来源。

SSA callable 的普通结果是 Shared Loan，携带签名中唯一非 owning entry 参数来源。
结果实体不得冒充 Value；普通 return 与 borrowed return 分开验证。
call-result loan 的 alias roots 与 source operand 相连，source parent 在结果／子 loan 存活时
不能结束、move、drop 或 mutate。callee 必须证明返回存储源自声明的 entry source；
仅声明 from 或返回某个非空地址不能替代证明。嵌套包装调用继续运输同一 origin。

LLVM 返回指向真实 target storage 的 pointer，复用 ADR-0016 的 Borrow 参数地址 ABI；
不复制或 retain payload，不在 callee stack 中创建逃逸临时 storage。
Map 确定借用返回真实 occupied slot 的 V storage，其保守 origin 是整个 owning Map，
不以 runtime key 相等推断不重叠。scoped callback 由 typed 访问事实同步调度，loan 不逃逸；
callback 的正常出口先结束 element/result loan，再结束 Map source loan并恢复权限。

nullable V 是确定存在槽位中的 nullable payload，非 nullable loan；Missing 走独立控制流。
Map 的已有按值 Copyable 查询和 owned remove 不复用此 Shared Loan 结果身份。
普通 aggregate construct/explode、owned call/return、存储和逃逸 capture 不能绕过身份检查。

受限 last-use 需要 loan 自身的来源、别名/子 loan 和未来使用事实；owner ASAP 只决定 drop，
不能据其推定 loan 结束。不能闭合的复杂路径返回明确诊断，不增加隐式 clone 或宽松 verifier。

## 验证与后果

验收同时检查前端诊断/Span、SSA origin/continuation/终止证明与正常 native 行为，
包括错误来源、owner 移动/变异、嵌套包装、闭包逃逸、callback 退出后恢复权限及 nullable V。
故障 IR 校准/注入不属于本次授权，不执行也不由聚合脚本间接触发。
实施和实际结果由 [SPEC-0288](../../specs/active/0288-map-native-execution.md)记录。

取代关系：局部扩展 ADR-0016 的“所有返回 owned、所有 call loan 返回即结束”部分；
其参数及同步借用 ABI 决策继续生效。当前 accepted 不表示实现已经完成。
