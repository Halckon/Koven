# SPEC-0111: 强化完整文件 mutation 矩阵 Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-111` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0079–0084、SPEC-0099、SPEC-0103–0110 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 六个完整文件 mutation integration tests、既有 frontend adversarial 调用方、共享 frontend matrix test support、Architecture |
| 语言语义变更 | 否；只补齐既有 mutation corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，前缀截断、token 删除 / 重复 / 相邻交换及 lexical poison 替换 / 插入六个完整文件
矩阵的全部 4,301 个实际 source case，均执行两次生产 Lexer 与两次完整文件 Parser。

## 2. 范围与需求

- 保持 22-file corpus、1,373 个 UTF-8 前缀、396 个删除、792 个 poison 替换、396 个重复、
  836 个 poison 插入与 374 个相邻交换主 case 不变，共 4,167 个主要变异 / 前缀 case。
- 同时覆盖六个矩阵各 22 个独立 baseline / complete case（132 次）与 omission 的 2 个定向
  回归；实际合计 4,301 个 source case，不允许共享入口只强化主循环。
- 每个 source 执行两次生产 Lexer，共验收 8,602 个 Lexer 产物；两次均验证 source identity、
  连续完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span 与完整公开 `Debug`。
- 每个 source 继续执行两次完整文件 Parser，共验收 8,602 个 Parser 产物及其 AST、diagnostic、
  roots、directive Span 与完整公开产物确定性。
- 六个矩阵既有 UTF-8 scalar 边界、lexical mode、精确 token / poison Span、owner 分类、sentinel、
  诊断与恢复断言保持不变。
- 双 Lexer 逻辑只在 `frontend_matrix_assertions` 暴露一个共享入口，不在六个测试文件复制。
- 既有 `frontend_adversarial` 改为复用同一入口，避免同一 test crate 重复加载底层 helper；其
  324-case corpus 与既有双 Lexer / 双 Parser 行为不变。
- 不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 mutation 生成、Parser 恢复、AST、诊断、资源预算或 grammar。
- 不固定每个 EOF 截断位置的诊断码或 AST 恢复形态。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 4,167 个主 case、132 个 baseline / complete 与 2 个定向回归保持固定，合计 4,301。
- [x] 8,602 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] 8,602 个 Parser 产物继续满足 AST、diagnostic、roots / directive 与确定性不变量。
- [x] 六个矩阵与既有 frontend adversarial 调用方复用同一个双 Lexer 共享入口。
- [x] UTF-8、lexical mode、精确 token / poison、owner、sentinel 与恢复断言保持通过。
- [x] 全部 4,301 个 source case 无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 六个直接相关矩阵测试、共享调用方兼容测试与窄 Clippy 通过。
- [x] 修正共享调用方后，workspace 标准基线完整通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`frontend_matrix_assertions` 组合 SPEC-0103 的 `lexer_matrix_assertions`，提供创建 source map、
执行并验证两次 Lexer 的单一 `lex_source_twice`。六个调用方使用该入口替换本地 `SourceMap +
lex + validate_lexed`，首个确定产物继续供精确 mutation 断言和既有 `parse_file_twice` 使用。

## 6. 实施计划

1. [x] 审计六个 file mutation 矩阵 → 验证：确认 4,301 个实际 source case 均为单 Lexer、双 Parser。
2. [x] 建立共享双 Lexer 入口并迁移六个矩阵 → 验证：17,204 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：六矩阵 8/8、共享调用方 2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行 workspace 标准基线 → 验证：首次 `check` 暴露共享调用方问题，修正后
   从 `fmt` 开始完整重跑并全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0111`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 六个 file mutation 矩阵共享双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen file mutation lexer invariants (SPEC-0111)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 六个直接相关 integration tests | 通过 | 8/8；4,301 个 source case、17,204 个前端产物 |
| 同一组六个 integration tests 的窄 Clippy | 通过 | `-D warnings`；0 warnings |
| `frontend_adversarial` 兼容测试与窄 Clippy | 通过（修正后） | 首次暴露私有 re-export 与 duplicate module；统一共享入口后 2/2、0 warnings |
| 首次 workspace Cargo 基线 | 未通过 | `fmt` 通过；`check` 发现 `frontend_adversarial` 依赖被收窄的 test-support re-export，后续命令未执行 |
| 修正后 workspace Cargo 基线 | 通过 | 从 `fmt` 开始完整重跑；`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
