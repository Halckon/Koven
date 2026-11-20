# SPEC-0477: Parser 与 Formatter 往返差分首片

> **性质**：测试基础设施变更合同 · **状态**：in-progress · **读取时机**：实施首个完整文件的格式化往返对照时 · **唯一真源**：本 Spec 的首片范围与验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试；不改变语言行为 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b` |
| 分支 | `feature/spec-0477` |

## Goal

针对 `crates/lang-frontend/tests/fixtures/phase1/parser-file-pass/multiple-declarations.ko`，
证明测试会检查：原文和格式化结果均可完整解析，且具有相同的受支持语法结构。
这是同一个 Parser 的变形测试，不是独立实现差分或生产正确性证明。

## 范围

仅在 `crates/lang-frontend/tests/formatting.rs` 所属测试目标内新增断言及私有模块。
投影首片只覆盖普通 package、普通 import、带整数初始化的顶层变量和隐式 Unit 的空函数体。
允许在受支持形状内变化名称、整数字面量、变量种类、声明顺序及可选 import alias，
以构造独立负控。其余构造报未支持，不跳过，不按节点种类套默认值。
不改生产 Parser、Formatter、Guide、依赖和公开接口。D1–D3 未裁决规则不纳入范围。

## 实施顺序

1. 先写投影正负控。以最小占位实现观察预期失败；记录命令和失败原因。
2. 实现测试私有投影与 Span 断言。只读取给定入口的文件头和有序根，通过 typed ID 追踪子节点。
3. 接入真实 fixture；仅调用各臂的现有 lexer/parser，复用原 Formatter 路径，不复制语料。
4. 跑完整 `formatting` 目标与定向 fmt/clippy/docs；审核既有 CI 选择，独立复核投影是否遗漏字段。
   需要新增持续验证接线时，优先复用现有 job，不新建 required job。

## 验收账本

| ID | 验收 | 当前证据 |
|---|---|---|
| 基线 | `cargo test --locked -p lang-frontend --test formatting` | 同 SHA 的规划工作树实跑：2 passed、0 failed、0 ignored、0 filtered；Rust 1.96.0/macOS AArch64 |
| DT-01 | 两臂均无词法/句法错误诊断及恢复 Error，遵守现有完整文件入口合同 | 真实 fixture 往返通过 |
| DT-02 | package、有序 imports、声明根、子树、源码拼写和 marker 可见；绝对位置与 arena ID 不进比较 | 私有根投影与身份负控通过 |
| DT-03 | 每臂独立核对支持节点、其源字段的 SourceId、UTF-8 范围及字面身份；不以根 Span 推断消费 | 校验文件头和声明首尾、子段归属、整数后缀；独立复审通过 |
| DT-04 | 位置平移相等；名称、整数拼写、alias 有无、import/声明顺序的单项变化可检出；非首片节点报未支持 | `formatting projection_` 空投影先 2 failed，真实投影后两项通过 |
| DT-05 | 差异报告包含 fixture/case 和首个不同的结构路径；未支持项报告对应位置 | 负控调用实际比较器，核首差路径；真实 fixture 走同一比较器 |
| DT-06 | 原有词法保真、幂等、可解析性断言继续通过，新增正反与 fixture 测试实际命中 | 完整 `formatting` 目标 5 passed、0 failed/ignored/filtered |
| CI 选择 | 现有 Parser 批次全量选择 `formatting` 一次，不新增 job | 策略红测 1 failed，接线后 `python3 -m unittest scripts.tests.test_check_ci_results` 45 passed；`bash -n scripts/check_stage_integration.sh` 通过 |
| Parser 组合回归 | 从 `scripts/check_stage_integration.sh` 选出含 `--test formatting` 的唯一 Cargo 调用，原样本地执行 | macOS AArch64，27 个 integration targets、479 passed、0 failed/ignored/filtered；不是完整 stage 或双宿主 CI |
| 本机门禁 | 定向 Clippy、fmt、docs、diff | 通过 |
| 交付 | PR CI、归档 | 待执行；未运行完整 stage、Linux 或远端 CI |

不把未纳入投影的语法计入覆盖，也不把本机定向结果替代 CI。
