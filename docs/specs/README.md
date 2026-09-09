# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。先从本页定位当前状态，
不要默认读取完成历史。

## 当前状态

- [Active](active/README.md)：`approved` / `in-progress`；当前 1 份 Phase 4 Spec 已批准。
- [Draft v0.36](drafts/v0.36/README.md)：关联常量，4 份，全部被未启用语义阻塞。
- [Draft v0.37](drafts/v0.37/README.md)：借用式顺序迭代，4 份，全部被未启用语义阻塞。
- [完成 Spec Archive](../archive/specs/README.md)：203 份 `done` 验收证据，仅在追溯时读取。

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
