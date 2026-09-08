# SPEC-0073: 锁定 Lexer 固定词与符号边界矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-073` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25 词法规范](../guides/v0.34-pre-restructure/02-lexical-spec.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0006、SPEC-0068 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 生产 Lexer 集成测试、Architecture |
| 语言语义变更 | 否；只补强固定词边界、最长匹配与注释优先级证据 |

## 1. Goal

完成后，生产 Lexer 的 42 个硬关键字、11 个未来保留字、2 个软词及 40 个普通固定符号，不再
只由独立 spelling 和代表性边界间接证明；ASCII 标识符首/续字符类别与任意相邻符号 spelling
均由确定性矩阵验证分类、最长匹配、Span、完整字节覆盖和唯一 EOF。

## 2. 范围与需求

- 对 55 个固定词分别添加 `x` / `_` 前缀、`x` / `_` / `0` 后缀和首字母大写变体，共执行
  330 个 case；每例必须形成唯一普通 `Identifier`，且不得把关键字/保留字前缀误分类。
- 对 40 个非词复合固定符号执行 40 × 40 相邻 spelling 笛卡尔积；排除 4 个从源码起点形成
  注释 opener 的组合后，1,596 个 case 的首 token 必须是所有可匹配 spelling 中最长者，并
  保留精确 `[0, spelling.len())` Span。
- 对 `!in` / `!is` 穷举 63 个 ASCII identifier continuation，必须回退为 `!` + Identifier；
  再验证 EOF / trivia / punctuation 边界形成复合 token。`as?` 后跟全部 63 个 continuation
  仍保持 `as?` 复合 token。该部分共执行 199 个 case。
- 单独验证 `//` 与 `/* ... */` 先于 `/`、`/=`、`*` 匹配。全部矩阵合计 2,127 个生产 Lexer
  case，每例验证连续、同 source、非空的 lexeme 覆盖和唯一 EOF。
- 不增加依赖、随机输入或生产代码；若矩阵发现实现缺陷，只做直接修复并保留定向回归。

## 3. 非目标

- 不改变关键字、保留字、软词、符号集合、标识符字符集或注释语义。
- 不替代既有精确 token 枚举、错误码、字面量、模式恢复、确定性和 fixture 测试。
- 不把测试表提升为生产词表或机器可读公共协议。
- 不引入 property-testing、snapshot、正则或第三方依赖。

## 4. 验收标准

- [x] 330 个固定词变体均形成唯一 Identifier 且零诊断。
- [x] 1,596 个非注释符号对均按最大 spelling 形成首 token，并保留精确 Span。
- [x] 199 个复合词符号边界覆盖全部 ASCII continuation 与代表性合法终止边界。
- [x] 两种注释 opener 均优先于固定符号，所有 2,127 个 case 保持完整 lexeme 覆盖和唯一 EOF。
- [x] 新矩阵、既有 Lexer 测试及窄 Clippy 通过，无生产代码或依赖变化。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test。词边界表只列规范 spelling；符号表携带公开 `Symbol` 身份，测试
通过 `starts_with` 候选的最大长度计算首 token 期望，而不复制生产 scanner 的匹配顺序。
矩阵使用固定嵌套循环和闭式数量断言，不依赖随机种子、文件系统 fixture 或内部 scanner API。

## 6. 实施计划

1. [x] 建立固定词与普通符号对矩阵 → 验证：1,926 个 case。
2. [x] 建立复合词符号与注释优先级矩阵 → 验证：201 个 case。
3. [x] 运行新增、既有 Lexer 窄测试与窄 Clippy → 验证：23/23 tests，0 warnings。
4. [x] 运行一次 workspace 基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0073`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer 边界矩阵、Architecture 与完成记录 | `test(frontend): lock lexer boundary matrix (SPEC-0073)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test lexer_boundary_matrix --test lexer --locked --offline` | 通过 | 23/23；新增 2,127 个矩阵 case |
| `cargo clippy -p lang-frontend --test lexer_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、399 tests、CLI build；0 failed / ignored / measured / filtered |
