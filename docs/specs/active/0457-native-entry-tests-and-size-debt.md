# SPEC-0457：Native 入口测试职责与根文件尺寸欠账

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：处理 native 测试根历史尺寸欠账时 · **唯一真源**：本批职责切片，语言语义仍从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0457` |
| 所属 Phase | Phase 4，native object/入口测试 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续完善历史欠账，且明确后续提交叠在同一 PR、不重复开 PR |
| 前置 Spec | [SPEC-0456](0456-runtime-buffer-responsibility-and-size-debt.md)实现 `6c5d0904` |
| 影响范围 | `native_tests.rs`、私有入口测试模块、该根文件的尺寸 policy |
| 阻塞项 | 无；不改变生产代码或扩大测试可见性 |
| 语言语义变更 | 否 |

## 1. Goal

把 native façade 的入口/object 发布边界用例从共享 harness 根分离，使根从1134行降到
1000行以内，退休其历史 baseline/exception；五个原测试完整保留、实际 native 结果不变。

## 2. 分支与交付

按当前用户明确授权继续 `fix/spec-0456`、PR #433，不新开分支/PR；这是本批对通常
逐 Spec 新建主干分支流程的明确例外，不改变其他任务默认规则。
输入提交 `6c5d090488802306991e500e1d47a89bc49e445b`；该分支创建时经fetch基于main
`d39d0d54`，发布时PR base已为`e16c1d27`。不把未合main组合冒充本地已验收，最终以精确候选CI另验。

## 3. 范围与非目标

移动以下五个完整用例到私有 `native_tests::entry_tests`，物理路径按现有native约定为
`native_entry_tests.rs`：

- `resolved_unit_entry_emits_the_existing_native_wrapper_object`
- `borrowed_argv_entry_accepts_utf8_boundaries_and_rejects_invalid_sequences_before_call`
- `entry_shape_and_analysis_identity_fail_before_object_emission`
- `frontend_diagnostics_and_unsupported_source_do_not_write_objects`
- `standard_error_emits_object_while_interpolated_message_fails_before_writing`

五项共同覆盖 native entry/analysis 身份、front-end拒绝不发布object、canonical error的
object边界与 argv wrapper，不与无关内容按容量拼接。保留原cfg(unix)、source/Span、
UTF-8正反矩阵、nm/main符号断言、目录快照、错误种类与object存在/不存在断言。

`TestDirectory`、唯一NEXT_DIRECTORY、emit_link_and_run与UnitSource原归属不动；
不复制或新建harness，不添加wrapper、重导出、pub或allow。保留后代constant_tests仍通过原根使用的NativeUnitEntry导入，不把间接消费者误判为unused。
仅退休该根baseline1297和exception1312；不改变其余阈值、checker、Rust生产逻辑或其他34项保留文件。

归档0182记录的旧完整测试名是历史命令，不改写为本批新路径；本批另记录一对一前缀迁移。
当前scripts/workflows/native export中没有五项旧完整名称的固定消费者，不添加旧名兼容层。

## 4. 验收

- [x] R1：五项测试含属性与正文逐字节保全，FQN仅增加entry_tests一层；fixture/断言/cfg与唯一harness不变。
- [x] R2：根与新文件在1000行内；仅该根两项policy登记删除，其他JSON值不变，尺寸门禁和47项policy tests通过。
- [x] R3：五项迁移前后默认/feature均实际运行；原argv有效/无效UTF-8、native返回与object/no-object断言通过。
- [x] R4：临近仍留根的stdout与layout/diagnostic consumer定向通过，双配置all-target Clippy、fmt/docs/空白通过；独立审查且有发现修后复核。
- [ ] R5：事实快照和真实验收记录同步；最终同PR精确CI绿灯后再申请交付，不借旧候选成功。

## 5. 顺序计划

1. [x] 读harness/入口helper/直接调用方，冻结五项正文与旧测试库存，跑双配置基线。
2. [x] 最小职责移动、删本根旧额度 → 机械保全、尺寸与policy核对。
3. [x] 双配置新命名用例与临近消费者、门禁、独立复核 → 记录实际结果。
4. [ ] 追加提交并更新同一PR；精确CI另验，不提前归档/合并。

本次是已有测试组织重构，不增加产品行为，不伪造RED；1134行为真实尺寸旧欠账。
若既有用例失败，先保留失败证据定位原因，不削弱断言、ignore或改语言合同。

## 6. 验证记录

输入为 `179e6bf8`（0457合同提交，Rust内容与前置6c5d0904相同）。原始日志与机械审计只留仓库外，以下为真实结果，不预填最终CI。

| 验证项 | 选择与结果 |
|---|---|
| 迁移前 | §3五个旧FQN默认/feature各 **5/0/0**；fixture与断言未改 |
| 机械保全 | 五个完整属性/正文逐字节相同；新FQN只插入entry_tests，两个libtest list各核实恰一次、旧FQN不重复；root除模块注册/移走测试外完全相同，所有原导入和harness字节不变 |
| 首次构建失败 | 默认after exit101，E0433位于 `native_string_length_tests.rs:302`：误删根NativeUnitEntry导入，遗漏后代constant_tests通过super::super::*使用。恢复原导入，不修改consumer或加allow |
| 最终定向 | 五项新FQN，加原根stdout、layout diagnostics及间接constant argv的3项，再加0456原26项；同次默认/feature各 **34/0/0**、exit0。8项本片与26项前片分组，不重复累计；native均实际build/link/run |
| 编译/格式 | 默认/feature all-target Clippy带-D warnings、fmt退出0；未另跑workspace check：本片仅cfg(test)私有测试接线，无生产或跨crate API变化，不用前片旧头check冒充本片执行 |
| 尺寸/policy | 根 **1134→869**、新entry文件 **274**；恰好删除本根baseline1297/exception1312，其他JSON值不变；policy tests **47/0**、尺寸门禁通过，当前全树35个历史超限，不宣称清零 |
| 独立审查 | 首轮误认删根导入正确；修正复核已明确撤回该结论，确认间接依赖、原导入恢复、最终34项/双Clippy/FQN证据及唯一harness；不以错误初审代替复核 |

Architecture已同步测试实际归属，归档0182旧名称原样保留。当前未跑frontend/完整codegen、
完整LLVM慢组、本地CI-only fault或sanitizer全套；实际全矩阵由同PR最终候选CI另验。
前片候选 `6c5d0904` 的run43659088768观察时仍in-progress（Linux tests已success，
macOS tests未结束），不作为本片完整验收；push过滤成功也不替代PR CI。
最终docs954、空白通过；一次合并门禁命令在120秒观察期限内未完成尺寸检查，
不计通过。随后独立尺寸复跑实际退出0（2145手写Rust、35历史超限），原超时边界保留。
