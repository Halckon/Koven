# ADR-0020：多文件 compilation unit、稳定身份与单 object 边界

## 状态

accepted

## 接受依据

2026-08-26，用户明确启用 guide v0.32 取代 v0.31；当前站立授权允许按依赖图接受已完成
审计的 ADR 并推进后继 Spec。本 ADR 只封闭 §32 留给实现的 compilation-unit 身份与阶段
边界，不改变语言可观察语义，因此据此接受。

## 背景

ADR-0005 已固定 source root、逻辑路径和 package 的确定映射，但现有 frontend、ownership、
codegen 与 LSP 都以单个 `ParsedFile` 为处理边界。跨文件实现如果继续复用文件局部 `SymbolId`、
把已知声明伪装成 `ExternalSymbolId`，或由各阶段自行枚举文件，会导致身份依赖加载顺序、LSP
与 CLI 使用不同 resolver，并让单态化和 native link 无法获得完整程序边界。

v1 需要一个最小 compilation-unit 架构承接候选 guide §32，同时避免提前引入 manifest、依赖
求解、增量数据库或多 object 链接系统。

## 决策

### 输入与身份

- driver 先把全部源码加入同一个 `SourceMap`，再显式构造 `CompilationUnitInput`。每项包含
  root identity、规范化逻辑路径、同一 map 中的 `SourceId` 与 `ParsedFile`；展示路径/URI 与
  语义 source key 分离。API 接受任意输入顺序，校验重复 key 后按 `(root identity, logical path)`
  内部排序；frontend 不读取文件系统，也不复制源码到新的 `SourceMap`。
- frontend 为 package、source unit 和所有源码 symbol 建立 `PackageId`、`SourceUnitId` 与
  `UnitSymbolId { source_unit, symbol }`；可作为引用目标的声明再使用 `DeclarationId`，package
  index 只保存其中的顶层子集。`SourceUnitId` 在规范化排序后分配，只在本次分析链内稳定。
- 文件局部 `ScopeId` / `SymbolId` 与 AST node ID 继续服务 local flow，并始终由 source/body
  identity 限定；不对整 unit 的 AST 全局重编号。跨文件引用必须解析为 `DeclarationId`，不能
  降级为不透明 external symbol。所有 ID 均不承诺持久化、公共 ABI 或跨版本稳定编码。

### 分阶段产物

- 名称阶段先建立全局 package/declaration index，再为每个 source unit 建立 import environment
  和 resolution facts；compiler 注入的 `NameEnvironment` / `TypeEnvironment` 仍只表示外部绑定，
  不承载 package index。类型阶段建立 unit-global canonical `TypeTable`，所有 source facts 共用
  同一 `TypeId` 空间；所有权阶段消费同一 identity/type graph。
- 每个阶段都返回 recovery product 与有序诊断，供诊断和 LSP 使用；只有无 error 的
  validated view/marker 才能进入下一阶段，不能把部分产物伪装成完整输入。单文件入口继续保留
  原 API 和事实含义，并包装成一个 source unit 的 unit 分析。
- 新的 `ordered_unit_diagnostics` 按稳定 source key、byte span 和错误码排序；现有单文件排序
  API 保持不变。渲染时仍使用 presentation source name，不得用 map-local `SourceId` 排序。
- LSP 与 CLI 复用相同 frontend API。LSP snapshot 必须共同拥有同一 `SourceMap` 与各阶段产物，
  buffer 更新后整体替换 snapshot，不能混用新旧 map 的 Span。磁盘/base source set 如何提供
  不由本 ADR 决定；首个 LSP host 协议由 ADR-0021 的显式初始化 source set 封闭。

### native 边界

- v1 的首个多文件 backend 接收一个已完成类型和所有权检查的 compilation-unit program，
  由显式 `DeclarationId` entry 在全 unit 上计算可达 callable、单态实例和 drop glue，生成一个
  SSA module、一个 LLVM module 和一个目标 object。公开项目 entry 发现与 CLI source discovery
  不属于该内部 codegen 边界。
- CLI 继续使用 ADR-0010 的系统 linker 链接该 object。不同 Koven source unit 不各自生成
  object，也不发布跨 object 的 Koven ABI；这避免在 package 语义刚落地时同时引入 symbol
  visibility、COMDAT、重复单态实例和初始化顺序问题。

## 替代方案

### 每个文件独立编译，再由 linker 合并

不采用首版。它要求先定义跨 object ABI、可见性、generic instance 合并和初始化顺序，远超
候选 package/import 的最小闭环。

### 把跨文件声明表示为 ExternalSymbolId

不采用。同一 compilation unit 内目标已知，丢失身份会迫使类型、所有权、codegen 与 LSP
重新按字符串解析，并破坏确定性引用和精确跳转。

### 立即引入增量查询数据库

不采用。首版没有已批准的持久缓存或变更传播需求；显式不可变阶段产物足以验证语义，未来
若引入增量系统，可在不改变这些输入/输出身份的前提下新增 ADR。

## 后果

收益：

- CLI、LSP 与 backend 共享一套 package/declaration identity 和确定性诊断；
- 单文件 API 可平滑退化为单 source unit，不需要并存两套 frontend；
- 单 object 策略复用现有 native linker 边界，并延后不必要的跨 object ABI。

代价与风险：

- frontend 公共产物需要从文件级扩展到 compilation-unit 级，并迁移直接调用方；
- 首版修改任一文件仍可重新检查/生成整个 unit，不提供增量性能保证；
- 大型项目最终可能需要多 object、缓存和并行查询，届时必须以新 ADR 扩展。

## 关联

- 候选实施 Spec：SPEC-0025、SPEC-0197、SPEC-0198、SPEC-0199、SPEC-0187
- 相关 ADR：[ADR-0004](./0004-source-span-position-model.md)、
  [ADR-0005](./0005-package-source-root-mapping.md)、
  [ADR-0010](./0010-first-native-object-and-linker-contract.md)、
  [ADR-0021](./0021-lsp-explicit-source-set-protocol.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
