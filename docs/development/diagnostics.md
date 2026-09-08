# 诊断规范

> **性质**：工程规则 · **状态**：current · **读取时机**：修改错误码、Diagnostic、Span、renderer 或机器输出时 · **唯一真源**：本页与 frontend 诊断注册表

- 稳定错误码格式为 `L` 加四位数字，并在 `lang-frontend` 集中注册；不得散落硬编码。
- 一个错误码只表达一类稳定语义。改变已发布含义必须同步规范和变更记录，不能复用编号。
- 每条用户诊断至少包含错误码、主消息和主 `Span`；必要时增加关联位置、说明和可操作建议。
- 行列、Unicode 和多行范围统一由 Source/Span 基础设施计算。
- 错误恢复不得伪造后续语义；级联诊断受控且顺序确定。
- 机器诊断遵循 accepted ADR 的 schema v1 JSON Lines；未批准的 build event、颜色或公共字段不得扩张。
- 正常用户错误不是运行日志；内部错误与用户诊断必须区分。

测试至少断言错误码和关键 Span；涉及顺序、恢复或机器输出时还要断言完整确定性。诊断目录的当前
实现结构见 [Architecture](../architecture/diagnostics-and-tests.md)。
