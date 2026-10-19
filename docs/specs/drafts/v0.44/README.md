# v0.44 实施草案路由

> **性质**：版本阶段索引 · **状态**：draft · **读取时机**：查找已启用合同对应的待重基实施草案时 · **唯一真源**：Guide 与各 Spec

[Guide v0.44](../../../guide/README.md)已启用选定的类型、控制流、Range、转换与 Map 合同，
不表示实现完成。[SPEC-0291](../../active/0291-readonly-map-construction.md)已从真实 main
`37e80526` 重基为 active，落实 owned 本地 receiver、插入序共享布局与资源验收；旧历史
候选通过固定提交追溯。本目录保留[0334 String? ABI](0334-nullable-string-map-abi.md)及[0335 caller证明](0335-map-borrow-caller-proof.md)两份设计前置draft；均未实施，不授权其他consume家族或复制转换。
[SPEC-0325](../../../archive/specs/0325-simple-variable-assignment-types.md)已有限归档并由PR111合入main `aa42bdc5`。
[SPEC-0326](../../../archive/specs/0326-closed-control-branch-join.md)已按合B后的联合CI双host39exact有界归档；
其余v0.44范围不因此获得实现完成状态。

## 已认领的终止性后继

0326作者（前端维护责任）继续跟踪变量初始化的falls-through传递，排序由项目负责人协调；
[Int函数中val初始化全分支return的真实源码与L0088入口差异](../../../archive/specs/0326-closed-control-branch-join.md#5-已认领的后继终止性待办)仍开放。
本片join根已为Nothing不代表声明/函数终止性全部一致；尚未另分Spec或宣称修复。

[SPEC-0328](../../../archive/specs/0328-integer-range-values.md)已按PR114实现CI双host36exact有界归档；
数值 for 值 provider 已由[SPEC-0329](../../../archive/specs/0329-integer-range-iteration.md)按双host37exact实现CI有界归档；归档head CI/main另验。
一般Range?参数/局部null与requireValue借用绑定函数含Binary的旧控制流边界仍由该作者跟踪；
MapResult实际查询与显式borrow复制已验不代表这些一般边界已完成。
[SPEC-0327](../../../archive/specs/0327-checked-integer-conversions.md)已按双host30exact实施CI有界归档；
八整数转换前端至native及CI-only负控已验，归档head CI/main待验，其余v0.44合同不随之完成。

[SPEC-0330](../../../archive/specs/0330-while-conservative-flow.md)已按 PR116 修复实现 CI 双 host129 exact 有界归档；已由PR116正常合并为真实 main `37e80526`，main CI独立跟踪；0291现按该基线启动。

## 已认领的 Rc 后端边界

[SPEC-0332](../../../archive/specs/0332-rc-nullable-while-regression.md#5-额外探索输入的实现边界)
只关闭PR116引入的SharedOwner错误专用分派。0332作者/后端维护责任继续跟踪unit nullable Rc
payload投影与while体内原owner重复nullable包装；原四探索源及精确4+4阶段表保留，后继修复
须以真实成功身份/清理证据更新该表，不将InvalidSsa视为语言限制。旧break/return整fixture
在PR116前已失败，但旧内部根因未逐诊断；不冒称这两缺口都由PR116引入。排序由项目负责人协调。
