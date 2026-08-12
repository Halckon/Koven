# SPEC-0003: 建立结构化诊断核心

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P0-003` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) |
| 前置 Spec | SPEC-0002 `done` |
| 前置 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md) 必须为 `accepted` |
| 关联 ADR | 机器可读公共协议另需后续 ADR |
| 阻塞项 | ADR-0003 尚未 `accepted`，且前置 Spec 未完成 |
| 影响范围 | `lang-frontend`、`lang-cli`、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，编译阶段可产生带稳定错误码和源码范围、顺序确定且可由 CLI 人类可读渲染的诊断。

## 2. 背景

诊断是所有非法源码路径的公共输出，必须先于 lexer 建立。不先固定内部模型，后续阶段容易
散落字符串、重复行列计算或依赖无序集合输出。

## 3. 范围与需求

- 定义诊断严重级别、稳定 `Ldddd` 错误码、主消息、主 `Span`、关联标签、说明和建议。
- 建立集中错误码目录 API；测试可使用 `cfg(test)` 目录，不能为尚未定义的语言错误提前发布
  正式错误码。
- 对错误码格式、重复注册、缺失主位置和非法关联范围返回具体内部错误。
- 提供稳定排序规则，不直接使用 `HashMap` / `HashSet` 的随机迭代结果。
- CLI 提供最小无颜色人类可读渲染，统一复用 SPEC-0002 的行列换算。
- 多个诊断即使从不同收集顺序进入，也产生相同排序和文本结果。

## 4. 非目标

- 不分配 lexer、parser、类型或所有权错误的正式语义编号。
- 不固定 JSON、JSON Lines、LSP Diagnostic 或其他公共机器协议。
- 不实现终端颜色探测、复杂源码折叠、自动修复应用或错误恢复。

## 5. 验收标准

- [ ] 单测覆盖完整诊断、仅主标签诊断、多标签诊断和建议文本。
- [ ] 非 `Ldddd`、重复代码、缺失主 `Span` 等不变量被明确拒绝。
- [ ] 同一诊断集合的不同插入顺序产生逐字节一致的渲染结果。
- [ ] Unicode、CRLF、多行与 EOF `Span` 的渲染位置正确。
- [ ] 生产错误码目录不含为了让 Phase 0 测试通过而虚构的语言错误。
- [ ] 受影响 crate 的窄测试及 workspace fmt、check、Clippy、test 基线通过。
- [ ] Architecture 记录诊断从 frontend 产物到 CLI renderer 的边界。

## 6. 技术方案与边界

内部诊断模型属于 frontend 可复用 API；CLI 只负责展示策略，不拥有语义错误生成逻辑。
默认稳定键使用 source 的用户可见名称、主范围起点/终点、错误码和消息。机器可读协议延后到
LSP 和 CLI 消费者真实出现后以 ADR 固定，避免 Phase 0 过早承诺兼容格式。

## 7. 实施计划

1. [ ] 实现错误码与诊断数据模型及不变量 → 验证：构造 / 拒绝单测
2. [ ] 实现确定性排序与最小 CLI renderer → 验证：乱序输入 golden
3. [ ] 覆盖 Unicode、CRLF、多行范围并审阅 golden → 验证：窄集成测试
4. [ ] 更新 Architecture 和 Spec 验收记录 → 验证：全 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 内部诊断模型、renderer、测试与 Architecture | `feat(diagnostics): add structured diagnostics core (SPEC-0003)` |

## 9. 未决问题

- 正式错误码按后续功能 Spec 分配；机器诊断 schema、颜色策略和 LSP 映射不在本 Spec 固定。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 〈实施时填写〉 | 未执行 | 当前仅完成 Draft Spec |
