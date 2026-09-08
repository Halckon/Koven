# SPEC-0155: 建立 Parser 大规模 lexical-owner 压力矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-155` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0014、SPEC-0075、SPEC-0095、SPEC-0135、SPEC-0151、SPEC-0154 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 四个公开 Parser 入口集成测试、Architecture |
| 语言语义变更 | 否；只锁定大量合法与局部恢复 string/interpolation owner 的现有行为 |

## 1. Goal

完成后，expression、declaration、block 与 file 四个公开 Parser 入口都必须保留 4,096 个合法
string/interpolation owner，并对 4,096 个空 interpolation 各产生一个源码有序 L0009 与对应
Error expression；全部大输入经双 Lexer / 双 Parser 验证，不 panic、吞 owner、错位恢复或产生
非线性级联诊断。

## 2. 范围与需求

- expression 入口解析含 4,096 个 `"${x}"` 实参的单一 call；declaration 入口在变量 initializer
  中解析相同 call；两者均保留 4,096 个实参。
- block 入口解析 4,096 个以 string 初始化的局部 `val` element；file 入口解析 4,096 个以
  string 初始化的顶层 `val` root。
- 四个合法源码各保留 4,096 个单 interpolation `Expression::String`，其 inner expression 为
  `Expression::Name`，且零诊断。
- 对应四个恢复源码把每个 owner 改为 `"${}"`；每个 owner 必须保留单 interpolation string 与
  inner `Expression::Error`，并精确产生一个 L0009。
- 四个恢复源码的 16,384 条 L0009 primary Span 必须在各自源码中严格递增；call argument、
  block element、file variable root 数量不得因局部错误减少。
- 8 个源码分别执行两次生产 Lexer 与对应 Parser，锁定连续完整覆盖、唯一 EOF、source-local
  AST / diagnostic Span、有效 typed root 与完整公开产物确定性。
- 不增加生产依赖、公开 API、语言语义、wall-clock 阈值或测试专用生产计数器；发现缺陷时只
  修复直接根因。

## 3. 非目标

- 不替代小型 lexical-owner placement / terminal-owner 精确 Span 矩阵。
- 不把空 interpolation 改为 Lexer 错误；其现行根因仍是 Parser 的 L0009。
- 不测试 EOF terminal owner、递归预算、OOM 或所有字符串内容组合。
- 不以固定 4,096 规模声明语言输入上限。

## 4. 验收标准

- [x] 四入口分别保留 4,096 个合法 owner，目标 root / element / argument 数量精确且零诊断。
- [x] 四入口分别恢复 4,096 个空 interpolation，保留 string 与 inner Error AST。
- [x] 全部 16,384 条 L0009 数量、错误码与源码顺序精确。
- [x] 8 个源码的双 Lexer / 双 Parser 公开产物不变量与确定性验证通过。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新矩阵测试及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增单一 Parser integration test target，复用 `parser_test_assertions` 四类双运行入口及共享 Lexer /
AST / diagnostic 校验。源码由确定性闭式循环生成；测试只遍历公开索引式 AST，逐个确认 string
的唯一 interpolation child 是 Name 或 Error，并验证四入口拥有者数量，不访问 Parser 私有
`LexicalRecoveryIndex` 或计数器。

## 6. 实施计划

1. [x] 审计 owner-rich 公开压力覆盖 → 验证：小型 placement 最高 144 case，私有线性比较仅
   16 / 32 events，四入口 4,096 元素压力不含 interpolation owner。
2. [x] 建立四入口合法与空插值恢复压力矩阵 → 验证：8 个源码、32,768 个 owner。
3. [x] 运行直接相关窄测试和窄 Clippy → 验证：新 target 通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0155`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 四入口 owner-rich 压力矩阵、必要修复、Architecture 与完成记录 | `test(frontend): stress parser lexical owners (SPEC-0155)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 首轮窄测试确认 expression / declaration 的 4,096 个 empty interpolation 可完整恢复；block
  case 失败是因为换行不能分隔相邻裸表达式。测试载体改为规范允许的局部 `val` element 后
  通过，未修改生产行为；临时定位探针已删除。
- `cargo test -p lang-frontend --test parser_owner_stress_matrix --locked --offline` 重跑通过：
  2 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test parser_owner_stress_matrix --locked --offline -- -D warnings`
  通过：0 warnings。
- `cargo fmt --all -- --check` 通过。
- `cargo check --workspace --all-targets --locked --offline` 通过。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通过：0 warnings。
- `cargo test --workspace --all-targets --locked --offline` 通过：461 passed，0 failed / ignored /
  measured / filtered。
- `cargo build -p lang-cli --locked --offline` 通过。
