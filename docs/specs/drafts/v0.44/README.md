# v0.44 实施草案路由

> **性质**：版本阶段索引 · **状态**：draft · **读取时机**：查找已启用合同对应的待重基实施草案时 · **唯一真源**：Guide 与各 Spec

[Guide v0.44](../../../guide/README.md)已启用选定的类型、控制流、Range、转换与 Map 合同，
不表示实现完成。[SPEC-0291](0291-readonly-map-construction.md)保留历史候选正文，须按
新的 owned 本地 receiver、插入序共享布局与资源验收重基后再申请 active；旧“不改变布局”
假设不能约束新的内部方案。此目录不授权其他 consume 家族或复制转换。
0325 的静态测试准备尚在独立未发布分支，不由本次版本切换冒称已测试或已实施。

[SPEC-0326](0326-closed-control-branch-join.md)认领已启用 A 的封闭 if/when join；
当前仅静态计划，须固定普通源码 helper/exact 名单独审后采红，不声称实现完成。
