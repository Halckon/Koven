# Koven Proposals

> **性质**：非规范候选索引 · **状态**：未启用 · **读取时机**：任务明确要求评审未来语义时 · **唯一真源**：各 proposal 正文

Proposal 本身不修改现行 v0.46、不批准 Spec，也不代表实现优先级。已选 String.clone 部分
已移入 guide，剩余候选仍未启用；普通开发任务不要读取本目录。

- [v0.44 类型、控制流与基础集合补全](v0.44-language-completion.md)：选定规则已启用，后继候选与实施验收分开
- [v0.46 v1 待决规则定稿](v0.46-language-decisions.md)：规则已由 Guide v0.46 启用，保留范围与验收要求
- [unsafe 与 FFI 边界](unsafe-and-ffi-boundary.md)：v0.46 从 Guide 13 移出的 `extern`/`RawPtr`/`unsafe` 草图，v2 再议
- [文本输入与 IO（M1B-b）](text-input-and-io.md)
- [集合剩余 API 与迭代](collection-api-followups.md)
- [检测扩展与定向外审（M4）](native-safety-validation.md)
- [测量、分发与未接受P2预算（M5）](measurement-distribution.md)
- [线程所有权转移（M6）](thread-transfer.md)
- [通用借用访问结果（M2B）](general-borrow-access-results.md)
- [借用结果声明与类型合同比较](borrow-access-contract-comparison.md)
- [Map / MutableMap 所有权候选](map-ownership.md)
- [v2 interface 值与动态分发](v2-interface-values-and-dynamic-dispatch.md)
- [所有权、可存储引用与 dyn 的中后期演进总纲](ownership-references-dyn-evolution.md)：v1.x–v4 方向、K0–K8 分阶段与待决问题
  - [之一：调研、基底能力内核与决断](evolution-01-kernel-and-decisions.md)
  - [之二：路线 L 借用内核](evolution-02-route-loan-kernel.md)
  - [之三：路线 X 抽象打包（dyn 与 callable）](evolution-03-route-existential.md)
  - [之四：路线 C/U/S 能力与共享、裸内存、挂起](evolution-04-route-capability-unsafe-suspend.md)
  - [之五：数据结构适配性检验](evolution-05-data-structure-fitness.md)
  - [之六：文本、全局状态、扩展函数与元数据](evolution-06-text-globals-extensions-metadata.md)
  - [之七：并发与异步模型、IO 与外部函数边界](evolution-07-concurrency-async-io-ffi.md)
  - [之八：中间表示、逃逸分析、向量化与自举条件](evolution-08-compiler-ir-escape-simd-selfhost.md)
  - [之九：总体架构——内核与扩展的分层、接口与原则](evolution-09-architecture.md)
  - [之十：尚未覆盖的领域](evolution-10-coverage-gaps.md)
  - [之十一：失败模型、通用接口清单、编译单元依赖](evolution-11-failure-interfaces-units.md)
  - [之十二：标准库清单与平台适配](evolution-12-stdlib-platform.md)
  - [之十三：实施路线——从当前到 v1，从 v1 到之后](evolution-13-implementation-roadmap.md)
- [v1 冻结前的兼容性约束（Z1–Z11）](v1-freeze-decisions.md)：可并入 v1 条目 Spec 的条款，已登记为路线图决策 D16
- [unsafe 别名、失败安全与外部边界合同（U0 + FF0）](unsafe-aliasing-contract-outline.md)：完整候选合同（AL1–AL8、FS1–FS8、MM1–MM7、UB1–UB11、TR1–TR4、FFI1–FFI5 与检测矩阵）
- [借用内核演算（内核 MVP 第 1–3 步）](loan-kernel-calculus.md)：参考检查器原型的语法、规则、用例清单与原型得到的发现
- [内核原型的覆盖与欠账](kernel-prototype-coverage.md)：各内核基底的原型覆盖状态，未覆盖项按“先做 / 记账后集中做”排期
- [内核演进两项悬决策略的裁决纪要与 3A 取消接口对比（2026-11）](kernel-open-decisions-2026-11.md)：OD1（A1 / 1A-ext）与 OD2（B1 / 首个已提交的普通失败）三轮讨论收口、Guide 合法位置示例、3A 取消接口两方案调用例对比与待补多任务验收清单
- [析构义务的原型（内核基底 KB1）](drop-obligation-model.md)：析构规划器与独立检验器、条件移动与失败出口的清理、“消费并解除析构”与必须消费的发现
- [结构能力推导的原型（内核基底 KB4）](capability-derivation-model.md)：能力引擎的模型、与现行规则和实现的对照用例、析构性质拆分与默认上界的发现
- [失败出口与清理义务的状态模型（内核 MVP 第 4 步）](failure-cleanup-model.md)：借用型子任务的失败传播与在途 IO 两个状态模型，各候选政策违反的不变量与发现
- [析构时机：两个方案的对比](drop-timing-comparison.md)：最早析构加屏障，对比词法析构加允许的提前释放；方向已裁决为后者
- [IO 资源借用、责任交接与清理：设计安全原则](io-resource-handoff-cleanup.md)：在途 IO 的保活、责任交接与清理进度；失败政策与后端证明未决
- [Parser 文法与验证实施建议：评审与路径分解](parser-grammar-verification-review.md)：恢复先行、机器文法为新语法把关的路线 F
- [顶层状态、静态存储与全局生命周期](global-and-static-state.md)：G1 接纳面收窄已决（v0.46 C），G2–G5 分层候选
- [String Copyable 候选取舍](string-copyability.md)
- [显式 clone() 候选设计](explicit-clone.md)
- [受限扩展函数候选设计](restricted-extension-functions.md)
- [集合算法所有权原候选](collection-algorithm-ownership.md)：保留原稿供对照，不作为独立启用依据
- [集合算法所有权受审修订（r3.1）](collection-algorithm-ownership-r3.md)

