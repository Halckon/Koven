# SPEC-0074: 覆盖 Parser 完整词法片段库存

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-074` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0069、SPEC-0073 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立声明恢复、四个公开 Parser 入口集成测试、Architecture |
| 语言语义变更 | 否；修复阶段内部 string-owner 恢复契约并增加总性证据 |

## 1. Goal

完成后，生产 Lexer 的全部公开固定 token 变体及代表性 literal、trivia、string mode 和八类
词法错误，都能作为用户输入安全进入 expression、declaration、block、file 四个公开 Parser
入口；入口只返回确定性 AST/诊断或既定资源错误，不把合法 `LexedFile` 误判为内部损坏。

## 2. 范围与需求

- 建立 120 个互异片段：42 个硬关键字、11 个未来保留字、43 个 `Symbol`、13 个 identifier /
  数值后缀 / char / string / interpolation atom、4 类 trivia，以及分别产生 `L0001`–`L0008`
  的 7 个片段（`L0002` 由未来保留字覆盖）。
- 库存自检精确数量和字符串唯一性，并用生产 Lexer 验证 keyword/reserved/symbol/trivia family、
  连续完整 byte 覆盖、source identity、唯一 EOF，以及八个词法错误码全部实际出现。
- 每个片段分别进入独立 expression、独立 declaration、block element 和完整 file root 上下文，
  共执行 480 个 entry/case；复用同一 `LexedFile` 重复解析两次，共 960 次生产解析并比较公开
  `Debug` 骨架，任何普通用户片段返回内部错误都使测试失败。
- 修复矩阵发现的独立声明 string-owner 缺陷：合法完整 string 位于声明起点时，从
  `StringStart` 整体恢复为一个 Error Item 和一个覆盖完整 owner 的 `L0017`，不得先消费 opener
  后让 tail recovery 在 `StringEnd` 返回 `InvalidLexemeStream`。
- 不增加依赖、公开 API 或新诊断，不改变合法语法。

## 3. 非目标

- 不把 token 库存矩阵当作随机 fuzzing、语法接受矩阵或类型正确性证明。
- 不替代逐语义 AST、精确恢复、复杂度、fixture、SPEC-0068/0069 对抗组合测试。
- 不修改 Lexer、token 集合、AST 形态、诊断含义或 guide。
- 不引入 snapshot、property-testing、parser generator 或第三方依赖。

## 4. 验收标准

- [x] 120 个库存片段互异，覆盖全部公开固定 token family、四类 trivia 和 `L0001`–`L0008`。
- [x] 480 个 entry/case 均无内部错误或 panic，960 次解析的公开产物逐例确定一致。
- [x] 独立声明完整 string 回归产生唯一 `L0017`，Error Item 与诊断 Span 均覆盖完整源码。
- [x] 新矩阵、四个既有 Parser 入口套件及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以规范 spelling 表构造固定库存。block/file 使用包含后续合法声明的
wrapper，让片段进入真实 owner/恢复上下文；expression/declaration 使用独立入口原始片段。
确定性指纹沿用公开 `Debug` 结构，不访问 Parser 私有状态。

生产修复只改变独立非 file-mode 声明在首 token 为 `StringStart` 的错误恢复起点：调用既有
owner-aware `recover_declaration_region` 从 opener 消费整个多 lexeme string。其他 token 继续
保持原先的单 token 根与 tail 契约，完整文件仍使用既有 `DeclarationStops::FILE`。

## 6. 实施计划

1. [x] 建立并自检 120 个词法片段库存 → 验证：family、覆盖、EOF、`L0001`–`L0008`。
2. [x] 建立四入口矩阵并修复 string-owner 缺陷 → 验证：480 case / 960 parses 与精确回归。
3. [x] 运行新增及既有四入口窄测试与 Clippy → 验证：126/126，0 warnings。
4. [x] 运行一次 workspace 基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0074`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 四入口库存矩阵、恢复修复、Architecture 与完成记录 | `test(frontend): cover parser token inventory (SPEC-0074)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_expression --test parser_declaration --test parser_block --test parser_file --test parser_token_inventory --locked --offline` | 通过 | 126/126；480 entry/case、960 次矩阵解析与 string-owner 定向回归 |
| `cargo clippy -p lang-frontend --test parser_token_inventory --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、402 tests、CLI build；0 failed / ignored / measured / filtered |
