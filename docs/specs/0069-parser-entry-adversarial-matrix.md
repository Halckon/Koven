# SPEC-0069: 覆盖独立 Parser 入口对抗矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-069` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0007、SPEC-0008、SPEC-0009、SPEC-0068 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 expression / declaration / block Parser 集成测试、Architecture |
| 语言语义变更 | 否；只增加公开入口的确定性与恢复证据 |

## 1. Goal

完成后，除完整文件入口外的三个公开 Parser 入口也由确定性短源码组合矩阵证明：任意矩阵
输入只产生受控 AST / 诊断或既定资源错误，不因用户源码返回内部 lexeme-stream 错误或 panic，
且重复运行结果完全一致。

## 2. 范围与需求

- 固定 16 个词法/语法前缀与 16 个后缀，分别交给 `parse_expression`、`parse_declaration`、
  `parse_block`，精确执行 768 个 entry/case 组合。
- 前后缀覆盖合法叶节点、声明、函数/类型、block/lambda、call/index、control-flow、string/
  interpolation、char/comment、Unicode、NUL 与不匹配 closer。
- 每个组合复用同一生产 Lexer 产物重复解析两次，并比较公开 `Debug` 骨架；该骨架包含稳定
  root typed ID、AST table 插入顺序与 source-owned span、诊断，不含 payload 地址或无序容器。
- 不增加依赖、随机输入、环境状态或生产代码；若发现阶段契约缺陷，仅做直接修复并增加定向
  回归。

## 3. 非目标

- 不替代已有逐语义精确 AST、错误码/span、owner recovery 或复杂度测试。
- 不重复 SPEC-0068 的完整文件根、package/import 和完整 lexeme 覆盖断言。
- 不改变 grammar、AST、诊断、公开 API 或语言 guide。

## 4. 验收标准

- [x] 测试精确断言 16 × 16 × 3 = 768 个组合，三个入口均实际执行。
- [x] 所有组合首次和重复解析均无内部错误或 panic，公开产物骨架逐字节一致。
- [x] 新测试运行时间保持适合常规 `cargo test`，不依赖随机数、文件系统或网络。
- [x] frontend 窄测试与一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增一个 integration test，以接收 `SourceMap` / `LexedFile` 并返回稳定字符串指纹的闭包复用
矩阵 runner。选择公开 `Debug` 是因为 `AstNode` 的既有稳定表示只暴露 source span 骨架，恰好
覆盖本 Spec 的 table 数量、顺序、范围与诊断确定性目标；payload 边与精确语言行为继续由既有
专用测试负责。

## 6. 实施计划

1. [x] 添加共享矩阵 runner 与三个入口用例 → 验证：新增 integration test。
2. [x] 处理发现的阶段契约缺陷（如有） → 验证：未发现新缺陷，无生产代码修改。
3. [x] 同步 Architecture、Spec 和索引 → 验证：文档与事实一致。
4. [x] 运行一次 workspace 基线并提交 → 验证：staged diff 单一且提交成功。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 三入口矩阵、Architecture 与完成记录 | `test(frontend): exercise parser entry matrices (SPEC-0069)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_adversarial --locked --offline` | 通过 | 1/1；768 个组合、1536 次解析，约 0.11 秒 |
| `cargo test -p lang-frontend --test parser_expression --test parser_declaration --test parser_block --locked --offline` | 通过 | 96/96 既有精确入口测试 |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、389 tests、CLI build；0 failed / ignored / measured / filtered |
