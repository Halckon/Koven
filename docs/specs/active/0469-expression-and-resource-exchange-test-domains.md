# SPEC-0469：表达式与资源交换测试领域归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批测试职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0469` |
| 所属 Phase | Phase 1 parser与Phase 3 ownership测试组织 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0468](0468-lexer-and-call-recovery-test-domains.md)，输入35b3e2af |
| 影响范围 | tests/parser_expression.rs、ownership_resource_deinit.rs及领域子模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整领域

1. AST形状：concrete_payloads至contextual_keyword_value的完整表达式构造/类型引用/
   precedence/strict typed suffix域，移parser_expression/shapes.rs。
2. 诊断恢复：parser_diagnostic_codes至lexical_poison_postfix的完整恢复域，移
   parser_expression/recovery.rs；跨域parsed_case_with_diagnostics留根，唯一terminal
   string helper随其消费者迁移。不修改错误树、primary/label span或SourceId。
3. 资源交换：root_resource_swap至文件尾完整replace/swap域，移
   ownership_resource_deinit/exchange.rs；复用原inspected_plans/point与双Single/Unit oracle。

所有#[test]/函数/fixture/断言正文逐字节保留；根共享helper/import/harness不复制，子模块
私有显式path与use super::*，FQN只加域层。无生产代码/规范变化，无新allow/ignore/pub/
wrapper，不删除34合理保留。两根及新模块均小于1000；只删对应baseline，其他policy等值。

## 2. 验收

- [ ] R1：完整迁移块和剩余根机械保全，原test leaf与新增域FQN一一核对无漏重。
- [ ] R2：原完整target基线→逐片定向复验→可构建本地提交，不逐片push、不伪造RED。
- [ ] R3：整组完整frontend/Clippy/fmt/policy/docs/尺寸/空白，独立精确提交diff审查。
- [ ] R4：Architecture/账本同步，集中push同#439，精确新头CI另验，active/Draft不提前归档合入。

证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

待逐片记录。
