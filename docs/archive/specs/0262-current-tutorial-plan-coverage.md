# SPEC-0262: 原治理计划的当前教程覆盖补齐

> **性质**：有界变更合同 · **状态**：done · **读取时机**：核验本批教程与提取门禁时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P5-262` |
| 所属 Phase | Phase 6；工程治理 P5 |
| 前置 | SPEC-0256 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户继续推进已批准整体计划；仅本地 main 分阶段交付 |

## Goal 与边界

原计划 P5 还要求自动借用、跨文件、root replace/swap、concrete deinit 教程。
0256 的七个正例覆盖 String.clone 等已有能力，但没有这四项显式示例。
本批添加四个真实 CLI 正例；Koven 源码仅在 Markdown，跨文件清单只引用 fence ID。
不修改编译器、语言语义、Guide、原诊断负例或 planned thread；不修复历史能力边界。

提取门禁拒绝重复/遗漏 fence、重复引用、非法相对路径和状态；允许按 ID 定向执行新增例，
默认仍运行全部合同。跨文件使用现有 project manifest/entry 协议。
新增示例比较完整 build/artifact/run 的 stdout/stderr/exit；不重复旧七正例和两负例。

## 验收账本

| 条件 | 实际证据 |
|---|---|
| 提取器先红后绿，覆盖多文件、源码遗漏/重复与路径拒绝 | 先红1通过/4错误（缺新示例/API）；最终六项定向Python合同通过；原始日志保留 |
| 四个新增例真实 CLI build/artifact/run | borrowing/root-replace-swap首次通过；deinit改为已支持scope夹具后通过；cross-file使用project.toml后通过；各实际build/artifact/run完整输出通过 |
| 文档、inventory、diff | 文档496页结构、inventory定向37项、git diff --check通过；无Rust生产改动 |
| Linux/远端与整体 P5 | 未运行，不由 Mac 有界通过推定 |

LSP 成本试点独立记账，调查阈值不构成本 Spec 的验收豁免。

本地Mac有界完成不等于整体P5双宿主退出；没有推送/PR或Linux结果。
