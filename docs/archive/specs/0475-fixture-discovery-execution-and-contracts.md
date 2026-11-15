# SPEC-0475：Fixture发现、执行与合同测试归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批fixture职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
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

- [x] R1：完整块/剩余根机械核对，fixture字符串/测试/平台条件及FQN不变。
- [x] R2：原完整fixtures target基线及逐片通过后本地提交，不逐片push、不伪造RED。
- [x] R3：完整frontend、generated-owner Python真实源码接缝回归、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，平台cfg未执行不计通过，
不重复累计，不为了数量机械拆散剩余三个大型单一iteration测试算法。

## 3. 真实证据

合同c5aa40ac，三个完整职责逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原fixtures完整target前后 | 尺寸 |
|---|---|---|---|
| discovery/sidecar | 12ccab2a | 23/0/0 | 新272行 |
| suite execution | 25134bfb | 23/0/0 | 新659行（含fmt签名折行） |
| 完整tests模块外置 | 7f9036a8 | 23/0/0 | 根2316→525，新contracts最终892 |
| 文件首空行修复 | 412fd85e | 23/0/0 | 仅删空行，非测试正文 |

原test FQN逐项相同，cfg(test)/Unix/Linux/非Linux Unix/Windows分支原样保留。本机仅执行
macOS平台可用的23项，不把Linux/Windows cfg分支计作已运行。发现/执行/测试完整块和
剩余根按rustfmt机械核对，除精确pub(super)、私有模块/use声明和文件首空行外相同；
原fixture字符串/报告文本/路径与所有断言保全，不机械dedent改变raw string。
policy只删fixtures2316 baseline，其他JSON等值。脚本/.github/support旧源码锚点搜索
仅命中sizepolicy，无0473式校准遗漏；generated-owner Python80（含真实源码锚点）通过。

整组frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail doctest），
ignored不计执行、定向不重复累计。workspace all-target cargo check、frontend all-target
Clippy -D warnings均0。fmt首轮失败仅contracts首空行；首轮机械核对同样识别文件首空行
差异，412fd85e删除该空行后target23与fmt复验0、机械核对通过，产品正文未改。
保留fmt-first-failed.log与原工具输出，不当行为RED。其后policy47、docs972、尺寸/空白0；
2194手写Rust、历史超限 **4→3**，本PR21→3。完整frontend/Clippy覆盖删除空行前相同
Rust token与测试正文，不声称删除空行后重复执行过完整全量。

独立fresh review完整c5aa40ac..7f9036a8真实3684行diff及共享harness/平台测试，
No issues found：核symlink/路径/扩展/sidecar排序与语法、SourceId/Span/诊断、各suite
expected与root shape、重复parse/lex与报告稳定性；审查不Cargo。后续仅一空行删除作
低风险自检，未改算法/断言/字符串。Architecture同步，原始证据/完整diff/review留仓库外。

最终门禁后Rust未再改。未跑下游codegen/CLI/LSP测试全套、生成资源真实校准重跑、
CI-only fault或sanitizer全套；R4集中push与精确新头CI另验，Spec active、PR Draft，
不归档合入，不借旧头CI或编译代替运行。剩余三个大型单一测试算法不以容量机械切碎。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
