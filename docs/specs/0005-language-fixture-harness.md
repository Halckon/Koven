# SPEC-0005: 建立语言 fixture harness

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P0-005` |
| 所属 Phase | Phase 0 |
| 语言规范 | [`agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) |
| 前置 Spec | SPEC-0001、SPEC-0003、SPEC-0004 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无；仅因前置 Spec 未完成而暂不可实施 |
| 影响范围 | `lang-frontend` 测试、`.ko` fixtures、Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，真实 Cargo test target 会确定性枚举并执行仓库中的 `.ko` fixture，且零用例或
非法 fixture 会使测试失败。

## 2. 背景

虚拟 workspace 根下的目录不会被 Cargo 自动执行。若没有可证明的枚举和零用例保护，后续
语言测试可能在完全未运行时显示全绿。Phase 0 只验证 source / AST / 诊断骨架，不引入临时
parser。

## 3. 范围与需求

- 在真实 Cargo integration test target 中建立最小 fixture 发现与运行入口。
- 使用 `.ko` 作为唯一目标语言源码扩展名，按路径排序后执行，拒绝未知文件类型。
- Phase 0 至少包含一个 source-loading pass fixture；它只验证读取、source / `Span` 和人工
  AST / 诊断接线，不声称源码已被解析。
- 枚举到零 fixture 时测试必须失败，并有对该保护行为自身的单元测试。
- runner 输出 case 相对路径和结果，不包含仓库绝对路径或随机顺序。
- 为后续 pass / fail fixture 保留最小目录约定，但正式错误码和期望格式由首个使用它的功能
  Spec 增量定义。

## 4. 非目标

- 不实现 lexer、parser、类型检查、所有权检查或端到端可执行文件测试。
- 不固定通用 snapshot 框架、机器诊断 schema 或跨平台 test runner CLI。
- 不添加“总是通过”的占位编译回调，也不把文件存在等同于语义通过。

## 5. 验收标准

- [ ] `cargo test -p lang-frontend --test fixtures`（或实施后等价真实 target）至少执行一个 `.ko` case。
- [ ] 临时空 suite 会返回配置错误，且该保护有自动测试。
- [ ] fixture 顺序按仓库相对路径稳定排序。
- [ ] 未知扩展名会明确失败。
- [ ] Phase 0 fixture 只断言已实现的 source / AST / 诊断能力，不调用临时 parser。
- [ ] workspace fmt、check、Clippy 和 test 基线通过；无 ignored / filtered case 被隐瞒。
- [ ] Architecture 记录 harness 的 Cargo target、fixture 根目录与执行路径。

## 6. 技术方案与边界

runner 首先作为 `lang-frontend` 测试支持代码存在，因为 Phase 0–3 fixture 的共同消费者是
frontend。保持普通文件系统枚举和明确断言，不先引入 snapshot 依赖。Codegen 运行 fixture
在 Phase 4 出现真实需要时复用目录约定或建立自己的 target，不提前泛化。

## 7. 实施计划

1. [ ] 建立 fixture 目录、case 枚举和确定性排序 → 验证：发现 / 排序单测
2. [ ] 接入真实 Cargo test target 与至少一个 `.ko` source-loading case → 验证：窄集成测试
3. [ ] 增加零 fixture 和未知文件保护 → 验证：失败路径单测
4. [ ] 更新 Architecture 和 Spec 验收记录 → 验证：全 workspace 基线

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | fixture harness、Phase 0 case、自检测试与 Architecture | `test(frontend): add language fixture harness (SPEC-0005)` |

## 9. 未决问题

- compile-fail sidecar 的最终字段在第一个产生正式语言诊断的 Spec 中确定；在机器协议 ADR
  接受前，不把内部 fixture 格式承诺为公共协议。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 〈实施时填写〉 | 未执行 | 当前仅完成 Draft Spec |
