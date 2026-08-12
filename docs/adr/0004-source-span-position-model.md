# ADR-0004: 源码身份、范围与展示位置模型

## 状态

accepted

## 背景

Token、AST、诊断、LSP 和调试信息都需要引用源码位置。若范围只保存字节偏移而由外围隐式
携带源码，跨文件诊断容易丢失身份；若各阶段分别缓存行列，则 Unicode、CRLF 和源码更新会
产生不一致。内部模型还不能依赖文件系统路径、加载顺序或某个尚未确定的公共协议。

因此，需要在 Phase 0 固定跨阶段使用的源码身份与范围不变量，同时把文件发现、module 映射
和协议坐标留给其各自的后续决策。

## 决策

- 一次分析流水线共享一个 `SourceMap`。它拥有加入后不可变的 UTF-8 源码快照及集中建立的
  行起始索引。
- `SourceId` 是不透明的 map-local 身份，只在分配它的 `SourceMap` 及对应源码快照生命周期内
  有效。它不等同于路径，也不承诺跨 map、进程、构建或序列化稳定；稳定产物不得依赖其数值
  或分配顺序。
- 每份源码由调用方提供稳定的用户可见名称。同一 `SourceMap` 内该名称必须唯一，重复注册
  返回具体错误，使诊断排序和展示不需要回退到加载顺序。
- `Span` 自身包含 `SourceId` 与 `[start, end)` UTF-8 字节范围。受检构造必须保证 source
  存在、`start <= end <= source length`，且两个端点均位于 UTF-8 字符边界；允许空范围和 EOF
  空范围。
- `Span` 不缓存行列。人类可读位置在展示边界通过所属 `SourceMap` 计算为 1-based 行列；列
  按行首到目标 offset 之前的 Unicode scalar value 数量加一，tab 计一个 scalar。换行从
  `\n` 后开始；在 `\r\n` 中两个换行字节的 offset 均属于前一行，裸 `\r` 不单独换行。
- 把另一个 `SourceMap` 分配的 ID 或 `Span` 交给当前 map 属于内部调用方违反不变量。当前
  map-local ID 表示不承诺检测数值恰好有效的跨 map 误用，因此所有阶段必须共享产生这些
  `Span` 的同一 `SourceMap`。
- LSP、机器诊断和 DWARF 在各自适配边界把内部字节范围转换为所需坐标或协议字段；不得把
  UTF-16、0-based 坐标或公共 schema 反向写入内部 `Span` 模型。

本 ADR 不决定文件发现、路径规范化、source root / module 映射、非法文件字节的诊断、终端
视觉宽度、LSP position encoding 协商、增量源码版本、rope、宏展开来源映射或具体 Rust 容器。

## 替代方案

### 只保存 source-local 字节范围

拒绝。调用方必须额外携带隐式 source 上下文，跨文件关联标签容易和错误源码组合。

### 在 `Span` 中缓存行列

拒绝。同一信息会同时以字节范围和行列存在，容易漂移；lexer、诊断和 LSP 也可能各自形成
不同的 Unicode 与 CRLF 算法。

### 直接用规范化路径作为持久 Source ID

暂不采用。module 映射、虚拟源码、大小写和 symlink 策略尚未确定，提前绑定路径会越过现行
guide 和后续 package / module 决策。

## 后果

收益：

- 每个 `Span` 都能追溯到明确源码，跨文件 AST 和诊断不依赖外围隐式状态；
- 所有阶段共享一套 UTF-8、Unicode 和 CRLF 位置规则；
- 内部模型保持协议无关，LSP、CLI 和 DWARF 可以在边界独立转换；
- 稳定输出不受 source 加载顺序影响。

代价与风险：

- 所有消费 `Span` 的阶段必须共享同一 `SourceMap`；
- source 注册需要维护用户可见名称唯一性；
- map-local ID 不能直接用于持久化、缓存键或跨进程协议；
- 后续增量编辑和生成源码需要新的 revision / origin 决策，不能静默扩展本模型。

## 关联

- 相关 Spec：[SPEC-0002](../specs/0002-source-span-foundation.md)、
  [SPEC-0003](../specs/0003-structured-diagnostics.md)、
  [SPEC-0004](../specs/0004-indexed-ast-foundation.md)
- 相关 ADR：[ADR-0003](./0003-diagnostic-architecture.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
