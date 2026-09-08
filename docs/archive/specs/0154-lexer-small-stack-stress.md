# SPEC-0154: 锁定 Lexer 深模式的小调用栈行为

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-154` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0129、SPEC-0150、SPEC-0153 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 压力集成测试、Architecture |
| 语言语义变更 | 否；只锁定现有迭代式 Scanner 对调用者栈大小不敏感的实现边界 |

## 1. Goal

完成后，Lexer 必须从 64 KiB 调用线程完成 4,096 层 string/interpolation mode、16,384 层
interpolation brace、4,096 层未终止 mode 与 4,096 项多字节诊断流的双运行验证，证明源码深度
和诊断数量不会转化为递归调用栈深度。

## 2. 范围与需求

- 在显式 64 KiB 调用线程内闭合 4,096 层 string/interpolation mode，保持零诊断与 16,386 个
  lexeme。
- 在同一调用线程内平衡单个 interpolation 中的 16,384 层 brace，保持零诊断与 32,774 个
  lexeme。
- 4,096 层未终止 mode 必须保持 8,194 个 lexeme，并只报告最内层一个 L0005。
- 4,096 个连续多字节非法 scalar 必须保持 4,097 个 lexeme 与 4,096 个 L0001。
- 四个源码各执行两次生产 Lexer；共享 helper 继续验证 source identity、连续完整 byte 覆盖、
  唯一 EOF、diagnostic primary / label Span 与完整公开产物确定性。
- 小调用线程启动或 join 失败、Lexer panic、数量漂移或非确定结果必须使测试失败。
- 不增加 Scanner worker、递归预算、生产测试钩子、依赖、语言语义或 wall-clock 阈值。

## 3. 非目标

- 不改变 Scanner 的 `Vec<Mode>`、字符分类或错误恢复实现。
- 不把 64 KiB 请求值解释为操作系统实际提交内存，也不测试平台线程调度。
- 不重复断言 SPEC-0150 已覆盖的每一个 token kind 与精确 Span；本 Spec 增加调用栈维度。
- 不测试 OOM、分配器失败或无限输入。

## 4. 验收标准

- [x] 四类 Lexer 压力源码均从显式 64 KiB 调用线程执行。
- [x] 深闭合 mode 与 brace case 双运行后零诊断且 lexeme 数量精确。
- [x] 未终止 mode 与大诊断流的诊断码和数量精确。
- [x] 八个 Lexer 产物的公开不变量与完整确定性验证通过。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 更新后的压力 target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

直接扩展既有 `lexer_stress_matrix` integration target，复用 SPEC-0150 的规模常量、源码形状与
`lex_source_twice` / `validate_lexed` 公共产物校验。新增测试只用 `std::thread::Builder` 设置
调用线程栈，不访问 Scanner 私有 mode stack；生产 Lexer 仍在该小线程直接执行，因此任何把
mode 或 brace 处理改为随源码深度递归的回归都会使测试失败。

## 6. 实施计划

1. [x] 审计 Lexer 小调用栈覆盖 → 验证：深模式压力已有，但全部位于普通测试线程。
2. [x] 扩展 Lexer 小栈压力矩阵 → 验证：四个源码、八个公开 Lexer 产物。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：更新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0154`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer 深模式小调用栈矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify iterative lexer stack use (SPEC-0154)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test lexer_stress_matrix --locked --offline` 通过：6 passed，
  0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test lexer_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：459 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
