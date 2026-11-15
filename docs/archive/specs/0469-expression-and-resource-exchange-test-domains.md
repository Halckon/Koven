# SPEC-0469：表达式与资源交换测试领域归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批测试职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
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

- [x] R1：完整迁移块和剩余根机械保全，原test leaf与新增域FQN一一核对无漏重。
- [x] R2：原完整target基线→逐片定向复验→可构建本地提交，不逐片push、不伪造RED。
- [x] R3：整组完整frontend/Clippy/fmt/policy/docs/尺寸/空白，独立精确提交diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

合同4d978daa，三个完整切片在逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原完整target前后 | 尺寸 |
|---|---|---|---|
| AST shapes十七项 | e077bb41 | parser57/0/0 | 新763行 |
| recovery二十三项 | 4045c110 | parser57/0/0 | 根1926→565，新607 |
| exchange十二项 | ab4d1dfd | resource31/0/0 | 根1054→597，新461 |

52项迁移测试（另含唯一terminal helper）正文逐字节保全，仅尾空白去除；两根
删除精确块、新增显式私有模块，parsed_case_with_diagnostics共享helper留根。
按leaf及预期新增域核对原88项无漏重，不复制harness或改断言。
policy只删parser1926与resource登记1093两baseline，其他JSON等值；resource实际1054
与旧登记1093分别记，不混淆。rustfmt调整两根模块顺序及尾空行，不改变执行逻辑。

机械核对脚本最初按插入顺序重建预期root，与rustfmt排序不同导致assert失败，逐段diff
确定仅两个模块顺序及尾空白；更新核对以剥离明确新增声明后比较root，通过。失败为
仓库外证据脚本假设错误，非产品测试失败；工具输出保留会话，无单独首次失败日志。
无为通过脚本改生产代码或测试。其后的完整测试/Clippy等门禁不受此前脚本失败影响。

组末完整frontend合计 **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行；定向88与既有组合不重复累计。frontend all-target Clippy
-D warnings、fmt、policy47、docs966、尺寸/空白退出0；2179手写Rust，历史超限
**16→14**，本PR累计21→14。

独立fresh review完整4d978daa..ab4d1dfd真实3736行diff及harness/helper/日志，
No issues found，核实AST/typed suffix提交回滚、source-local错误、lexer poison和nested
owner边界，以及Single/Unit资源root/direct-field、中断abort、abandoned result与parent
唯一cleanup oracle。审查者不Cargo，不把读日志算其独立重跑；父流程另验组末门禁。
Architecture同步测试域，原始证据/完整diff/review与机械结论留仓库外。

Rust组末门禁后未再修改。未本地重跑下游codegen/CLI/LSP全套、CI-only fault/真实校准或
sanitizer，不以纯测试迁移扩展能力；R4集中push和精确新头CI另验，Spec active/PR Draft，
不归档或合入，不使用旧头绿灯替代当前候选。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
