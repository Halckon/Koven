# ADR-0014: 版本化 JSON Lines 机器诊断协议

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权。

## 背景

ADR-0003 已让 frontend 拥有结构化 `Diagnostic`，CLI 负责展示，LSP 通过自己的 UTF-16
适配层消费同一模型。SPEC-0055 已证明 Lexer、Parser、名称、类型和所有权诊断可以被外围
工具稳定聚合；Phase 6 现在需要一个不依赖终端文本、颜色或 LSP transport 的机器接口。

机器消费者需要稳定识别 schema、诊断码、精确源码范围和关联信息，但 Koven 尚未定义通用
编译事件流、增量协议、自动修复或 build command。把内部 Rust 结构直接序列化会泄漏
`SourceId`、枚举布局和未来实现细节；把所有 CLI 失败都伪装成语言诊断又会破坏 `Ldddd`
只表示用户源码问题的既有不变量。

## 决策

### 启用方式与输出边界

- CLI 默认继续输出 ADR-0003 的人类可读文本。显式全局选项
  `kovenc --message-format=json <command> ...` 才启用机器诊断；本 ADR 不改变命令自己的成功
  stdout、退出码或文件写入行为。
- 每条 frontend `Diagnostic` 编码为一个 UTF-8 JSON object，后跟一个 `\n`；空诊断集合不
  输出记录。记录按 `ordered_diagnostics` 的唯一全序产生，字段和值在相同输入下字节确定。
- 机器诊断沿用诊断流的 stderr。这样 formatter 的格式化源码 stdout、未来编译产物与机器
  诊断不会混流。
- 用法错误、文件 I/O、非 UTF-8 输入、内部不变量失败和输出写入失败不是 frontend
  `Diagnostic`，继续使用普通 stderr 与既有退出码；不得给它们伪造 `Ldddd`。因此消费者只在
  成功选择机器模式且命令实际产生结构化诊断时，把对应 stderr 行解释为本协议。

### v1 schema

每条记录必须包含以下字段：

- `schema`: 固定字符串 `koven.diagnostic`；
- `version`: JSON number `1`；
- `severity`: `error` 或 `warning`；
- `code`: 稳定 `Ldddd` 字符串；
- `message`: 未改写的单行主消息；
- `primary`: 一个 location；
- `details`: 按生产者顺序排列的 detail 数组。

location 包含原样 `source` 名称、半开 UTF-8 `byte_start` / `byte_end`，以及 `start` / `end`
位置。位置对象的 `line` / `column` 均为 1-based；column 按 Unicode scalar value 计数，与
ADR-0004 和人类 renderer 一致。JSON 自身负责 source name 和文本的转义，不做路径发现、
规范化或重映射。

detail 是带 `kind` 判别字段的 object：`label` 还包含 `message` 和 `location`；`note` / `help`
只包含 `message`。v1 不承诺建议 edit、颜色片段、渲染文本、LSP UTF-16 range、诊断分组或
编译阶段名称。

### 兼容和实现边界

- `version` 表示单条诊断记录 schema。删除字段、改变含义/单位/基数、改变枚举拼写或 detail
  形态属于 breaking change，必须通过后续 ADR 增加版本；消费者应忽略同版本未知字段，以
  允许向后兼容的可选扩展。
- 协议 adapter 位于 `lang-cli`，输入仍是 `&SourceMap` 与 `&[Diagnostic]`。frontend 不依赖
  Serde/JSON，LSP 继续使用自身协议类型，二者不得通过机器 schema 反向耦合。
- adapter 必须先解析、排序并编码完整集合，任一 foreign/invalid span 或 JSON 编码错误都
  返回具体内部错误且不产生部分记录。
- 实现复用 workspace 已有、lockfile 固定的 `serde_json`；不新增通用事件框架、日志依赖或
  自定义 JSON encoder。

## 替代方案

### 直接序列化 frontend `Diagnostic`

拒绝。它会把 Rust 字段名、枚举布局和 map-local `SourceId` 变成公共协议，并迫使 frontend
依赖展示格式。

### 使用单个 JSON 数组

拒绝。JSON Lines 允许消费者逐条处理，也让未来编译过程中的诊断能够流式产生；当前实现仍
先完整验证，避免失败时输出半条有效集合。

### 复用 LSP Diagnostic

拒绝。LSP 使用 0-based UTF-16 position、URI 和 transport 特定字段；通用 CLI 工具不应构造
虚假 URI，也不应让 LSP 的演进决定 CLI schema。

### 把所有 CLI 错误纳入同一事件协议

暂不采用。当前没有稳定 build/event 模型，文件 I/O 和用法错误也没有 `Span` 或 `Ldddd`。
需要全机器化命令生命周期时应另立 Spec/ADR，定义 event envelope 和 operational error。

### 机器诊断写 stdout

拒绝。`kovenc format` 的成功产物已经使用 stdout；让诊断也进入 stdout 会迫使消费者依据
退出状态重新解释同一字节流，并妨碍未来命令复用。

## 后果

收益：

- 工具可以稳定消费完整诊断结构，而不解析人类文本；
- 字节范围支持精确源码切片，统一行列支持直接展示；
- frontend、CLI JSON 与 LSP UTF-16 三个职责边界保持清晰；
- schema 的 breaking-change 规则在首个消费者出现时即被固定。

代价与风险：

- machine mode 的 operational error 仍不是 JSON，消费者必须结合退出码区分协议诊断与命令
  失败；
- 同时携带字节和行列会增加记录大小，但避免每个消费者重复读取源码换算；
- 1-based scalar column 与 LSP 的 0-based UTF-16 range 不同，跨协议消费者必须显式适配；
- v1 字段一旦发布不能静默改名或改变单位。

## 关联

- 相关 Spec：SPEC-0060
- 相关 ADR：ADR-0003、ADR-0004
- 取代的 ADR：无
- 被以下 ADR 取代：无
