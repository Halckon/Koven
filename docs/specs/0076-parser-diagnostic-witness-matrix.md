# SPEC-0076: 建立 Parser 已发布诊断 witness 矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-076` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0014、SPEC-0069、SPEC-0075 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口集成测试、必要诊断修复、Architecture |
| 语言语义变更 | 否；只建立已发布 Parser 诊断的可触达性和确定性证据 |

## 1. Goal

完成后，生产目录中 `L0009`–`L0078` 的每个现行 Parser 诊断码都有一个由公开 Parser 入口
实际产生的、Lexer-clean 的最小 witness；已退役但为兼容保留的 `L0016` 被明确排除并证明
矩阵不会重新发出，防止诊断常量失效、误复用或只有私有路径能触达。

## 2. 范围与需求

- 建立 expression、declaration、block、file 四类 witness，共覆盖 `L0009`–`L0078` 中除
  `L0016` 外的 69 个互异 Parser 诊断码。
- witness 码集合必须与生产 `diagnostic::codes::ALL` 对应区间精确一致；任何遗漏、重复、越界
  或新增码均使测试失败。
- 每个 witness 先经生产 Lexer 验证零词法诊断，再调用指定公开 Parser 入口；目标码必须恰好
  出现一次，所有诊断 Span 必须属于当前 source 且落在源码边界内。
- 同一 `LexedFile` 重复解析两次并比较公开 `Debug` 产物；69 个 case 共执行 138 次生产解析。
- 矩阵所有实际诊断均不得出现已退役 `L0016`；该码继续保留在生产目录，不改变兼容事实。
- 不增加依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不替代各领域测试对消息、精确 primary / label Span、AST 恢复形态和诊断顺序的断言。
- 不覆盖 Lexer `L0001`–`L0008` 或 Phase 2 `L0079` 之后的语义诊断。
- 不把一个 witness 当作对应错误码全部触发条件的穷举证明。
- 不重新启用 `L0016`，不修改诊断编号或 guide。

## 4. 验收标准

- [x] 69 个 witness 互异覆盖现行 Parser 码，和生产目录精确对齐，仅排除已退役 `L0016`。
- [x] 69 个 Lexer-clean case 共执行 138 次解析，无内部错误且公开产物逐例确定一致。
- [x] 每个目标码恰好出现一次，所有诊断 Span 合法，整个矩阵不发 `L0016`。
- [x] 矩阵发现的生产缺陷有最小修复和定向断言；本矩阵未发现生产缺陷，未改生产代码。
- [x] 新矩阵、四个公开入口既有套件及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，用静态 `Witness { code, entry, source }` 表声明触发输入。入口适配器
只返回公开 `Debug` 指纹与诊断指纹，不访问 Parser 私有常量或状态；码集合直接与公开生产目录
交叉核对。精确语义仍由现有分领域测试负责，矩阵只建立目录到公开可执行路径的一一 witness
契约。

若 witness 无法触达目录中的现行码，先判断是测试输入错误、实现退化还是诊断确已退役；不得
为了让矩阵变绿而从无关路径伪造诊断。

## 6. 实施计划

1. [x] 审计现有 delimiter / recovery 矩阵与诊断覆盖 → 验证：确认 delimiter 矩阵已充分，缺少集中诊断可触达性契约。
2. [x] 建立 69 个诊断 witness → 验证：目录集合、Lexer-clean、目标码计数、Span 与 138 次确定性解析。
3. [x] 运行窄测试并最小修复实际缺陷 → 验证：124/124，窄 Clippy 0 warnings；未发现生产缺陷。
4. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0076`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 诊断 witness 矩阵、必要修复、Architecture 与完成记录 | `test(frontend): witness parser diagnostics (SPEC-0076)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_diagnostic_witness_matrix --test parser_expression --test parser_declaration --test parser_block --test parser_file --locked --offline` | 通过 | 124/124；69 个 Lexer-clean witness、138 次生产解析 |
| `cargo clippy -p lang-frontend --test parser_diagnostic_witness_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、406 tests、CLI build；0 failed / ignored / measured / filtered |
