# SPEC-0151: 建立 Parser 大平坦列表与恢复压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-151` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0117–0119、SPEC-0128–0129、SPEC-0135、SPEC-0150 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口集成测试、Architecture |
| 语言语义变更 | 否；只锁定大平坦合法列表与重复错误恢复的资源边界行为 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口都必须对 4,096 个合法
平坦元素保留精确 AST 数量，并对 4,096 个独立错误区域产生一一对应、源码有序的诊断与错误
节点；全部大输入经双 Lexer / 双 Parser 验证，不 panic、漂移、吞元素或诊断爆炸。

## 2. 范围与需求

- expression 入口解析含 4,096 个位置实参的单一 call；root 必须是 `Expression::Call`，实参数量
  精确为 4,096，且零诊断。
- declaration 入口解析含 4,096 个具名值参数的函数；root 必须是 `Item::Function`，参数数量
  精确为 4,096，且零诊断。
- block 入口解析含 4,096 个局部 `val` 的单一 block；root 的 `elements` 精确为 4,096，且零诊断。
- file 入口解析 4,096 个换行分隔的顶层 `val`；`roots` 精确为 4,096，且零诊断。
- expression 的 4,096 个 `++` unsupported suffix、declaration 参数表的 4,096 个空 element、
  block 的 4,096 个 `@` element 与 file 中各由后续 `val` starter 分隔的 4,096 个 `@` region，
  必须分别产生精确 4,096 个 L0015 / L0024 / L0029 / L0017，primary Span 按源码严格递增。
- declaration 错误恢复保留唯一尾参数；block 为每个错误区保留一个 `Statement::Error`；file
  为每个错误区保留一个 `Item::Error`，并继续保留紧随其后的 4,096 个 sentinel 声明。
- 8 个压力源码分别运行生产 Lexer 与对应 Parser 两次，锁定连续完整覆盖、唯一 EOF、
  source-local AST / diagnostic Span、有效 root 与完整公开产物确定性。
- 不增加生产依赖、公开 API、语言语义或 wall-clock 阈值；发现缺陷时只修复直接根因。

## 3. 非目标

- 不改变 Parser 的 1,024 递归预算；本 Spec 只测试不增加递归深度的平坦输入。
- 不替代各语法 suite 的精确小型 Span / recovery 测试或内部 N→2N inspection 计数。
- 不声称固定测试规模等于任意输入上限，也不测试 OOM。

## 4. 验收标准

- [x] 四个公开入口分别保留 4,096 个合法元素，零诊断且 AST 数量精确。
- [x] 四个公开入口分别恢复 4,096 个错误区域，目标诊断码和数量精确。
- [x] declaration 尾参数、4,096 个 block error statement、4,096 组 file error root / sentinel 保留。
- [x] 全部 16,384 条目标诊断的 primary Span 在各自源码中严格递增且 source-local。
- [x] 8 个源码的双 Lexer / 双 Parser 产物一致，连续覆盖、唯一 EOF 与 typed root 有效。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新矩阵测试及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增单一 Parser integration test target，复用 `parser_test_assertions` 的四类双运行入口与共享
Lexer / AST / diagnostic 公产物校验。源码由确定性闭式循环生成；测试只检查公开 AST、诊断与
Span，不暴露 Parser 私有计数器或复制 grammar 实现。

## 6. 实施计划

1. [x] 审计 Parser 大输入覆盖 → 验证：领域测试有局部长链证据，但四入口没有统一 4,096 项
   合法 / 错误恢复矩阵。
2. [x] 建立四入口合法与错误压力矩阵 → 验证：8 个源码、32,768 个受验收元素 / 错误区。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0151`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 四入口大平坦压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress parser flat recovery (SPEC-0151)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 首轮窄测试的合法矩阵通过，错误矩阵在 file case 失败：连续 `@` 行被正确合并为一个未知
  region，证明换行不是 file recovery boundary。测试改为每个 `@` 后放置真实 `val` starter，
  未修改生产行为；完整窄测试随后重跑。
- `cargo test -p lang-frontend --test parser_stress_matrix --locked --offline` 重跑通过：2 passed，
  0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：455 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
