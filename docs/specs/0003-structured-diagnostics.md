# SPEC-0003: 建立结构化诊断核心

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P0-003` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) |
| 前置 Spec | SPEC-0002 `done` |
| 前置 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) 均必须为 `accepted` |
| 关联 ADR | 机器可读公共协议另需后续 ADR |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`、`lang-cli`、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，编译阶段可产生带稳定错误码和源码范围、顺序确定且可由 CLI 人类可读渲染的诊断。

## 2. 背景

诊断是所有非法源码路径的公共输出，必须先于 lexer 建立。不先固定内部模型，后续阶段容易
散落字符串、重复行列计算或依赖无序集合输出。

## 3. 范围与需求

- 定义诊断严重级别、稳定 `Ldddd` 错误码、主消息、主 `Span`、关联标签、说明和建议；主与
  关联 `Span` 均按 ADR-0004 自带 source identity。
- 建立集中错误码目录 API；单元测试可使用 `cfg(test)` 目录，integration test 可使用只编译进
  测试 target 的共享 support 模块，不能为尚未定义的语言错误提前发布正式错误码。
- 公共构造 API 强制接收严重级别、已验证错误码、非空单行主消息和主 `Span`，不让缺失主
  位置成为可构造状态；关联标签、说明和建议文本同样必须非空且单行；对错误码格式、重复
  注册、非法必填文本和无法由当前 source map 解析的范围返回具体内部错误。
- 提供覆盖全部可渲染字段的稳定全序，不直接使用 `HashMap` / `HashSet` 的随机迭代结果，也
  不使用 `SourceId` 数值或加载顺序作为 tie-breaker。
- CLI 提供返回文本或具体内部错误的最小无颜色纯 renderer，不直接写 stderr，并统一复用
  SPEC-0002 的行列换算。
- 多个诊断即使从不同收集顺序进入，也产生相同排序和文本结果。

## 4. 非目标

- 不分配 lexer、parser、类型或所有权错误的正式语义编号。
- 不固定 JSON、JSON Lines、LSP Diagnostic 或其他公共机器协议。
- 不实现终端颜色探测、复杂源码折叠、自动修复应用或错误恢复。

## 5. 验收标准

- [x] 单测覆盖完整诊断、仅主标签诊断、多标签诊断和建议文本。
- [x] 非 `Ldddd`、重复代码、空或多行必填文本被明确拒绝；公共 API 无法创建缺失主 `Span`
      的诊断。
- [x] 同一诊断集合的不同插入顺序产生逐字节一致的渲染结果；逐层覆盖主 source / 范围、
      严重级别、错误码、主消息，以及关联标签、说明、建议完整序列的 tie-breaker。
- [x] 跨 source 关联标签可正确渲染；无法由给定 source map 解析的主或关联 `Span` 返回内部
      错误而不 panic。
- [x] Unicode、CRLF、多行与 EOF `Span` 的渲染位置正确。
- [x] 生产错误码目录不含为了让 Phase 0 测试通过而虚构的语言错误。
- [x] 受影响 crate 的窄测试及 workspace fmt、check、Clippy、test 基线通过。
- [x] Architecture 记录诊断从 frontend 产物到 CLI renderer 的边界。

## 6. 技术方案与边界

内部诊断模型属于 frontend 可复用 API；CLI 只负责展示策略，不拥有语义错误生成逻辑。
诊断集合依次按主 source 名称、主范围、严重级别、错误码、主消息及关联标签、说明、建议的
完整有序序列比较；单条诊断内部保持生产者给出的顺序。

Phase 0 renderer 固定以下无颜色、无代码框的内部文本形态，并始终展示半开范围的起止位置：

```text
error[L0001] sample.ko:1:2-1:4: primary message
  label other.ko:2:1-2:3: related message
  note: note text
  help: suggestion text
```

每个 label、note、help 独占一行，顺序与模型一致。renderer 使用 source map 中的用户可见
名称，仅把反斜杠、CR、LF 分别展示为 `\\`、`\r`、`\n`，避免名称破坏单行结构或产生转义
歧义；它不自行读取、规范化或附加机器文件系统路径。该格式用于 Phase 0 人类可读验证，
不是版本化机器协议。机器协议延后到 LSP 和 CLI 消费者真实出现后以 ADR 固定，避免过早
承诺兼容格式。

## 7. 实施计划

1. [x] 实现错误码与诊断数据模型及不变量 → 验证：构造 / 拒绝单测
2. [x] 实现确定性全序与最小 CLI renderer → 验证：同主键差异字段的乱序输入 golden
3. [x] 覆盖跨 source、Unicode、CRLF、多行范围和非法 source map → 验证：窄集成测试
4. [x] 更新 Architecture 和 Spec 验收记录 → 验证：全 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 内部诊断模型、renderer、测试与 Architecture | `feat(diagnostics): add structured diagnostics core (SPEC-0003)` |

## 9. 未决问题

- 正式错误码按后续功能 Spec 分配；机器诊断 schema、颜色策略和 LSP 映射不在本 Spec 固定。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test diagnostic_model --locked --offline` | 通过 | 9 passed；0 failed / ignored / measured / filtered out |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 3 passed；含损坏关联标签的防御性错误路径 |
| `cargo test -p lang-frontend --doc --locked --offline` | 通过 | 1 compile-fail doctest passed；锁定主 `Span` 必填 |
| `cargo test -p lang-cli --bin kovenc --locked --offline` | 通过 | 6 passed；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 member 的全部 target 检查成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 无 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 共 28 passed；0 failed / ignored / measured / filtered out |
| `cargo build -p lang-cli --locked --offline` | 通过 | `kovenc` dev profile 构建成功 |
