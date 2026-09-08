# ADR-0021：LSP 显式 source-set 初始化协议与 snapshot 生命周期

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-26 持续 Goal 已授权按 roadmap 与依赖图推进；接受前审计确认本 ADR 只封闭现行
v0.32 留给 host 的 source-set wire、base/overlay 与 snapshot 生命周期，不改变语言语义、
frontend compilation-unit 边界或 legacy 单文档行为。SPEC-0025/0197/0198 尚未完成，因此
本次接受不批准或授权实施 SPEC-0187。

## 背景

SPEC-0055/0056 的现有 LSP 对每个打开 URI 建立独立 `SourceMap`，不读取磁盘。多文件 package /
import 分析不能把“当前打开的 URI 集合”当作完整 compilation unit：未打开声明会消失，close
会改变 package membership，跨文件诊断和 definition 也无法获得稳定 target。

ADR-0005/0020 已规定抽象 source root、逻辑路径、unit identity 与整体 snapshot，但没有规定
host 如何把完整 base unit 交给 LSP。让 server 自行扫描 workspace 会同时引入 manifest discovery、
symlink/IO 和文件监听；只在测试中注入私有数据则不能形成可用的 host 协议。

## 决策

### 版本化初始化输入

首版在标准 `InitializeParams.initializationOptions` 下接受可选的 `koven.sourceSet`：

```json
{
  "koven": {
    "sourceSet": {
      "schema": "koven.lsp.source-set",
      "version": 1,
      "roots": ["main", "generated"],
      "sources": [
        {
          "root": "main",
          "logicalPath": "app/main.ko",
          "uri": "file:///workspace/src/app/main.ko",
          "text": "package app\n"
        }
      ]
    }
  }
}
```

- 一个 LSP session 首版最多承载一个 compilation unit。`root` 是 host 提供的非空 opaque UTF-8
  稳定 identity，按 UTF-8 bytes 精确比较；它可以直接承载 ADR-0022 的 manifest-relative root
  identity，但 server 不把它解释为物理目录、数组序号或 package path。
- `roots` 必须非空且无重复；`sources` 可以为空，未被 source 引用的 root 也合法，从而可表示
  已知 root 尚无 `.ko` 文件的空 unit。每个 source 的 `root` 必须存在于 `roots`。
- `logicalPath` 精确遵守 ADR-0005：UTF-8、`/` 分隔、相对、无空段、`.` 或 `..`，并以 `.ko`
  结尾。语义 source key 是结构化 `(root, logicalPath)`；server 校验后按此 key 排序。
- `uri` 只是 presentation locator，可为任意合法绝对 LSP URI。source key 与 parsed URI 在 unit
  内分别唯一且互为一一映射；server 不由 URI 推导 root、逻辑路径、宿主大小写或 symlink。
- `text` 是固定 base snapshot 的完整 UTF-8 源码。server 不打开 URI、不 `stat` 路径、不读取
  workspace。roots/sources 数组顺序不影响 unit identity、诊断或 publish 顺序。
- `sourceSet` 存在时严格校验该对象自身的 schema/version、字段、root 引用、路径、URI 与
  重复项，未知 `sourceSet` 字段也拒绝；结构错误使 initialize 返回 `InvalidParams`，不伪造
  Koven `Ldddd`。普通源码错误仍是成功 snapshot 中的 frontend 诊断。内部 frontend/映射失败
  返回 `InternalError`。`initializationOptions` 的其他顶层字段及 `koven` 下除 `sourceSet` 外的
  sibling 不属于本协议，server 不因本 ADR 拒绝它们。
- `sourceSet` 缺席时精确保留 SPEC-0055/0056 的 legacy 单文档模式。其他非 Koven
  initialization options 不由 server 拒绝。未来 breaking wire change 使用新的 `version`；不得
  静默改变 version 1 的字段含义。

### base、overlay 与发布

- initialize 成功后持有 immutable base sources；收到标准 `initialized` notification 后，按
  source key 向全部 base URI 发布首批 diagnostics，base 文档的 version 为 `None`。
- 以下严格事件规则只适用于提供了 `sourceSet` 的 unit mode。`didOpen` 只接受 base 中已知 URI，
  建立 `{version, text}` overlay；`didChange` 只接受已打开 URI、一个 full-document change 和
  严格递增 version。未知、重复 open、未打开/stale/partial change 记录协议日志并忽略，不改变
  membership 或 snapshot。
