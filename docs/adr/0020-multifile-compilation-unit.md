# ADR-0020：多文件 compilation unit、稳定身份与单 object 边界

## 状态

proposed

## 接受依据

不适用（`proposed`）。guide v0.32 候选尚未启用；本 ADR 在启用后仍需明确接受或使用有效
站立授权完成接受审计。

## 背景

ADR-0005 已固定 source root、逻辑路径和 package 的确定映射，但现有 frontend、ownership、
codegen 与 LSP 都以单个 `ParsedFile` 为处理边界。跨文件实现如果继续复用文件局部 `SymbolId`、
把已知声明伪装成 `ExternalSymbolId`，或由各阶段自行枚举文件，会导致身份依赖加载顺序、LSP
与 CLI 使用不同 resolver，并让单态化和 native link 无法获得完整程序边界。

v1 需要一个最小 compilation-unit 架构承接候选 guide §32，同时避免提前引入 manifest、依赖
求解、增量数据库或多 object 链接系统。

## 决策

### 输入与身份

- driver 显式构造 `CompilationUnitInput`：稳定排序的 source roots，以及每个 source unit 的
  root identity、规范化逻辑路径和源码。frontend 不读取文件系统。
- frontend 为 package、source unit 和顶层声明建立 `PackageId`、`SourceUnitId`、
  `DeclarationId`。ID 由规范化输入 key 与确定性收集顺序派生；不同输入枚举顺序产生相同
  语义图和诊断。
- 文件局部 `ScopeId` / `SymbolId` 继续服务局部绑定；跨文件引用必须解析为
  `DeclarationId`，不能降级为不透明 external symbol。ID 只属于一次 compilation，不承诺
  持久化、公共 ABI 或跨版本稳定编码。

### 分阶段产物

- 名称阶段先建立全局 package/declaration index，再为每个 source unit 建立 import environment
  和 resolution facts；类型与所有权阶段消费同一身份图并发布 compilation-unit 级产物。
- 每个阶段返回完整产物或有序诊断，不通过可变全局状态交换事实。诊断按稳定 source key、
  byte span 和错误码排序；单文件入口可以包装成只有一个 source unit 的 compilation unit。
- LSP 与 CLI 复用相同 frontend API。LSP 可在内存中替换打开 buffer 的源码，但不维护第二套
  package/import resolver。

### native 边界

- v1 的首个多文件 backend 接收一个已完成类型和所有权检查的 compilation-unit program，
  在全 unit 上计算可达 callable、单态实例和 drop glue，并生成一个目标 object。
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
  [ADR-0010](./0010-first-native-object-and-linker-contract.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