- [Unit 函数根集合工程候选](unit-function-root-set.md)：D10 whole-file 身份见证退出路径，未启用
- [D10 独立 loop 证明候选](d10-independent-loop-proof.md)：独立 catalog、结构边和 SSA replay 的架构约束，未启用
- [D10 child 未修改根最窄候选](d10-child-pass-through-root.md)：两根、单根更新的隔离设计，未启用
- [Parser 分阶段形式化验证计划](parser-formal-verification-plan.md)：Phase 1，待评审
- [Origin / IO 讨论审计与候选基线](origin-io-candidate-baseline.md)：安全不变量、六项验收矩阵与未决实施门槛，非完整定稿

## 候选依赖与推进顺序

2026-10-08 的 collection r3.1 小修保留核心规划方向；候选正文不启用新能力。
以下是设计/实施依赖，不以候选表面或已有语法授权代替 Guide 明确启用：

```text
闭合已批准 M2B/Map 欠项 --> N1a 范围 carrier 生命周期 --> 最小范围视图矩阵
显式 consume/构造原语   ----------------------------> 立即消费路径
N1b 自有索引与交付合同  ----------------------------> 借用 filter（后续）
复制能力独立评审        ----------------------------> 显式 materialize
受限扩展声明            ----------------------------> 标准库算法表面

最小矩阵闭合 --> 其他算法；可选 owned 元素返回另以 Option 为条件依赖
String Copyable、Map 转换与 v2 interface 值不由此获得启用
```

| 候选 | 前置 | 当前状态 |
|---|---|---|
| 受限扩展函数 | 独立 grammar/静态查找审查；需新 Guide 启用 | 已记录放宽语法授权，仍未启用；见候选 §5.1 |
| 显式 clone() | 通用能力需独立设计 | String.clone 已启用；不代表所有 MoveOnly 可 clone |
| String Copyable | 无 | 候选结论：保持 `MoveOnly` |
| 集合算法所有权 | consume 原语、N1a carrier、复制能力与声明表面 | r3.1 区分首片必需项与 N1b 后续验收；正文保持候选 |
| N1 非逃逸 carrier | N1a 新描述符交付、绑定/参数/返回与真实来源 loan；N1b 自有存储另审 | 独立验收；不能直接替换多 capture borrowed closure |
| 通用借用访问结果（M2B） | 实际缺口与正式 Spec | v0.42 已启用受限普通结果与 scoped Map；其余候选及实现欠项保持独立 |
| Map / MutableMap 所有权 | 新增借用结果依赖 M2B；其它查询按所选合同 | 历史候选；新决定须先重基 |
| v2 interface 值与动态分发 | 无 | 独立候选 |
| 顶层状态与静态存储 | G1 需用户决定接纳面；G2 需 const 扩展与 Phase 3 静态 root；G3/G4 需 v2 Shareable | 独立候选；S1 建议进入下一 Guide 小版本 |

