# SPEC-0458：SSA verifier 职责与历史尺寸欠账

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 verifier 职责整理时 · **唯一真源**：本批验收边界，语言语义仍从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0458` |
| 所属 Phase | Phase 4，typed SSA verifier |
| 语言规范 | 现行 [Guide](../../guide/README.md) |
| 批准依据 | 用户要求每次定向测试、本地提交，成组后统一验证；继续叠在PR #433 |
| 前置 Spec | [0457](0457-native-entry-tests-and-size-debt.md)，输入 `8f589a6d` |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 影响范围 | `ssa/verify.rs`、`ssa/verify_operation.rs`及直接私有子模块与对应尺寸policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无；只整理完整算法，保留所有已有证明边界 |

## 1. Goal与批次

将两个仍超千行的SSA verifier根按完整算法归属拆分，保持原调用门面、阶段顺序、
结构化错误与确定性不变；根和新文件均在1000行内，退休根的历史额度。

沿用用户授权的 `fix/spec-0456` / PR #433，不新开PR。先分别定向测试并本地提交，
两片完成后再统一组合回归、Clippy/fmt/docs/尺寸/policy与独立审查，最后集中push。
本地提交不是已交付；未通过组合门禁不得发布为完成，不因减少门禁频次跳过必要检查。

## 2. 两个顺序切片

1. `verify.rs`：从1009行根分离完整dominance、instruction positions、reachable和
   fixed-point dominators到私有 `verify/dominance.rs`。根保持compute_dominators与
   verify_dominance原导出门面；根公开范围不变，不增加wrapper或第二份算法。
2. `verify_operation.rs`：从1235行根分离root/aggregate/heap/shared/tagged/nullable
   存储构造与投影合同到私有 `verify_operation/storage.rs`。字符串和容器算法不混入；
   原aggregate_explode_contract对SSA域的路径保持，其他helper只供父模块调用。

保持原算法函数整体正文；只改必要相对路径、import与等价可见性。字段、Variant、错误文案、
Origin/Span、branch proof、presence payload、owner/loan及中止/cleanup合同不变。
根使用直接re-export维持既有算法门面，不加转接函数或为测试扩大可见性。

## 3. 非目标

不改语言准入、CFG算法、未达边界的borrow/loop能力、representation或LLVM/native ABI；
不优化fixed point，不弱化expect/assert或测试oracle，不移植未合#427，不动frontend。
不按容量混装，不删现有fixture/函数，不拆大match为碎片，不提高1000行额度。
不把合理保留的34项全部当作缺陷，原历史正文和未覆盖范围不由本批门禁补齐。

## 4. 验收

- [ ] R1：两个切片分别保全完整算法/合同；原调用方与门面不变，私有权限无扩大，机械核对正文及测试源。
- [ ] R2：两个根及新文件均在1000行以内；仅对应旧baseline/exception删除，其他JSON完全相同。
- [ ] R3：每片先跑既有针对性基线，迁移后跑同组并本地提交；用例/断言不改，实际数与未测项分开记录。
- [ ] R4：批次结束统一组合回归（含先前runtime/native）、双配置all-target Clippy、fmt/docs/空白/尺寸/47项policy tests；独立全pass发现修复后复核。
- [ ] R5：更新Architecture和真实账本，集中推送同PR，最终精确候选CI另行验收后才归档。

## 5. 实施顺序

1. [ ] 冻结dominance及直接消费者，定向基线/迁移/定向测试 → 本地提交。
2. [ ] 冻结storage合同及直接消费者，定向基线/迁移/定向测试 → 本地提交。
3. [ ] 一次组合门禁与独立复核，修具体发现 → 更新账本并集中推送。
4. [ ] 最终候选CI独立验收；不使用旧head的绿灯替代。

无行为变化重构以既有GREEN→重构→同组GREEN验证，不伪造RED；原1009/1235行是
真实尺寸欠账。若发现产品失败，保留失败见证，停下定位而非顺手改合同。

## 6. 真实验证记录

实施后按两个本地切片和最终批次分别追加，当前不预填通过。原始日志/快照置仓库外；
每片不重复执行全套门禁，不在Cargo相同target上并发。
