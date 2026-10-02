# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。先从本页定位当前状态，
不要默认读取完成历史。

## 当前状态

- SPEC-0245：普通 concrete class 资源析构有界验收完成并归档；[PR #12](https://github.com/Halckon/Koven/pull/12) 初始 head 双平台 CI 全绿，保持 Draft，由用户决定合并。历史证据从完成 Spec Archive 追溯。

- SPEC-0243：receiver 两阶段借用与 unit native 有界验收完成并归档；[PR #10](https://github.com/Halckon/Koven/pull/10) 已通过本切片双平台 CI，保持 Draft，由用户决定合并。历史证据从完成 Spec Archive 追溯。
- [SPEC-0241](active/0241-return-control-operands.md)：return if/when、Litmus4 两入口前端与单文件 native；联合门禁和远端 CI 待完成。
- [SPEC-0242](active/0242-automatic-borrow-call-migration.md)：调用点自动借用迁移；直接 parser 验证通过，联合回归与远端 CI 待完成。
- [SPEC-0239](active/0239-linux-ci-gates.md)：Linux/macOS CI 与定向整合门禁，本地验证通过，远端待授权验证。

- [SPEC-0238](active/0238-guide-litmus-gate.md)：PR #6 审查后的 Guide 勘误与可执行 Litmus 前端门禁。
- [演进实施账本](evolution-status.md)：13 项计划的实际实现边界与独立基线缺口。

- [Active](active/README.md)：`approved` / `in-progress`；当前状态与验收见各 Spec。
- [v0.36 阶段路由](drafts/v0.36/README.md)：单文件与跨文件常量 Phase 2/3/4 均已完成。
- [v0.37 阶段路由](drafts/v0.37/README.md)：借用式顺序迭代，3 份 done、1 份 approved。
- [完成 Spec Archive](../archive/specs/README.md)：216 份 `done`/`superseded` 记录，仅在追溯时读取。
- [Spec 依赖图](dependency-graph.md)：`scripts/gen_spec_dag.py` 生成的拓扑图（SVG 版本
  [dependency-graph.svg](dependency-graph.svg)），不含验收状态，状态以本页为准。

### 版本总览

| 版本 | 状态 | 下一项 | 依赖入口 |
|---|---|---|---|
| v0.40（本地已启用） | SPEC-0235 in-progress，真实 v0.39 已归档 | 三项规则的实现闭环及获准后的 PR CI | [版本路由](drafts/v0.40/README.md) |
| v0.36（已启用） | 常量 Phase 2/3/4 done | 当前阶段链已完成 | [v0.36 阶段路由](drafts/v0.36/README.md) |
| v0.37（已启用） | 0179/0211/0212 done、0182 approved | SPEC-0182 for lowering | [drafts/v0.37](drafts/v0.37/README.md) |
| clone-first v0.39（已由 v0.40 继承） | SPEC-0236 in-progress，本地定向验证完成 | 整合回归、获准发布与 PR CI | [SPEC-0236](active/0236-explicit-string-clone.md) |
| 本地八阶段整合 | SPEC-0237 in-progress | 交叉契约与统一 target 门禁 | [SPEC-0237](active/0237-local-integration.md) |
| host-native 扩展（已批准） | SPEC-0228 in-progress | Linux x86_64 + glibc 目标与基线验收 | [SPEC-0228](active/0228-linux-x86-64-native-host.md) |
| v2 interface 值 / Map 所有权（proposal） | 未启用 | 待评审 | [proposals](../proposals/README.md) |

本表是导航摘要，不改变任何 Spec 的批准状态；各版本阶段路由与 Spec 正文仍是唯一真源。

## 生命周期

```text
draft → approved → in-progress → done
  │                                  └→ archive/specs/
  └─ blocked 时保持 draft
```

- 编号在 active、drafts 和 archive 间全局唯一且不复用。
- 前置 Spec 必须 `done`、前置 ADR 必须 `accepted`、语义 guide 必须已启用，才能进入 active。
- `done` 前逐条勾选验收并记录实际命令；未执行项必须写明原因。
- 一个 Spec 只定义一个 Goal；长期架构理由写 ADR，完整语义链接 guide，不在 Spec 复制。

新建草案使用 [TEMPLATE.md](TEMPLATE.md)，并放入 `drafts/` 或对应未启用版本子目录。
