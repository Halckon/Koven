# AGENTS.md — Koven 文档治理规范

本文件适用于 `docs/` 及其子目录。根 [`AGENTS.md`](../AGENTS.md) 规定工程实现方式；本文件
规定文档如何分类、流转和保持一致。文档不是实现的替代品，但它们定义范围、记录决策并
保存当前架构事实。

---

## 1. 文档体系

```text
docs/
├── README.md
├── AGENTS.md
├── agent-language-design-guide-v*.md
├── specs/
│   ├── README.md
│   ├── NNNN-kebab-case-title.md
│   └── TEMPLATE.md
├── adr/
│   ├── README.md
│   ├── NNNN-kebab-case-topic.md
│   └── TEMPLATE.md
└── architecture/
    └── README.md
```

| 文档 | 回答的问题 | 生命周期 |
|---|---|---|
| 版本化语言规范 | 语言语义及 guide 已强制确定的实现、Phase 边界是什么 | 新版本取代旧版本，旧版保留 |
| Spec | 这一次具体做什么、如何验收 | `draft → approved → in-progress → done` |
| ADR | 为什么选择这项长期架构决策 | 接受后不改写历史；由新 ADR 取代 |
| Architecture | 仓库当前已经实现成什么样 | 随实现直接更新为最新事实 |

当前语言语义真源是
[`agent-language-design-guide-v0.8.md`](./agent-language-design-guide-v0.8.md)。它是跨功能、
跨 Phase 的版本化规范，不属于单次实现 Spec。

---

## 2. 推进顺序

一次功能或行为变更按以下顺序推进：

1. **读取规范**：确认现行语言 guide 与当前 Phase，不从 Kotlin、Rust 或旧文档推测语义。
2. **起草 Spec**：在 `specs/` 中定义范围、非目标、验收标准、测试证据和受影响模块。
3. **记录 ADR（按需）**：若工作改变长期架构边界，先接受 ADR，再批准引用它的 Spec。
4. **实现与验证**：按 Spec 任务清单推进，测试结果必须能对应每条验收标准。
5. **更新 Architecture**：只记录最终已经落地的结构、依赖和数据流，不把计划写成事实。
6. **完成 Spec**：全部验收通过后标记 `done`；未执行检查必须明确记录，不能静默完成。
7. **提交并完成 Goal**：创建只属于该 Spec 的提交；提交成功后才把关联 Goal 标为完成。

如果只是修正文案、链接、拼写或清理无效旧文档，不必创建 Spec。纯 bug 修复可以不新建
Spec，但必须添加回归测试；若修复会改变既有语言语义，则必须先更新规范并建立 Spec。

### 站立授权与简化确认

用户可以在当前任务或持续 Goal 中授予后续 Spec / ADR 的站立授权。站立授权仅在授予它的
当前任务或持续 Goal 范围内有效；用户撤销、收窄授权或该 Goal 完成时终止，不自动延伸到
其他任务。撤销或收窄只影响尚未发生的后续批准 / 接受，不回滚已经形成的 `approved` /
`accepted` 历史。授权有效且未被撤销时：

- 完整、无阻塞且符合现行 guide 的 Spec 可按 `draft → approved → in-progress` 的逻辑顺序
  连续推进，无需逐份再次询问用户，也无需为 `approved` 单独创建状态提交；
- ADR 在背景、决策、替代方案、收益与代价完整，且没有改变现行 guide 的语义或边界时，
  可直接记录为 `accepted`，无需先提交 `proposed` 再创建纯状态提交；
- 使用站立授权自动推进的新建 Spec / ADR，或首次由 `draft` / `proposed` 转为
  `approved` / `accepted` 的文档，必须记录所依据的站立授权；既有 `done` / `accepted`
  文档不追溯补写。授权只替代重复人工确认，不替代前置条件、阻塞审计、验收标准、测试、
  Architecture 同步、验证记录或独立实现提交；
- 批准可以自动化，验收可以自动执行，但不能自动视为通过。验收勾选和 `done` 状态必须由
  实际行为证据、回归测试及适用的 workspace 基线支持；
- 未决问题会改变范围、语言语义或用户可观察结果时，仍保持 `draft` / `proposed` 并请求
  用户决定，不得把站立授权解释为允许猜测；
- 站立授权不适用于语言 guide。每个新 guide 仍必须由用户明确指定具体版本取代当前版本，
  才能成为真源。

---

## 3. 版本化语言规范

- 文件名使用 `agent-language-design-guide-vMAJOR.MINOR.md`；版本号表达语言设计版本，不与
  Spec 或 ADR 编号混用。
- 只有用户明确指定的新版本才能取代当前版本。创建了更高版本号文件不等于自动生效。
- 从 v0.3 起，新版本必须说明它取代的版本并维护变更记录；旧版本不删除、不原地改造成
  新语义。v0.3 之前的历史材料未随当前仓库归档，不适用这条保留要求。
- 不改变既有语义的拼写、链接、表述或示例勘误可以修改当前版本；如果改变关键字、语法、
  类型、所有权、标准库契约、强制实现边界或 Phase 验收，必须创建新版本，并同步更新相关
  章节、关键字表（如适用）和版本变更记录。
- 新版本只有经用户明确指定后才能成为当前真源；在此之前，相关 Spec 不能进入
  `approved` / `in-progress`。
- 根 `AGENTS.md` 只保留工程护栏，不复制完整语法表；避免形成第二语言语义真源。
- guide 中存在正文与示例冲突时，按根 `AGENTS.md` 的优先级规则处理，并登记文档缺陷。

---

## 4. Specs

### 何时需要

