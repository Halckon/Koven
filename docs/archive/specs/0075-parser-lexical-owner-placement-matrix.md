# SPEC-0075: 覆盖 Parser lexical owner 语法位置矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-075` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0069、SPEC-0074 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser lexical-owner 恢复、集成测试、Architecture |
| 语言语义变更 | 否；只增加恢复总性证据并修复矩阵实际发现的内部缺陷 |

## 1. Goal

完成后，完整、可恢复和 EOF 截断的 string / interpolation 等 lexical owner 能安全出现在
声明名称、类型、参数、继承、成员、enum variant 和表达式等代表性语法位置；完整文件入口
始终返回确定性 AST / 用户诊断，不因恢复从 owner 中部开始而返回内部错误或吞掉后续顶层声明。

## 2. 范围与需求

- 建立 16 个代表性语法位置，覆盖顶层声明、修饰符之后、变量名称 / 类型 / initializer、函数
  名称 / 参数名称 / 参数类型 / 返回类型、class 名称 / 字段名称 / 字段类型 / supertype、成员、
  enum variant 和调用实参。
- 把完整 string、完整 interpolation、含非法 escape 的闭合 string、换行终止 string 投放到每个
  位置，共 64 个可继续解析 case；每例追加合法顶层 `val after = 1` 并证明恢复后仍能识别。
- 把 EOF 截断 string、EOF 截断 interpolation、EOF 截断 escape、EOF 截断 char 和 EOF 截断
  block comment 投放到同一批位置前缀，共 80 个 terminal case。
- 每例验证生产 Lexer 的连续 byte 覆盖、source identity、唯一 EOF、预期词法错误码；复用同一
  `LexedFile` 完整解析两次并比较公开 `Debug` 产物，任何内部错误或 panic 都使测试失败。
- 不增加依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不穷举全部 grammar production、嵌套深度或 lexical fragment；矩阵只锁定 owner-aware 恢复位置。
- 不替代既有精确 AST / Span 单元测试、四入口库存矩阵、fixture 或 fuzzing。
- 不改变 Lexer 分段、错误码含义、Parser AST 形态、guide 或 Phase 2 语义检查。
- 不为通过矩阵而接受原本非法的源码。

## 4. 验收标准

- [x] 16 个位置与 4 个可继续 owner 形成 64 个 case，后续 sentinel 声明全部存活。
- [x] 16 个位置与 5 个 terminal owner 形成 80 个 case，均完整消费到唯一 EOF。
- [x] 144 个 case 各解析两次，共 288 次解析无内部错误且公开产物逐例确定一致。
- [x] 矩阵发现的恢复缺陷有最小生产修复和定向断言；若未发现则明确记录。
- [x] 新矩阵及受影响既有 Parser 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以固定 `prefix + owner + suffix` 表生成可继续解析源码，并以相同
prefix 生成 EOF terminal owner。测试只使用公开 Lexer / Parser / AST API；确定性指纹沿用公开
`Debug`，不读取 Parser 私有状态。

若出现 `ParserInternalError::InvalidLexemeStream`，只修正最早错误恢复起点，使既有
owner-aware recovery 从 opener 开始消费完整 lexical owner；不得放宽语法或复制一套 owner
扫描逻辑。

## 6. 实施计划

1. [x] 审计既有 lexical-owner 恢复测试 → 验证：确认默认参数、调用实参与文件根已有定向覆盖，名称 / 类型 / class-family 等位置缺少系统矩阵。
2. [x] 建立 16 ×（4 + 5）位置矩阵 → 验证：64 个可继续 case、80 个 terminal case，288 次重复解析。
3. [x] 运行窄测试并最小修复实际缺陷 → 验证：91/91，窄 Clippy 0 warnings；classifier 名称完整 string 回归通过。
4. [x] 运行一次 workspace 标准基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0075`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lexical-owner 位置矩阵、必要恢复修复、Architecture 与完成记录 | `test(frontend): exercise lexical owner placements (SPEC-0075)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_lexical_owner_matrix --test parser_declaration --test parser_class_family --test parser_call_argument --test parser_file --locked --offline` | 通过 | 91/91；144 case、288 次矩阵解析与 classifier 名称定向回归 |
| `cargo clippy -p lang-frontend --test parser_lexical_owner_matrix --test parser_class_family --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、405 tests、CLI build；0 failed / ignored / measured / filtered |
