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
| 阻塞项 | 本地实施、独立评审及精确 head 双宿主验收待完成 |
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

拟采用固定纯 runner `analysis::analyze_single_file`，接收显式 `&SourceMap`、`SourceId`、
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
| A1 旧生产宿主基线 | CLI完整human/JSON/exit；LSP冻结manual链和协议oracle | 旧main代码的CLI build已运行并冻结二进制；新oracle待运行 |
| A2 完整facts与来源 | `single_file_analysis`：手工链差分、原SourceId、同轮环境/typed/owned、clone/fresh负例 | 待执行 |
| A3 阶段与恢复 | 同target：五gate/typed observer顺序与短路、foreign source/env、recovery/const/deferred | 待执行 |
| A4 公开封闭合同 | `single_file_analysis_compile_contracts`：正控、私有字段/只读、借用不逃逸、raw与unit能力不互换 | 待执行 |
| A5 CLI/LSP迁移 | 两宿主完整测试；CLI native build/run与失败输出保全，LSP全部publication/definition/UTF16/legacy差分 | 待执行 |
| A6 相邻前端合同 | 单文件names/types/ownership与既有unit façade合同定向选集 | 待执行 |
| A7 工程门禁 | fmt、workspace all-targets check、受影响三crate严格clippy、docs/Python policy/尺寸/diff | 待执行 |
| A8 交付 | 新targets现有stage各恰一次无filter；独立实现review；一份Draft最终精确head双宿主CI | 未发布；不以基底CI替代本片 |

本地仅x86_64 Linux；macOS留最终远端实跑。frontend全量、workspace全量tests与性能测量不在
本片声明中。配置接线不等于执行，passed/failed/filtered/ignored及未运行分别记录。

## 4. P3b退出与剩余

四宿主parity、公开能力/身份合同与精确head双宿主证据齐全后才可结项P3b。
const owned交接仍为P3a剩余，0182、其余P2/P4/P5与整体计划后的外部审计不被本片关闭。
不把单文件→unit内核合并、basic/const合并或整体snapshot加入本片必做项。