| 场景 | 是否需要 Spec |
|---|---|
| 新增编译器、标准库、CLI、LSP 或工具链功能 | 需要 |
| 改变可观察行为、语言语义或诊断契约 | 需要 |
| 跨多个 workspace member 的实质重构 | 需要 |
| 不改变既定行为的局部重构 | 通常不需要 |
| 有回归测试的纯 bug 修复 | 通常不需要 |
| 文案、链接、格式或遗留文档清理 | 不需要 |

### 命名与状态

- 文件名为 `NNNN-kebab-case-title.md`，从 `0001` 起在 `specs/` 内单独递增。
- 使用 [`specs/TEMPLATE.md`](./specs/TEMPLATE.md)，一个变更一份文件，不额外拆出重复的
  `plan.md` / `tasks.md`。
- 每份 Spec 只定义一个可验证 Goal，并显式列出 Phase、前置 Spec、前置 ADR、阻塞项、实施
  计划与提交计划。一个提交不得混合多个 Spec；一个 Spec 可以有多个始终可验证且都引用该
  Spec 编号的提交。
- `前置 ADR` 是进入实现的强制门禁，必须为 `accepted`；`关联 ADR` 只用于追溯相关决策，
  不自动构成状态门禁。不得用“关联 ADR”模糊替代前置条件。
- 前置 Spec 全部 `done`、前置 ADR 全部 `accepted`、阻塞项全部解除后，Spec 才能进入
  `in-progress`。不新增 `blocked` 状态；尚有阻塞时保持 `draft`。
- 状态含义：
  - `draft`：正在起草、尚未批准或仍有阻塞，不能开始正式实现；
  - `approved`：用户或权威任务已确认目标与验收标准；
  - `in-progress`：正在实现；
  - `done`：全部验收完成且证据已记录；
  - `superseded`：由另一 Spec 取代，保留原文件并链接替代者。
- 不按状态创建子目录。完成和被取代的 Spec 留在原路径，以编号和 Git 历史追溯。

### 内容约束

- 验收标准必须可执行或可观察，不能使用“基本完成”“看起来正确”等表述。
- 语言功能至少定义 compile-pass、compile-fail、错误码 / `Span`（适用时）以及相邻语法
  回归用例；codegen 功能还需定义运行输出或目标产物。
- Spec 写“做什么”和必要的实现边界，不在其中展开长期选型论证；复杂决策引用 ADR。
- Spec 不复制 guide 或根 `AGENTS.md` 的规则，只链接适用章节并写本次增量。
- `done` 前逐条勾选验收标准和任务，并记录实际检查命令；跳过项必须说明原因。
- 完成实现、测试、Architecture 同步和 Spec 验收后先创建对应提交；提交成功后才可把关联
  Goal 标记为完成。提交信息必须包含 `SPEC-NNNN`，使 Goal、Spec 与 Git 历史可互相追踪。

---

## 5. ADR

### 何时需要

以下变化通常先写 ADR：

- workspace member 或依赖方向变化；
- 新增或替换 IR 层、LLVM 接入策略、runtime / ABI、链接或 bootstrap 方案；
- 影响多个 Phase 的错误模型、缓存、增量编译或平台支持策略；
- 引入会长期改变开发方式的核心依赖或架构模式。

ADR 只能决定现行 guide 留白处的架构选择。若决策会改变 guide 已强制确定的 workspace、
IR、后端或 Phase 边界，必须先形成并启用新 guide 版本，再由 ADR 记录其具体落地理由。
局部实现细节、普通功能和 bug 修复不写 ADR。

### 命名与状态

- 文件名为 `NNNN-kebab-case-topic.md`，从 `0001` 起在 `adr/` 内单独递增。
- 使用 [`adr/TEMPLATE.md`](./adr/TEMPLATE.md)，至少包含状态、背景、决策、后果和替代方案。
- 状态使用 `proposed`、`accepted`、`rejected`、`superseded`。
- 已接受 ADR 是历史记录，不因后来观点变化而改写原决策。需要改变时新增 ADR，并在两份
  文件中互相链接。
- 后果必须同时写收益与代价；只记录结论、不记录被放弃方案的 ADR 不完整。

---

## 6. Architecture

- `architecture/` 记录当前已实现的 workspace、编译流水线、依赖关系、核心数据结构、
  诊断流、runtime / ABI 与平台边界。
- 架构文档是可更新快照，不保留每次变化的历史叙事；决策历史属于 ADR。
- 必须明确区分“已实现”“已批准但未实现”“未决问题”。尤其在 Phase 0 前，不得把 guide
  的路线图写成仓库现状。
- 实现使架构图、模块职责或数据流失真时，必须在同一任务中更新 architecture。
- 推荐用小型 Mermaid 图表达依赖或流水线，但图必须配有文字边界与当前状态说明。

---

## 7. 一致性与交付检查

文档变更完成前检查：

- [ ] 根 `AGENTS.md` 指向当前 guide 的真实路径；
- [ ] Spec 的 guide 版本、Phase、ADR 和受影响 member 引用有效；
- [ ] ADR 被取代关系是双向且编号正确；
- [ ] Architecture 只描述已实现事实，计划和未决事项有显式标记；
- [ ] Markdown 相对链接存在，章节引用仍对应正确内容；
- [ ] 没有把语言规则完整复制到 Spec、ADR 或 architecture；
- [ ] 没有把未运行的测试或检查写成“通过”。

禁止保留其他项目的 Spec、ADR、architecture 或模板作为 Koven 的有效文档。需要历史材料时
应明确归档为 legacy，且不得被当前索引或规则引用。
