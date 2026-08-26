# ADR-0022：最小 project manifest 与本地 source discovery

## 状态

proposed

## 接受依据

不适用（`proposed`）。guide v0.32 与 ADR-0020 已生效，但 SPEC-0025 尚未完成；本 ADR 先
封闭候选 SPEC-0052 的工具输入与文件系统边界，不授权项目构建。

## 背景

ADR-0005/0020 已定义 frontend 所需的稳定 root identity、logical path 与显式 compilation-unit
输入，但没有定义磁盘项目如何产生这组输入。路线图只预留了“解析 `project.toml`”，若不先
固定最小 schema、路径安全和完整 source-set 规则，CLI 很容易从 cwd/首个源码猜项目、让 root
枚举顺序进入 identity，或把依赖、entry 与 native build 一次塞进同一个 Spec。

首个 filesystem provider 只需为本地单 compilation unit 生成 immutable base snapshot。LSP
已由 ADR-0021 接收 host 显式提供的内存 snapshot，不要求复用 manifest IO，因此现在没有新增
第六个 workspace crate 的充分理由。

## 决策

### version 1 schema

一个项目由调用方显式给出文件名精确为 `project.toml` 的 manifest 路径；首版不从 cwd、源码
路径或祖先目录搜索。manifest 必须是 UTF-8 TOML，并且只接受以下 schema：

```toml
schema = "koven.project"
version = 1

[project]
name = "hello-world"
source-roots = ["src", "generated"]
```

- `schema` 必须精确为 `koven.project`，`version` 必须为整数 `1`。version 1 的字段含义不得静默
  改变；breaking change 使用新版本。
- `project.name` 是展示与未来 package-manager identity，必须匹配 ASCII
  `[A-Za-z][A-Za-z0-9_-]*`；它不进入 Koven 源码 package identity。
- `project.source-roots` 是非空、无重复的字符串数组。每项是相对 manifest 目录的 UTF-8 `/`
  路径，禁止绝对路径、空段、`.`、`..`、反斜杠和尾 `/`；首版 root 必须位于项目目录内。
- version 1 拒绝未知顶层、`project` 字段和所有 dependency/target/entry 配置，避免拼写错误或
  未来字段被旧工具静默忽略。依赖、lock 与 executable target 由后继 ADR/Spec 定义。

每个 root 的规范化 manifest-relative path 同时是 ADR-0005 的 `root identity`。它不使用绝对/
canonical path、package 名、数组位置或项目搬迁前的位置；source-roots 数组顺序无语义。

### filesystem discovery

- manifest provider 负责 IO，`lang-frontend` 仍不读取文件系统。每个 root 必须存在、可读、是
  非 symlink 目录；不同 roots 的规范逻辑路径与解析后的物理目录不得相同或互相嵌套。
- provider 递归枚举 root 下的目录，不跟随 symlink；symlink entry 不进入 snapshot。只有扩展名
  精确为 `.ko` 的普通文件成为 source，其他普通文件忽略；不支持 ignore、glob、include/exclude、
  hidden 特例、generated 标记或环境变量插值。
- 纳入遍历的目录段与 `.ko` 文件名必须可表示为 UTF-8；源码内容也必须是合法 UTF-8。权限、
  类型、路径、编码或读取失败是 project/provider operational error，不产生 Koven `Ldddd`。
- provider 在 frontend 前拒绝重复 `(root identity, logical path)`、重叠 root 与重复 physical-file
  identity。physical/canonical 信息只用于 IO 安全与去重，不参与 source/package identity。
- `logical path` 是 root-relative UTF-8 `/` 路径并遵守 ADR-0005。读取全部源后按
  `(root identity, logical path)` UTF-8 byte order 排序，形成 immutable base snapshot；文件系统
  枚举顺序、mtime、inode 和绝对项目位置均不影响输出身份。合法空 root/空 source set 可以形成
  snapshot，entry/build 是否可用由后继阶段判断。

### crate 与阶段边界

- SPEC-0052 首版在 `lang-cli` 的窄 `project` 模块实现 manifest 解析和 filesystem adapter，输出
  SPEC-0025 的中性 source-set/unit 输入；不新增 `lang-project` member，也不让 LSP 依赖 CLI。
- 只有第二个真实 filesystem-manifest consumer 出现，并证明 CLI/LSP 复制实现的维护成本后，
  才由新 ADR 评估共享 crate。ADR-0021 的 LSP wire 与本 manifest 可以由外部 host 相互适配，
  但二者不是实现依赖。
- provider 不 lex/parse Koven 源码、不校验 package directive、不选择 entry，也不创建
  `DeclarationId`。SPEC-0025 之后的 frontend 才产生源码诊断；SPEC-0054 在 validated unit 上
  解析项目 entry 并编排 build/run。

## 替代方案

### 新增 `lang-project` workspace member

暂不采用。目前只有 CLI 需要 filesystem manifest provider，LSP 接收显式内存 snapshot；新增
member 会提前固定尚未出现的跨工具 API，并改变 ADR-0002 的五 crate 基线。

### 由 frontend 解析 manifest 或遍历目录

拒绝。项目 IO、symlink、权限与宿主路径规则不属于语言分析，会破坏 frontend 的显式输入边界。

### 默认使用 `src` 并允许省略 source-roots

暂不采用。显式 root 使空项目、多个 root 与迁移行为一致，不需要从约定反推 identity。未来可
在新 schema version 增加 shorthand，但 version 1 不存在隐式 root。

### 同时定义 dependency、target 与 entry

拒绝。依赖会引入独立 compilation-unit visibility/lock/ABI，entry 还需要 package-qualified
selector 与 process shape；它们都不是“从磁盘产生 base snapshot”的必要条件。

## 后果

收益：

- 本地项目以显式、可搬迁、枚举顺序无关的方式产生 ADR-0020 输入；
- 路径与 IO 失败在 frontend 前明确分类，不污染 `Ldddd` 或 package identity；
- SPEC-0052、0053、0054 可按 source provider、依赖锁定、本地 build 分阶段实施；
- 保持五 crate 与 frontend 纯分析边界，LSP 不被迫读取磁盘。

代价与风险：

- version 1 用户必须显式列出至少一个 source root，且不支持 workspace、外部或重叠 root；
- 不支持 ignore/glob，root 内所有普通 `.ko` 都进入 snapshot；
- manifest parser 与 filesystem adapter 首先位于 CLI，若未来出现第二个 consumer 可能需要提取；
- 项目仅能被检查/加载，直到 SPEC-0054 定义 target/entry 并接入 native build。

## 关联

- 相关 Spec：[SPEC-0052](../specs/0052-minimal-project-manifest-source-set.md)、SPEC-0053、
  [SPEC-0054](../specs/0054-local-project-build-run.md)
- 相关 ADR：[ADR-0002](./0002-bootstrap-workspace-layout.md)、
  [ADR-0005](./0005-package-source-root-mapping.md)、
  [ADR-0020](./0020-multifile-compilation-unit.md)、
  [ADR-0021](./0021-lsp-explicit-source-set-protocol.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