建议先闭合当前已批准欠项，再分别评审 consume 基础与复制能力，冻结 N1a carrier 合同、
对齐受限扩展声明，形成最小范围视图/消费矩阵；N1b/filter 后续验收。各独立设计可以先评审；
语义实施前仍须明确启用，资源原语不能反向依赖尚未实现的 filter。
完整验收维度见[collection §11](collection-algorithm-ownership-r3.md#11-phase-边界)，
已选实施范围与顺序见[Spec索引](../specs/README.md)；未选范围由上列候选各自维护。

对应阻塞 Spec 从 [Specs](../specs/README.md) 进入；长期架构候选仍遵循 ADR 生命周期。

## 从候选 guide 到可实施 Spec

版本号不表示累积继承已经成立；任何未来候选都须明确对现行版本的继承与取代关系。
启用前按以下顺序收口，每一步保留可审查证据：

1. 逐条对照候选规则与现行 v0.46，列出保留、补充、取代的规则及目标章节；对冲突请求决定。
2. 明确新版本是否包含其他候选版本。未纳入的候选保持未启用，不能凭版本号隐式合并。
3. 准备完整的新 guide 与引用更新，并由用户明确启用；在此之前 current 入口仍为现行版本。
4. 将对应 Spec 重基到启用后的 guide，核对前置 Spec/ADR 与批准依据，再迁移依赖完备的切片到 active。
5. 按各版本 Spec 索引推进 Phase 产物，逐项记录定向验收；规范启用不代表实现已经完成。

验收命令和测试并行仅由[测试与分层验收](../development/testing.md)定义；候选正文不复制工程门禁。

## 表面语法提案增量门禁（仅限新增或修改表面语法）

本节仅适用于新增或修改 Koven 表面语法的提案；不适用于纯语义、类型、所有权、codegen 或文案改动，
不减免既有 required 检查，也不启用任何候选语义。此类提案在进入 Guide 前须补齐以下 6 项内容：

1. **受影响入口与规则映射**：列明涉及 `parse_file` / `parse_expression` / `parse_declaration` / `parse_block` 中的哪些入口及目标 Guide 章节，禁止在多章复制互相矛盾的允许条件。
2. **纯 CFG 与原始冲突变化**：给出改动前后的离线参考文法增量；若引入新的 shift/reduce、reduce/reduce 或 Earley 双树，必须逐条登记原始冲突与消解规则，不得用优先级指令或词法吞换行静默掩盖。
3. **软词、词法与换行影响**：明确是否新增上下文关键字或改变 `NL` / CRLF / 块注释内换行 / `;` 的续行与分隔判定，以及必要的外部谓词（邻接 Span、无副作用 trial 等）。
4. **恢复边界与资源合同（R1–R5）**：说明非法或未闭合输入是否影响 caller hard stop、soft declaration boundary、owner stack、诊断锚点或线性扫描/递归预算。
5. **独立正反例与边界见证**：覆盖正常接受、跨行/注释/分号边界、未闭合回退及与相近语法的互斥对照；预期从拟议条文独立写出，不从现有实现回填。
6. **下游同步范围**：列明受影响的 Formatter 往返投影、独立参考差分语料与 Tree-sitter 已知漂移清单；未同步项显式登记为范围外。

进入 Guide 前，评审者须逐项核验以下 5 项放行检查（完整清单与演练记录见[F5 增量门禁清单](parser-formal-verification/syntax-gate-checklist.md#2-评审放行清单与-advisory--required-升级条件)）：

- [ ] 规则增量已回链唯一 Guide 章节，未在多章复制互相矛盾的允许条件。
- [ ] 冲突变化已由离线参考工具实测并保留原始报告；任何真正的多重推导（如换行后的附着 vs 独立元素）已在条文中规定唯一消解规则。
- [ ] 软词、换行与邻接谓词均未破坏既有普通 `Identifier` 或词法错误交接（R4）。
- [ ] 正反例覆盖跨行（LF/CRLF/含换行块注释）、分号阻断、未闭合回退与调用方边界。
- [ ] 未将离线参考工具或 Tree-sitter 提升为第二规范，未夸称有限用例为全语言证明。

离线文法或差分切片若需从 advisory 升为 required CI，必须另案获批，并满足依赖治理、双宿主（macOS AArch64 与 Linux x86_64）干净 checkout 可复现及连续观察窗零环境误报。

## 历史

v0.35、v0.36 与 v0.37 已启用；原候选与重基记录仅用于追溯：
[v0.35](../archive/guides/v0.35-candidate.md)、[v0.36](../archive/guides/v0.36-candidate.md)、[v0.37](../archive/guides/v0.37-candidate.md)。

## 已批准文本／IO 的迁移准备

[v0.47 迁移对账](../archive/migrations/v0.47-preparation.md)保全迁移前的完整范围与用户错误 oracle 决定；现已由 [v0.47](../guide/README.md)启用，生产实现另验。
