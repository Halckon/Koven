# SPEC-0475：Fixture发现、执行与合同测试归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批fixture职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0475` |
| 所属 Phase | Phase 0/1 fixture测试harness组织 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0474](0474-single-ownership-lifecycle-and-declarations.md)，输入8c614955 |
| 影响范围 | tests/fixtures.rs及私有领域子模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整职责

1. discover_fixtures至parse_decimal_offset完整发现/sidecar方法族移fixtures/discovery.rs，
   fn改pub(super)并根私有use，恢复原根域，路径验证/排序/sidecar语法原样。
2. run_suite至run_block_fail_case完整Source/Lexer/Parser suite执行族移fixtures/execution.rs，
   fn改pub(super)并根私有use，所有读源/重复解析/expected诊断/root shape门保留。
3. 原完整cfg(test) tests模块体外置fixtures/contracts.rs，根显式path仍mod tests，
   原临时目录/atomic、所有test与cfg平台分支不变，不通过新层改变FQN。

共享模型/fixture路径、load/validate/parse_fixture_twice、报告函数及support模块留根，
不复制harness。仅可见域/模块导入/安全rustfmt变化，不改字符串fixture、断言/SourceId/
Span/排序、平台symlink与空/非UTF8/sidecar失败保护。无allow/ignore/新增依赖/wrapper，
根与新文件小于1000，仅删fixtures baseline其他policy等值。现有scripts/.github/support
引用搜索仅尺寸policy指向旧根，不存在类似0473校准源码锚点遗漏。

## 2. 验收

- [ ] R1：完整块/剩余根机械核对，fixture字符串/测试/平台条件及FQN不变。
- [ ] R2：原完整fixtures target基线及逐片通过后本地提交，不逐片push、不伪造RED。
- [ ] R3：完整frontend、generated-owner Python真实源码接缝回归、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，平台cfg未执行不计通过，
不重复累计，不为了数量机械拆散剩余三个大型单一iteration测试算法。

## 3. 真实证据

待逐片记录。
