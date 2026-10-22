# SPEC-0344: 独立门户内容同步

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：维护独立门户内容同步时 · **唯一真源**：本次交付范围与验收

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0344` |
| 所属 Phase | Phase 6 |
| 语言规范 | [当前 Guide](../../guide/README.md)；门户按用户指定的已启用 v0.46 分支设计 |
| 批准依据 | 2026-10-22 用户要求更新独立 koven-site，兼具门户和 Tour，并指定按 v0.46 设计 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 影响范围 | 独立站点、主仓库通知工作流；编译器无行为变化 |
| 语言语义变更 | 否 |

## 1. Goal

独立站点从一个确定的主仓库版本导入教程、执行合同、Guide 与路线图，避免语言示例在站点漂移。
主仓库保留语言内容与真实 CLI 验证，成功的 main CI、版本 tag 或手动事件可通知站点重建。

## 2. 范围

- 复用 `docs/tutorials/koven-tour.md`、`examples.json` 和 `scripts/check_tutorial.py`；不复制一套 `.ko` 真源。
- 站点记录源提交和文件摘要；设计说明、预期输出、计划示例分别展示。
- v0.46 暂从其已启用设计提交导入，不将当前 main 的 v0.45 替换或冒称 v0.46 已全部实现。
- 站点从显式配置检出来源；nightly 用 main，稳定版本用真实 tag。当前无发布 tag 时不伪造稳定版。
- 主仓库只有 `.github/workflows/notify-site.yml`；通知目标和凭据在 Actions 配置，不写死未创建仓库。

## 3. 非目标

不修改语言规则、编译器或现有教程 oracle，不提供浏览器编译器，不注册域名，不迁移主仓库未提交的旧部署文件。

## 4. 验收

- [x] 站点生成器保持具名 fence 与合同一一对应，并拒绝重复、缺失、非法路径。
- [x] 站点类型检查、生产构建、来源一致性检查通过。
- [x] 门户、Tour、规范、路线图及窄屏交互经浏览器核验；独立审查具体记录。
- [x] 通知工作流触发/条件静态检查通过；远端接通需真实站点仓库与 Actions secret。
- [x] 文档治理、diff whitespace 检查通过。

## 5. 验证记录

本地验证：生成器与来源选择11测试通过，无跳过；快照校验、vue-tsc、Vite构建通过；
1440px/375px浏览器检查及独立代码/视觉复审通过。通知工作流完成YAML/shell检查及两组mock。
文档门禁732页通过，diff whitespace通过。

独立站点已发布到 `https://koven-lang.github.io/koven-site/`，首次站点构建与 Pages 部署成功（Actions run `39781286849`）。
主仓库目标变量已设为 `koven-lang/koven-site`；缺失专用通知凭据时必须失败，避免将未送达记为成功。
2026-10-22：PR #124 已合入 main `b35c01746ce2df3e2b0fbfea9e2fa95b719a732c`。
主仓库使用专用 Repository secret 的[通知运行](https://github.com/Halckon/Koven/actions/runs/39784467407)成功，
实际触发站点[构建与 Pages 发布](https://github.com/koven-lang/koven-site/actions/runs/39784489466)，两 job 均成功。
用户确认暂保留 v0.46 固定快照，合入 main 后再切换；重建不会提前切回 v0.45。

本次 main CI `39784202653` 的双平台检查、测试和消费者 job 均通过，但被后续 main 更新取代，整体为 cancelled；
对应通知正确 skipped。成功 main CI 自动触发路径等待最新 main CI `39786604501`；因此保持 in-progress。
未运行编译器 native 测试；现有教程源码和 oracle 未改动，站点导入测试不替代 native 验收。
