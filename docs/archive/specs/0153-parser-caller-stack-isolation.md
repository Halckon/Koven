# SPEC-0153: 锁定 Parser 四入口的调用者栈隔离

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-153` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0007–0009、SPEC-0014、SPEC-0136、SPEC-0152 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口集成测试、Architecture |
| 语言语义变更 | 否；只锁定现有固定 Parser worker 与调用者栈隔离契约 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口都必须从 64 KiB 调用线程
执行各自的递归边界成功与超限 case，并保持与普通调用线程相同的成功产物或精确资源错误，
从而防止任一入口绕过固定 Parser worker、重新依赖调用者栈大小。

## 2. 范围与需求

- expression 入口在 64 KiB 调用线程中解析 511 层 group，并在 512 层返回精确
  `NestingLimitExceeded { limit: 1024 }`。
- declaration 入口在同等调用线程中解析 1,023 层 generic type，并在 1,024 层返回相同资源错误。
- block 与 file 入口分别解析 1,024 层 nested block，并在 1,025 层返回相同资源错误。
- 四个成功源码均执行双 Lexer / 双 Parser，保持零诊断、有效 typed root、source-local AST /
  diagnostic Span 与完整公开产物确定性。
- 四个失败源码均执行双 Lexer / 双 Parser，错误类型、limit 和重复运行结果精确一致。
- 每个公开入口使用独立小调用线程，线程启动失败、调用者线程 panic 或 Parser 结果漂移都必须
  使测试明确失败。
- 不暴露固定 worker 栈大小为公共 API，不增加生产测试钩子、依赖、语言语义或耗时阈值。

## 3. 非目标

- 不改变固定 Parser worker 的 32 MiB 栈或 1,024 单位递归预算。
- 不以测试线程请求值推断操作系统实际保留、提交或调度的栈内存。
- 不测试线程创建失败注入、OOM、平台线程实现或 Parser panic 恢复。
- 不替代 SPEC-0152 对九类语法形状的精确普通调用线程边界矩阵。

## 4. 验收标准

- [x] 四个公开入口均从显式 64 KiB 调用线程执行。
- [x] 四个最后成功边界源码双运行后零诊断且完整公开产物有效、确定。
- [x] 四个第一拒绝边界源码双运行后精确返回 limit 1,024。
- [x] 小调用线程启动、join 与 Parser 结果任一失败均被测试观察。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新矩阵测试及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增单一 integration test target。测试用 `std::thread::Builder` 为每个入口创建独立 64 KiB
调用线程；线程内复用 `parser_test_assertions` 的双 Lexer、四类双 Parser 成功入口和 typed
内部错误断言。源码深度直接复用 SPEC-0152 已固定的公开边界，不访问生产私有常量或线程名，
因此实现若删除任一公开入口的隔离 worker，边界递归将重新落到小调用栈并使矩阵失败。

## 6. 实施计划

1. [x] 审计四入口调用者栈覆盖 → 验证：仅发现一个浅层 expression/lambda 小栈 case。
2. [x] 建立四入口小调用栈边界矩阵 → 验证：8 个源码在四个独立小栈线程中双运行。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0153`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 四入口调用者栈隔离矩阵、必要修复、Architecture 与完成记录 | `test(frontend): verify parser stack isolation (SPEC-0153)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_stack_isolation_matrix --locked --offline` 通过：
  1 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_stack_isolation_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：458 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