- unit mode 的 `didClose` 只删除 overlay，并用 immutable base text 重建 unit；该 source 仍属于
  unit，随后发布 base diagnostics 且 version 为 `None`，不能沿用单文档模式的无条件清空。
  未提供 `sourceSet` 时，open/change/close 精确保留 SPEC-0055/0056 已实现的 legacy 行为，包括
  duplicate open 覆盖、未打开 change 忽略、不强制 version 递增以及 close 发布空 diagnostics。
- base membership 在 session 内固定。增加/删除 base source、磁盘刷新、watched files、manifest
  discovery 和多个 compilation unit 等待后续协议；首版通过新 session 替换 base。
- 每次合法事件都从候选 overlay 集合构造一个全新的、共同拥有单一 `SourceMap` 与各 frontend
  产物的 unit snapshot。普通源码诊断不阻止替换；内部分析或 mapping 失败时记录 error 并保留
  last-good overlays、snapshot 与 definition facts，绝不混用新旧 `map_id` 的 Span。
- 成功 snapshot 先预计算全部 publish payload，再按 source key 向所有 URI 发布一次（包括空
  diagnostics），从而清除跨文件陈旧结果。primary Span 决定目标 URI；related location 按自身
  SourceId 映射。打开文档带当前 overlay version，base-only 文档 version 为 `None`。

### definition

- definition query 先由 URI 定位当前 snapshot 的 `(SourceUnitId, SourceId)`；以稳定的
  `(SourceUnitId, byte offset)` 查询 frontend reference fact，再用同一 snapshot 的 `SourceId`
  完成 Span/UTF-16 映射。target `DeclarationId` 映射为 target SourceUnitId/SourceId/Span/URI。
  目标无需打开，但必须属于 base unit。
- exact import terminal/alias、限定路径和普通引用可跳转到声明；wildcard 的 `*` 与纯 package
  segment 没有 definition，wildcard 引入名称的实际使用跳转到目标声明。未知 URI 返回 `null`。

## 替代方案

### LSP 自行读取 `project.toml` 并扫描磁盘

暂不采用。它会把尚未物化的 SPEC-0052 manifest、文件发现、IO、安全边界与监听策略并入
SPEC-0187，也破坏现有“不读取未打开文件”的可复核基线。未来 project host 可以把发现结果
适配为本协议，而不改变 frontend unit API。

### 把当前打开 URI 集合作为 unit

拒绝。open/close 会改变 package membership，未打开 target 无法诊断或跳转，同一项目在不同
编辑器状态下得到不同语言结论。

### URI 推导 root 与 logical path

拒绝。URI 可能是虚拟文档，也包含宿主大小写、编码和 symlink 差异；它是展示定位，不是
ADR-0005 的语义 source key。

### 新增共享 project crate 后由 LSP 扫描 manifest

不作为首个多文件 LSP 的前置。CLI/LSP 共享项目发现以后可能证明新 workspace member 的必要性，
但当前显式 wire 已能在不改变五 crate 边界的情况下交付确定、多 root、虚拟源码的 unit。

## 后果

收益：

- LSP、测试与未来 project host 可向同一 frontend API 交付完全确定的 source set；
- overlay 不改变 unit membership，close 可恢复 base，跨文件诊断与 definition 不依赖磁盘状态；
- legacy 单文档客户端继续工作，version 1 wire 为 host 集成提供稳定兼容边界；
- 不新增 workspace crate，也不把文件系统或 LSP URI 语义泄漏到 frontend。

代价与风险：

- host 必须在 initialize payload 中提供全部 base 源码，超大项目的 payload 与全量重分析成本较高；
- 首版不感知磁盘或成员变化，host 需要重启 session 才能替换 base；
- 自定义 initialization option 需要编辑器/host 集成；仅使用通用 LSP 客户端时仍退化为单文档模式；
- 将来 manifest/增量方案必须保持 version 1 行为，或显式发布新协议版本。

## 关联

- 相关 Spec：[SPEC-0187](../../archive/specs/0187-multifile-lsp-diagnostics-definition.md)、
  [SPEC-0055](../../archive/specs/0055-single-document-lsp-diagnostics.md)、
  [SPEC-0056](../../archive/specs/0056-single-document-definition.md)
- 相关 ADR：[ADR-0004](./0004-source-span-position-model.md)、
  [ADR-0005](./0005-package-source-root-mapping.md)、
  [ADR-0020](./0020-multifile-compilation-unit.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
