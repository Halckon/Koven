# ADR-0001: 使用 ADR 记录架构决策

## 状态

accepted

## 背景

Koven 是一个分阶段实现的 AOT 编译器。workspace 边界、IR、LLVM 接入、runtime / ABI、
bootstrap 和平台支持等选择会跨越多个功能与 Phase，长期影响代码结构。若这些选择只存在于
聊天、提交或最终代码中，后来者只能看到结果，无法判断约束、代价和曾被放弃的方案。

Spec 适合定义一次变更“做什么”，architecture 适合描述仓库“现在是什么样”，二者都不
适合保存一次长期技术选择的原始理由。

## 决策

采用轻量级 Architecture Decision Record：

- ADR 存放于 `docs/adr/`，使用四位递增编号；
- 架构变更先提出并接受 ADR，再批准关联 Spec 并进入实现；
- ADR 至少记录状态、背景、决策、替代方案以及收益与代价；
- 已接受 ADR 不改写历史。改变决策时新增 ADR，并将旧记录标记为 `superseded`。

具体格式与状态规则见 [`TEMPLATE.md`](./TEMPLATE.md) 和 [`../AGENTS.md`](../AGENTS.md)。

## 替代方案

### 只在 Spec 中记录

拒绝。Spec 完成后主要承担验收证据，架构理由容易埋在实现细节中，且同一决策可能影响多份
Spec。

### 只维护 architecture 文档

拒绝。Architecture 必须保持为当前快照，直接覆盖会丢失当初的选择背景和被放弃方案。

### 只依赖代码与 Git 历史

拒绝。代码能表达“是什么”，但通常无法完整表达“为什么”和当时的约束。

## 后果

收益：

- 关键架构选择、边界和代价可以独立追溯；
- Spec 可以保持聚焦于范围与验收；
- Architecture 更新时不需要保留冗长历史叙事。

代价：

- 架构变更前增加少量文档工作；
- 必须持续维护 ADR、Spec 与 architecture 的交叉引用和状态一致性；
- 团队需要判断哪些选择足够长期，避免把普通实现细节写成 ADR。
