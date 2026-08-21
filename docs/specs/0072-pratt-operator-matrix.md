# SPEC-0072: 锁定 Pratt 运算符矩阵契约

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-072` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25 运算符层级](../guide/03-grammar-core.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强词法、语法与 Parser 测试验收 |
| 前置 Spec | SPEC-0007、SPEC-0069 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 公开 expression Parser 集成测试、Architecture |
| 语言语义变更 | 否；只补强既有 Pratt 优先级、结合性与诊断证据 |

## 1. Goal

完成后，现行 14 层 Pratt 运算符不再只由相邻层和代表性链式用例间接证明：所有表达式右操作
数中缀层两两组合、三个最高层与全部更低层、结合组变体和四个不结合组成员组合均由公开
Parser API 的结构或精确诊断矩阵锁定。

## 2. 范围与需求

- 对 11 个表达式右操作数中缀层的任意两个不同层，分别以高层在前和低层在前构造源码，执行
  110 个 case；根节点必须属于低优先级层，高优先级节点必须位于对应左 / 右子树。
- 将 postfix member、prefix `-`、cast `as` 分别与所有更低中缀层组合，并补齐三者之间的
  顺序，共执行 36 个 case。cast 后比较使用无歧义的 `>`；`T < ...` 按现行 `type_ref` EBNF
  开始泛型实参，不被测试误定义为比较。
- 对乘法、加法、`to`、`&&`、`||` 的全部同组成员组合验证左子树方向；对 6 种 assignment
  的 36 个有序组合、Elvis 与两个 cast 变体验证右 / 左结合方向，共执行 54 个 case。
- 对 range、membership/type-test、comparison、equality 四个不结合组执行 40 个成员笛卡尔
  组合；每例必须且只在第二个运算符的精确 byte span 产生一个 `L0012`。
- 全部 240 个 case 使用生产 Lexer 与公开 `parse_expression`，不增加依赖、随机输入或生产
  API；矩阵自身精确断言执行数量，防止静默漏项。

## 3. 非目标

- 不改变运算符集合、优先级、结合性、`type_ref` 歧义规则、AST 或诊断。
- 不复制生产 binding-power 数值；测试只按 guide 的高到低语义顺序声明代表性层。
- 不替代既有逐 operator payload、相邻层、恢复、资源预算和 fixture 测试。
- 不引入 property-testing、snapshot 或 parser generator 依赖。

## 4. 验收标准

- [x] 110 个不同中缀层双向组合均形成预期根与高优先级子树。
- [x] postfix、prefix、cast 与更低层及彼此之间的 36 个组合均形成预期 AST。
- [x] 54 个结合性 case 覆盖全部左结合组成员、全部 assignment 有序组合、Elvis 与 cast。
- [x] 40 个不结合组成员组合均产生唯一 `L0012`，主范围精确指向第二个运算符。
- [x] 新矩阵和既有 expression Parser 窄测试通过，无生产代码或依赖变化。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration test，以小型 `OperatorCase` 表描述源码拼写及公开 AST 枚举身份。优先级
断言只观察根和一层关键子树，不依赖私有 binding power；不结合断言只观察稳定错误码和 byte
span。所有表都以确定性嵌套循环执行，并由闭式数量断言证明没有零用例或漏跑分支。

## 6. 实施计划

1. [x] 建立跨层优先级与最高三层矩阵 → 验证：146 个结构 case。
2. [x] 建立结合性与不结合组矩阵 → 验证：94 个结构 / 诊断 case。
3. [x] 运行新增与既有 expression 窄测试 → 验证：57/57。
4. [x] 运行一次 workspace 基线并同步完成记录 → 验证：标准命令全部成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0072`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Pratt 矩阵、Architecture 与完成记录 | `test(frontend): lock Pratt operator matrix (SPEC-0072)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_operator_matrix --test parser_expression --locked --offline` | 通过 | 57/57；新增 240 个矩阵 case |
| workspace Cargo 基线 | 通过 | `fmt`、`check`、Clippy、395 tests、CLI build；0 failed / ignored / measured / filtered |
