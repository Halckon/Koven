# SPEC-0253：共享单文件阶段门面

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 bootstrap 与 legacy LSP 的共享纯阶段推进时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-253` |
| 所属 Phase | Phase 1→3 单文件纯编排与 Phase 6 宿主适配；治理 P3b 有界切片 |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 前置 Spec | SPEC-0250、0251、0252 `done` |
| 前置 ADR | [ADR-0020](../../adr/accepted/0020-multifile-compilation-unit.md)、[ADR-0021](../../adr/accepted/0021-lsp-explicit-source-set-protocol.md) `accepted` |
| 阻塞项 | 本地实现与独立评审通过；发布及精确 head 双宿主验收待完成 |
| 语言语义变更 | 否 |

## 1. Goal 与有界交付

按获批[治理计划 P3b](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)
及[执行账本剩余](../../development/engineering-governance-progress.md#p3b-有界剩余与退出条件)，
从 PR33 main `82610ab9d3d8148949f2a6f485e5a0b50d045ff1` 建立 `feature/spec-0253`。
本分支保留已独立审阅的0252归档提交 `e6dfea4`，与本片合并在一份后继 Draft PR 中发布，
减少重复 PR/CI；本地实现验收完整后才发布，不作中间 speculative push。

本片只共享 bootstrap 与 legacy LSP 的纯单文件阶段推进。保留五阶段原始产物与内部错误、
CLI任意非空诊断逐阶段早停、LSP完整recovery与typed之后ownership之前的definition观察。
不改checker、const能力、单文件/unit内核、codegen交接、entry/native/link或legacy协议。
不新增workspace member、dependency、trait/context/mode框架，不承诺性能收益。

## 2. 阶段与身份合同

采用固定纯 runner `analysis::analyze_single_file`，接收显式 `&SourceMap`、`SourceId`、
同一次创建的 name/type environment 与两个宿主接缝：

1. 每阶段完成后 `gate(stage, diagnostics)`，Err立即停止，不能先运行完整链再检查
2. typed gate通过后、ownership开始前，一次 `typed_observer(SingleFileTypedView)` 返回宿主 T

view字段私有，只读投影原parsed/names/typed。回调临时借用不能逃逸或作为T持有。
结果 `SingleFileAnalysis<T>` 私有字段拥有原raw parsed/names/typed/owned与T，通过只读
访问或消费式 `into_parts` 取回；不克隆大facts，不伪装validated basic/const/backend能力。

错误分为原Lexer/Parser/Name/Type/Ownership内部错误与Host(E)，Display/source保留原原因。
原SourceMap与SourceId/Span不重建；不提前核验环境配对，foreign环境仍在原type阶段失败。
宿主各调用一次standard_environments并传原配对，回调不能替换阶段输入或facts。
不聚合、去重或排序诊断，不读盘、注册source或依赖LLVM/LSP类型。

CLI gate复用原reject_diagnostics与FrontendStage，observer返回()；entry/native留原处。
LSP gate不拒绝用户诊断，observer直接返回DefinitionIndex；之后按parsed/names/typed/owned
原顺序聚合并使用ordered_diagnostics。duplicate open、旧/同version接受、publish成功后替换、
close先remove与source-set的不同语义不变，不把legacy升级成unit事务策略。

## 3. 实施次序与唯一验收账本

先冻结旧生产宿主oracle，独立设计review通过后，新API准确编译红→最小实现→CLI/LSP迁移。
全部Cargo串行共用既有target、不clean；新Rust文件≤1000物理行，不提高历史尺寸额度。

| ID / 合同 | 目标与证据 | 实际结果 |
|---|---|---|
| A1 旧生产宿主基线 | CLI完整human/JSON/exit；LSP冻结manual链和协议oracle | 旧生产CLI build及二进制冻结；148条进程捕获，新CLI4/4、旧链完整LSP45/45；Unicode/CRLF强化另由旧二进制16组合实跑 |
| A2 完整facts与来源 | `single_file_analysis`：手工链差分、原SourceId、同轮环境/typed/owned、clone/fresh负例 | API准确E0432红后实现；完整8/8通过，raw全字段差分与独立兼容性断言、合法clone/recheck及fresh拒绝分别验证 |
| A3 阶段与恢复 | 同target：五gate/typed observer顺序与短路、foreign source/env、recovery/const/deferred | A2同次8/8；6事件顺序、各gate/observer中止、payload析构、foreign错误优先级/原原因、显式非标准环境、const+move及无诊断deferred均通过 |
| A4 公开封闭合同 | `single_file_analysis_compile_contracts`：正控、私有字段/只读、借用不逃逸、raw与unit能力不互换 | 完整5/5；E0451/E0616/E0308/E0521及精确lifetime反例配正控；T/E/capture不得借走view，move-only与合法外部借用/跨输入期按值结果均可用 |
| A5 CLI/LSP迁移 | 两宿主完整测试；CLI native build/run与失败输出保全，LSP全部publication/definition/UTF16/legacy差分 | 最终fixture下完整CLI82/82（48bin＋3format＋9native＋9project＋9unit oracle＋4单文件），完整LSP45/45，CLI build通过；四宿主原parity均实际重跑 |
| A6 相邻前端合同 | 单文件names/types/ownership与既有unit façade合同定向选集 | 17个完整frontend targets284/284（含新13），frontend docs12/12；全部0 failed/ignored/filtered，完整选择与计数见下 |
| A7 工程门禁 | fmt、workspace all-targets check、受影响三crate严格clippy、docs/Python policy/尺寸/diff | 全通过；docs476、Python100/100；701手写Rust/48历史超限/0生成物，无额度或例外变化；门面202行、bootstrap366、analysis163，新增测试最大395行 |
| A8 交付 | 新targets现有stage各恰一次无filter；独立实现review；一份Draft最终精确head双宿主CI | selection policy零选择真红→16/16绿；独立设计与实现均Approve；未发布或跑本片双宿主CI，不以基底证据替代 |

本地仅x86_64 Linux；macOS留最终远端实跑。frontend全量、workspace全量tests与性能测量不在
本片声明中。配置接线不等于执行，passed/failed/filtered/ignored及未运行分别记录。

## 4. P3b退出与剩余

四宿主parity、公开能力/身份合同与精确head双宿主证据齐全后才可结项P3b。
const owned交接仍为P3a剩余，0182、其余P2/P4/P5与整体计划后的外部审计不被本片关闭。
不把单文件→unit内核合并、basic/const合并或整体snapshot加入本片必做项。


## 5. 本地执行与失败历史

实际环境x86_64 Linux，Rust/Cargo1.96.0、LLVM/Clang21.1.8；全部Cargo串行复用target，
无clean、无新dependency。原CLI binary在任何生产迁移前由e6dfea4构建并冻结；新增LSP
oracle首次执行时bootstrap/analysis与该基底生产diff为空，期望端仍是独立旧手工链。

新API首轮E0432为缺接口的真实编译红；最小实现后的首次编译因测试中impl Trait函数指针
和unreachable observer未标返回类型发生E0283/E0282，补测试类型注解后绿，未改阶段语义。
LSP旧链首次42/45，3项误把不同SourceMap的隐藏owner当相等；改为完整稳定raw诊断、各自
primary/label所属map/URI校验与definition的URI/byte/slice投影，再获45/45。完整publication
Eq与独立UTF16断言保留；同map raw Eq/来源合同另在frontend测试中执行，不降低身份要求。
初次Architecture概述写入已满200行的names-and-types被文档门禁拒绝，移至既有tooling
职责页后通过；未压缩历史正文或放宽门禁。最后Unicode fixture补跨行CRLF，在旧binary16组合
重新捕获验证后，完整CLI/LSP均以最终fixture复跑通过。
收尾base-to-head diff发现真实CRLF被默认whitespace规则当作trailing whitespace；
`.gitattributes`仅给两份明确CRLF fixture设置`-text`与`cr-at-eol`，保留其余默认whitespace
检查；不改变fixture bytes、生产代码或Rust policy。完整base diff复查通过。

17个frontend完整target计数：basic6/compile4、lexer20、names15、view1/compile7、
ownership31/constants16、parser file27/error propagation8、single-file8/compile5、
type callable19/checking82/constants24、snapshot7/compile4，共284，均无filter或ignore。

```sh
cargo test --locked --offline -p lang-frontend --no-fail-fast --test single_file_analysis --test single_file_analysis_compile_contracts --test unit_name_snapshot --test unit_name_snapshot_compile_contracts --test basic_unit_ownership --test basic_unit_ownership_compile_contracts --test owned_compilation_unit_view --test owned_unit_view_compile_contracts --test name_resolution --test type_checking --test type_callable --test type_constants --test ownership_checking --test ownership_constants --test lexer --test parser_file --test parser_error_propagation
cargo test --locked --offline -p lang-frontend --doc
cargo test --locked --offline -p lang-cli
cargo build --locked --offline -p lang-cli
cargo test --locked --offline -p lang-lsp
cargo fmt --all -- --check
cargo check --locked --offline --workspace --all-targets
cargo clippy --locked --offline -p lang-frontend -p lang-cli -p lang-lsp --all-targets -- -D warnings
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_rust_sizes.py --base 82610ab9d3d8148949f2a6f485e5a0b50d045ff1
git diff 82610ab9d3d8148949f2a6f485e5a0b50d045ff1 --check
```

本地未运行完整stage/Guide脚本、完整codegen、frontend全量、workspace全量tests或macOS；
前者不被17个定向targets冒充，双宿主最终CI留交付阶段。本片仅局部实现验证完成，Spec仍
in-progress，须由最终精确head双宿主证据决定P3b结项；归档0252正文完整保留。
