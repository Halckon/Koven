# SPEC-0360: String when 清理事实的 refinement 消费

> **性质**：有界后端变更合同 · **状态**：in-progress · **读取时机**：修改或验收 String when refinement 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0360` |
| 所属 Phase | Phase 4/5；不变更 Phase 3 合同 |
| 语言规范 | [Guide v0.46](../../guide/README.md) |
| 批准依据 | 2026-10-27 用户“继续推进”；独立实现 p41、Draft PR 与官方 CI，禁止本地动态验证 |
| 前置 Spec | [0357](0357-string-when-ownership-facts.md)、[0356](../../archive/specs/0356-refine-when-family.md) |
| 关联 ADR | [0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[0018](../../adr/accepted/0018-string-owner-runtime-abi.md) |
| 影响范围 | refinement subset / when CFG、真实源测试、p41 oracle 与事实文档 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 main `5b7f3e977aacfc72f4419095ec3bc4fb75f2f73d` 新建
`feature/spec-0360-string-when`。0359 的未合并 Rc 分支
`44d1e11c2a764d41289c919c05901b9eb8e81cd5` 不作为输入，不修改其工作树。
交集为 subset、测试注册/准入、conformance 与架构/路线图；不交叉改变 Rc 语义。

以现有通用 refinement 真正运行原 `conformance/probes/p41_when_string.ko`，不改其字节与
`when-str\n` 强 stdout oracle。无 legacy 捷径、无 frontend 新释放点。

## 2. 事实消费

- subject 求值一次。String Group 按 producer 同样归一到内部真实表达式身份。
- `StringEqual` 已计算 Boolean 后、CFG 分流前，消费 source-qualified
  `WhenComparisonCompleted { control, entry, alternative }`；候选 owner 恰一次。
- 成功边消费既有 `WhenAlternativeMatch`；失败边消费 `WhenAlternativeNoMatch`。
  所有目标与位置只来自 validated facts，不由最后一次比较等语法推测释放。
- 构造互斥边时恢复同一 subject temporary 的原 ID/槽位；不发新 drop，不修改 fact。
  零比较沿已有 `AfterExpression(subject)`，不伪造 alternative。
- 直接 Borrow String 参数 subject 使用 caller shared loan，不 Read 为 owned String，不结束
  或释放 caller loan；callee 的其他 MoveOnly Borrow 用法仍由既有 subset 限制。
- 完整事实消费检查、通用槽位 CFG/SSA liveness、SSA 与 LLVM verifier 均保留。

## 3. 范围与原观察

仅无插值 String literal/Group candidates、producer 已证明且属于现有 refine 表达式子集的
普通 named/temporary String subject；包含逗号、早/晚/无匹配、else/无 else、零比较、
body/后续使用、既有 Return/Break/Continue。非 String MoveOnly、复杂 candidate、field/index、
borrow-result call、显式 borrow-result binding、一般 Abort call 不扩展。

0357 两条 consumer 拒绝 fixture 原源码与 UnsupportedNode 断言保留，使用既有 test-only
`with_per_family_lowering` 钉住仍拒绝新事实的旧路径；公开路径另加同源成功测试。subset 原
String 源码原字节移入成功准入矩阵。这是阶段能力门的迁移，不删除原语义观察或其他负控。
旧 per-family 对未消费事实的拒绝门保留，防止其他未迁入构造落入漏清理路径。

## 4. 验收矩阵

| 原始来源/场景 | 预期行为与证据 |
|---|---|
| p41 原文件 include | `when-str\n`；公开单文件 native + 单/双 source SSA/LLVM |
| 动态 temporary + early/late/miss + Group/逗号 | subject 一次求值；每次 1 allocation/1 free；未求值后缀无 drop |
| named subject body + after | 两次原字符串输出；1 allocation/1 free |
| no else、only else、empty | `done\n`；每条 1 allocation/1 free |
| Borrow 参数 + caller 后续使用 | callee 仅 candidate drop、无 owned Read；caller 两 allocation/free |
| Return/Break/Continue | `return\nloop\n`；3 allocation/free，无重复清理 |
| 候选清理精确次序 | SSA equality 紧跟候选 drop；native String drop hook 为 1/3/3 候选加固定4 |
| 显式 borrow-result binding | 保持 subset false；不扩大既有能力 |

每个新增 source 经 single 与两个 source 的正逆输入公开分析；双 source 输出确定性比较。
计数器与 native 命令只由官方 CI 执行。allocation/free 不冒充 literal owner drop 证明，后者
另用 String drop hook 与 SSA 身份/次序断言。新测试位于 `ssa/refine_string_when_tests.rs`。

## 5. 验证账本

- 本地 Cargo/build/check/clippy/test/native/test-list：全部未执行（用户禁止）。
- 本地只做官方定向 rustfmt、文档生成/结构、尺寸/字节与 diff 静态检查。
- exact Draft PR 的双宿主官方 CI 待发布与核验；未宣称编译/运行通过。
- 保持 in-progress；未 Ready、未 merge、未 Slack 通知；父任务负责一次 review。
