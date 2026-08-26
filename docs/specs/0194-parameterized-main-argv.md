# SPEC-0194：参数化 main 与 argv owner

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P6-194` |
| 所属 Phase | Phase 4/6 纵向切片 |
| 语言规范 | 现行 [`guide/01-design-decisions.md` §30.1](../guide/01-design-decisions.md#301-单文件-conventional-main)；String runtime 依赖非现行 v0.31 候选 §31 |
| 批准依据 | 无；前置 SPEC-0192 与 ADR-0019 尚未解除门禁，当前持续 Goal 只授权先物化 draft |
| 前置 Spec | SPEC-0193 `done`；SPEC-0192 `draft`，必须先变为 `done` |
| 前置 ADR | ADR-0010、0016 `accepted`；[ADR-0019](../adr/0019-parameterized-process-entry-bridge.md) `proposed` |
| 关联 ADR | ADR-0007、0008、0018 |
| 阻塞项 | SPEC-0192 `done`；ADR-0018、0019 `accepted` |
| 影响范围 | `lang-codegen` native entry plan/wrapper/runtime，`lang-cli` entry selection/run 参数转交，native tests，Architecture/Roadmap |
| 语言语义变更 | 否；实施现行 v0.30 已批准的参数化 main 语义 |

## 2. Goal

完成后，公开单文件 build/run 能选择唯一的
`fun main(args: Array<String>): Unit`，生成真实 process argv wrapper，将不含 executable name、
保持顺序且合法 UTF-8 的参数作为 wrapper-owned Array/String 以 Borrow 调用 Koven entry，并在
正常返回后完整析构。

## 3. 背景

SPEC-0193 已实现 conventional 零参数 main，并能精确识别参数化形状，但当前固定返回
`UnsupportedParameterizedEntry`。ADR-0010/native object API 与 LLVM wrapper 也只接受
`() -> Unit`。v0.30 已发布参数化入口语义；候选 SPEC-0192 将提供一般 String owner，并先验证
String 作为现有顺序容器元素的布局/drop glue。

本 Spec 是 SPEC-0192 的直接后继，只接 process argv bridge 和公开 CLI，不重新定义 String、
Array 或 Borrow ABI。

## 4. 范围与需求

- conventional selection 在唯一合法参数化 shape 时提交该 entry；零参数、missing、invalid-shape、
  ambiguous 与显式 `--entry` 行为保持 SPEC-0193 契约。零参数和参数化 main 同时存在仍是
  ambiguous；显式 entry 仍只接受 `() -> Unit`。
- codegen 建立 ADR-0019 的 verified native entry plan；参数化 plan 精确验证单一 shared Borrow
  `Array<String>` 参数、Unit 返回和稳定 String/Array SSA identity，损坏 plan 在 LLVM/object 前失败。
- LLVM wrapper 使用 `i32 main(i32 argc, ptr argv)`，排除 `argv[0]`；无分配地预检 count、长度和
  UTF-8 后，按正序构造 heap-backed String 与既有两字段 Array owner。
- wrapper 对完整 Array 建立 shared loan 调用 entry；正常返回后结束 loan、逆序 drop String、
  唯一 free Array buffer 并返回 0。零参数 Array 使用既有 sentinel，不 malloc/free buffer。
- invalid UTF-8、负 argc、存在参数时的无效 argv 或目标计数/长度不可表示时，在 entry 调用和
  owner 创建前形成非零 operational failure；argc 为 0/1 都形成空 Array，不产生 replacement
  character、语言诊断、部分 owner 或残留产物。
- `kovenc run` 接受 `[-- <program-arg>...]`，separator 后保留 `OsString` 原始字节并通过
  `Command::args` 直接转交 executable；compiler options、stdout/stderr 与退出状态保持现有协议。
- native integration tests 必须直接执行 build 产物和 `kovenc run` 两条路径，覆盖零参数、多个
  参数、空参数、Unicode、非法 UTF-8、参数顺序、explicit override 与双 shape ambiguity。

## 5. 非目标

- 不改变显式 `--entry` 的零参数契约，不支持 `main` 返回 Int/Result/Nothing、async entry、环境
  变量、stdin、多个 package 或项目级 main 搜索。
- 不实现 String interpolation/member/formatting、一般容器公共 API、Windows wide argv、公开
  `extern` ABI 或稳定 operational diagnostic schema。
- 不让 Koven String/Array 借用宿主 argv，不引入 partial Array owner、unwind cleanup、runtime
  crate、replacement character 或 shell command 拼接。

## 6. 验收标准

- [ ] conventional/explicit entry 选择矩阵覆盖零参数、参数化、双 shape、非法 shape 与 overload，
      既有 SPEC-0193 operational error 保持稳定。
- [ ] native entry plan/verifier 正反矩阵证明参数化 target 精确为 Borrow Array<String>→Unit，
      corrupt type/mode/function identity 在 LLVM 前拒绝。
- [ ] LLVM IR 锁定标准 `argc/argv` signature、先全量 UTF-8/size 预检后 allocation、排除 argv0、
      String/Array owner 构造、Borrow call 与逆序 drop/free；零参数 wrapper IR 保持不变。
- [ ] build 产物真实运行时精确观察空/ASCII/Unicode/空字符串/多参数顺序，main 正常返回 0且无
      泄漏/重复 free；invalid UTF-8 在 main 前非零退出。
- [ ] `kovenc run source.ko -- ...` 原样转交宿主参数；无 separator 的额外 compiler 参数继续按
      usage error 处理，显式 entry 与零参数 main 可以忽略 process args。
- [ ] Architecture、Guide/Roadmap、Spec 验证记录与 workspace 标准基线同步。

## 7. 技术方案与边界

严格实施 accepted 后的 ADR-0019。native entry plan 作为现有 `FunctionId` object-emission 参数的
受检扩展；process wrapper 仍由 `lang-codegen` 拥有，CLI 只选择 typed entry 并转交 program
`OsString`。String 创建/drop 复用 SPEC-0192，Array header/buffer/drop 复用 SPEC-0036，Borrow
call 复用 SPEC-0195/ADR-0016。

两阶段 argv scan 是本 Spec 的关键失败边界：第一阶段不得创建 Koven owner，第二阶段只可能
正常完成或沿现有 allocation abort 终止。不得为逐项验证失败新增未被 SSA/verifier 表达的
partially-initialized Array 清理路径。

## 8. 实施计划

1. [ ] 建立 native entry plan、参数化 shape verifier 与 C wrapper 签名 → 验证：entry/LLVM 窄测。
2. [ ] 实现 UTF-8/count/size 预检、String/Array 构造、Borrow call 和 drop → 验证：IR 与直接
   executable native 矩阵。
3. [ ] 接 conventional selection 与 `kovenc run --` 原始 OsString 转交 → 验证：CLI 进程正反测试。
4. [ ] 同步 Architecture、Roadmap、验证记录并运行 workspace 标准基线 → 验证：文档与事实一致。

## 9. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | verified native entry plan 与参数化 argv wrapper | `feat(codegen): bridge parameterized process entry (SPEC-0194)` |
| 2 | conventional selection、run 参数转交、native 验收与完成文档 | `feat(cli): run parameterized main with argv (SPEC-0194)` |

## 10. 未决问题

- 无设计未决项。状态门禁仍是 SPEC-0192 `done` 且 ADR-0018/0019 `accepted`；在此之前不得实施。

## 11. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 前置审计 | 部分通过 | v0.30 与 SPEC-0193 已生效；SPEC-0192 尚为 draft，ADR-0018/0019 尚为 proposed |
