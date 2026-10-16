# v0.44 实施草案路由

> **性质**：版本阶段索引 · **状态**：draft · **读取时机**：查找已启用合同对应的待重基实施草案时 · **唯一真源**：Guide 与各 Spec

[Guide v0.44](../../../guide/README.md)已启用选定的类型、控制流、Range、转换与 Map 合同，
不表示实现完成。[SPEC-0291](0291-readonly-map-construction.md)保留历史候选正文，须按
新的 owned 本地 receiver、插入序共享布局与资源验收重基后再申请 active；旧“不改变布局”
假设不能约束新的内部方案。此目录不授权其他 consume 家族或复制转换。
[SPEC-0325](../../../archive/specs/0325-simple-variable-assignment-types.md)已有限归档并由PR111合入main `aa42bdc5`。
[SPEC-0326](../../../archive/specs/0326-closed-control-branch-join.md)已按合B后的联合CI双host39exact有界归档；
其余v0.44范围不因此获得实现完成状态。

## 已认领的终止性后继

0326作者（前端维护责任）继续跟踪变量初始化的falls-through传递，排序由项目负责人协调；
[Int函数中val初始化全分支return的真实源码与L0088入口差异](../../../archive/specs/0326-closed-control-branch-join.md#5-已认领的后继终止性待办)仍开放。
本片join根已为Nothing不代表声明/函数终止性全部一致；尚未另分Spec或宣称修复。

[SPEC-0328](0328-integer-range-values.md)仅准备整数 Range 普通值首片；构造、包含性与序列相等
尚未实施，数值 for 值 provider 由该 Spec 作者认领为独立后继，不以首片代替完整 D。
