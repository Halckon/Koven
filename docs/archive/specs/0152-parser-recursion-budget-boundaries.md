# SPEC-0152: 锁定 Parser 递归预算的精确公开边界

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-152` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0007–0009、SPEC-0014、SPEC-0117–0119、SPEC-0135–0136、SPEC-0151 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口集成测试、Architecture |
| 语言语义变更 | 否；只锁定现有 1,024 单位实现预算映射到代表性源码形状后的公开边界 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口都必须在代表性递归语法
形状的最后可接受源码深度成功，并在相邻的第一拒绝深度确定性返回精确
`NestingLimitExceeded { limit: 1024 }`，从而阻止预算判断出现 off-by-one 或入口基线漂移。

## 2. 范围与需求

- expression 入口分别覆盖 alternating prefix、assignment、Elvis、group、generic type 与 function
  type 六类递归形状。
- prefix 与 group 的最后接受 / 第一拒绝深度必须为 511 / 512；assignment、Elvis、generic type
  与 function type 必须为 1,022 / 1,023。
- declaration 的嵌套 generic type 必须接受 1,023 层并拒绝 1,024 层。
- block 与 file function body 的嵌套 block 必须接受 1,024 层并拒绝 1,025 层。
- 九类成功边界源码执行双 Lexer / 双 Parser，保留合法 root、source-local AST / diagnostic Span、
  零诊断与完整公开产物确定性。
- 九类相邻失败源码执行双 Lexer 与双 Parser，精确返回相同的
  `NestingLimitExceeded { limit: 1024 }`。
- 不增加生产依赖、公开 API、语言语义或 wall-clock 阈值；发现缺陷时只修复直接根因。

## 3. 非目标

- 不改变 `MAX_RECURSION_DEPTH` 的 1,024 单位实现预算。
- 不把实现预算单位等同于统一的用户源码嵌套层数；不同语法形状可因调用路径而有不同边界。
- 不测试进程栈、OOM 或依赖特定机器的耗时上限。
- 不替代既有中等深度成功、明显超限失败或 Parser 私有 trial 预算测试。

## 4. 验收标准

- [x] 六类 expression 形状的最后接受与第一拒绝深度均被相邻 case 锁定。
- [x] declaration、block 与 file 三个入口的最后接受与第一拒绝深度均被相邻 case 锁定。
- [x] 九个成功边界源码双运行后零诊断，公开 Lexer / AST / root / Span 不变量有效且确定。
- [x] 九个失败边界源码双运行后精确返回 limit 1,024，错误类型和内容确定。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新矩阵测试及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增单一 Parser integration test target，复用 `parser_test_assertions` 的四类双运行入口、双 Lexer
校验与 typed 内部错误断言。成功与失败输入仅相差一层；测试检查公开结果，不访问 Parser 私有
递归计数器。边界数值来自公开入口的二分审计，最终测试不保留搜索循环或动态阈值推导，避免
实现漂移被测试自行接受。

## 6. 实施计划

1. [x] 审计四入口及递归语法族 → 验证：测得九类公开形状的相邻通过 / 拒绝边界。
2. [x] 建立四入口精确边界矩阵 → 验证：18 个源码双运行并精确断言结果。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0152`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 四入口递归预算精确边界矩阵、必要修复、Architecture 与完成记录 | `test(frontend): lock parser recursion boundaries (SPEC-0152)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 边界审计先以临时二分 probe 测量九类公开形状，随后删除 probe，并把每个固定阈值的相邻
  成功 / 失败 case 写入最终矩阵；测试不会动态推导并自行接受实现漂移。
- `cargo test -p lang-frontend --test parser_recursion_boundary_matrix --locked --offline` 通过：
  2 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_recursion_boundary_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：457 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
