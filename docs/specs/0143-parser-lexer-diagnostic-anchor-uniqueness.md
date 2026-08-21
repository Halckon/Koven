# SPEC-0143: 锁定 Parser 的 Lexer diagnostic anchor 唯一性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-143` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0006–0009、SPEC-0065、SPEC-0129、SPEC-0135–0142 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery、engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只拒绝重复复用生产 Lexer anchor 的内部诊断流 |

## 1. Goal

完成后，Parser 的 diagnostic/lexeme 双向契约从“至少一次”收紧为“恰好一次”：同一 lexeme
anchor 不能被第二条 Lexer diagnostic 复用，避免重复 poison 或 owner 根因进入最终输出。

## 2. 范围与需求

- 在 SPEC-0142 的覆盖位图设置前检查 anchor index 是否已占用；重复占用立即返回
  `ParserInternalError::InvalidLexemeStream`。
- 从 L0001 unexpected character 与 L0004 unterminated string 两份独立双运行 Lexer 产物分别
  复制唯一生产 diagnostic，覆盖 poison anchor 与 owner anchor，共验证 4 个正常 Lexer 产物。
- 两类产物保留 source identity、lexeme 结构及两个完全相同的生产 diagnostic。
- recovery index 与 expression、declaration、block、file 四入口对每类各执行两次，共验证
  20 个确定的 `InvalidLexemeStream`。
- 不改变合法 Lexer / Parser 产物、diagnostic 内容、语言语义或既有复杂度。

## 3. 非目标

- 不改变 diagnostic catalog、排序或去重策略；非法重复输入直接作为内部流错误拒绝。
- 不禁止不同 anchor 上出现相同 code，也不禁止同一源码区域内不同根因使用不同 anchor。
- 不修改 Parser grammar、AST、用户错误恢复或公开 API。
- 不新增随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 第二条 diagnostic 不能复用已覆盖 anchor index。
- [x] L0001 poison 与 L0004 owner 两类重复均从独立双 Lexer 产物派生，共验证 4 个正常产物。
- [x] 两类输入保持 lexeme 结构有效，并精确保留两个相同 code。
- [x] recovery index 与四入口双运行拒绝全部输入，共验证 20 个精确错误。
- [x] SPEC-0140–0142 的关联矩阵与合法 Lexer / owner 测试保持通过。
- [x] `lang-frontend` library 单元测试增至 31 项。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

继续复用等长布尔覆盖向量；`diagnostic_anchor_index` 成功后先读取位值，已设置则失败，未设置
才标记。本变更不增加额外遍历、分配或渐近复杂度。

## 6. 实施计划

1. [x] 审计重复 anchor 路径 → 验证：重复 diagnostic 当前会重复进入输出。
2. [x] 物化 Spec 与唯一性校验 → 验证：复用既有覆盖位，无新增扫描。
3. [x] 增加两类 test-only corpus 与五消费者双错误矩阵 → 验证：4 个 Lexer 产物、20 个错误。
4. [x] 运行直接相关窄验收 → 验证：31/31，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0143`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | anchor 唯一性、重复诊断矩阵、Architecture 与完成记录 | `test(frontend): reject duplicate lexer diagnostic anchors (SPEC-0143)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：31 passed，0 failed / ignored /
  measured / filtered。
- `cargo test -p lang-frontend --test lexer --test parser_lexical_owner_matrix --locked --offline`：
  21 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 435 passed，0 failed / ignored / measured / filtered。
