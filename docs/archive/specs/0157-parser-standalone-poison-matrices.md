# SPEC-0157: 扩展 Parser 独立 lexical poison 变换矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-157` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0081、SPEC-0083、SPEC-0088–0089、SPEC-0111、SPEC-0114、SPEC-0142、SPEC-0156 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件与独立入口 lexical-poison insertion / replacement 测试、Architecture |
| 语言语义变更 | 否；只把现行 L0007 / L0008 纳入已建立的独立可恢复 poison 矩阵 |

## 1. Goal

完成后，完整文件与 expression、declaration、block 三个独立 Parser 入口的逐 token replacement
和逐 gap insertion 矩阵，除 L0001 / L0002 外还必须覆盖可独立恢复的 L0007 非法字符字面量与
L0008 非法数字，并保持既有总性、确定性、Span、typed root 和完整文件 sentinel 证据。

## 2. 范围与需求

- 在共享 poison corpus 增加闭合但包含两个 scalar 的 `'ab'` 与带非法 suffix 的 `1e3`，分别由
  生产 Lexer 精确产生一个 L0007 / L0008 及覆盖全部 poison 文本的 primary Span。
- 完整文件 replacement 保持 22 个 grammar case、396 个显著 token slot，执行 396 × 4 = 1,584
  个 mutation；insertion 保持 418 个 token gap，执行 418 × 4 = 1,672 个 mutation。
- 独立入口 replacement 保持 12 个 case 与 240 个 slot，执行 240 × 4 = 960 个 mutation；
  insertion 保持 252 个 gap，执行 252 × 4 = 1,008 个 mutation。
- 四个 target 合计执行 5,224 个 mutation；每种 poison 分别执行 1,306 次，并继续双 Lexer、双
  Parser 验证公开产物。
- code mode 中新增 poison 必须产生精确目标 code / Span；落入 string text 或替换 lexical owner
  时沿用既有分层，只锁定 Scanner 真实行为、阶段总性与确定性。
- 完整文件非 owner replacement 和全部 insertion 继续保留后置 `val sentinel = 0`；独立入口
  继续要求 typed root 可解引用。
- 不修改 Lexer 规则、Parser grammar、诊断目录、公开 API 或依赖；发现缺陷时只修复直接根因。

## 3. 非目标

- 不把终止扫描的 L0003、L0004、L0005 或 terminal L0006 混入可继续 mutation corpus。
- 不重复 SPEC-0156 的 string-owned L0006 压力矩阵，也不组合多个 poison。
- 不固定每个 mutation 的 Parser 诊断集合或恢复 AST 细节；精确领域测试继续负责语义形状。
- 不新增随机 fuzzing、wall-clock 阈值或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 共享 poison corpus 精确包含 L0001、L0002、L0007、L0008 四类独立 poison。
- [x] 四个 insertion / replacement target 共执行 5,224 个 mutation，每类 poison 1,306 次。
- [x] code-mode L0007 / L0008 均精确锚定插入或替换文本，无阶段内部错误。
- [x] Lexer / Parser 双运行不变量、确定性、typed root 与完整文件 sentinel 分层保持通过。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 四个更新 target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

只扩展 tests 私有 `LEXICAL_POISONS` 单一真源，并同步四个矩阵的长度、分层与 mutation 总数断言。
poison 文本两侧继续由矩阵插入空格，避免与相邻 token 合并；字符串模式与 lexical-owner slot
继续走现有分支，不为测试增加生产钩子或复制 Scanner 分类算法。

## 6. 实施计划

1. [x] 审计独立可恢复 Lexer poison 与四个 mutation 矩阵 → 验证：确认只缺 L0007 / L0008。
2. [x] 扩展共享 corpus 与四个矩阵计数 → 验证：5,224 个 mutation，四类各 1,306 次。
3. [x] 运行四个直接相关 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0157`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | L0007 / L0008 mutation 矩阵扩展、必要修复、Architecture 与完成记录 | `test(frontend): expand standalone poison matrices (SPEC-0157)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test parser_lexical_poison_replacement_matrix --test parser_lexical_poison_insertion_matrix --test parser_entry_lexical_poison_replacement_matrix --test parser_entry_lexical_poison_insertion_matrix --locked --offline` 通过：4 passed，0 failed /
  ignored / measured / filtered；四个 target 合计执行 5,224 个 mutation。
- `cargo clippy -p lang-frontend --test parser_lexical_poison_replacement_matrix --test parser_lexical_poison_insertion_matrix --test parser_entry_lexical_poison_replacement_matrix --test parser_entry_lexical_poison_insertion_matrix --locked --offline -- -D warnings` 通过：
  0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：462 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
