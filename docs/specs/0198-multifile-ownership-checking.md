# SPEC-0198：跨文件所有权检查

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-198` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32 §32 |
| 批准依据 | 2026-08-27 当前持续 Goal 授权先审计 roadmap、再按依赖图推进已完成审计的 Spec |
| 前置 Spec | SPEC-0029、0030、0032、0197 `done` |
| 前置 ADR | ADR-0020 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit ownership products、fixtures；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 2. Goal

完成后，每个跨文件 callable/constructor 使用全局 typed identity 获得精确参数、loan、move、drop
和 closure effects，整个 compilation unit 形成可供 codegen 消费的完整 owner-aware typed product。

## 3. 范围与需求

- 每个 body 恰检查一次；跨文件 call/constructor 复用目标声明的既有 mode 与 ownership facts。
- 所有权数据流仍是 source/body-local；跨文件 callee 的参数 mode/effect 已由 typed call descriptor
  固化，不新增 interprocedural value-state 分析。
- 跨文件 MoveOnly delivery、Borrow/Inout loan、capture、return 与 ASAP drop 规则和单文件一致。
- 诊断携带使用文件与目标声明关联位置，并按 unit 稳定排序。
- 失败 unit 不发布部分 codegen input；单文件 ownership API 保持兼容包装。

## 4. 非目标

- 不改变所有权语义，不实现 SSA/LLVM、跨 compilation-unit ABI、LSP 或项目构建。
- 不消费 SPEC-0210 的 const-enabled typed unit，也不实现跨文件 const materialization；该能力
  必须由显式消费 0198、0210 与 0208 的后继 Spec 增量发布，不能隐式重开本 Spec。

## 5. 验收标准

- [ ] 正反例覆盖跨文件 Borrow/Value/Inout、MoveOnly return、constructor、closure 与 drop point。
- [ ] use-after-move/loan 冲突诊断含精确跨文件目标信息且顺序确定。
- [ ] 单文件 ownership suite、frontend/workspace 基线与 Architecture 同步。
- [ ] mixed-analysis product、错误 unit/source/body locator 与重复 source 均被内部门禁拒绝。

## 6. 技术方案与边界

消费 SPEC-0197 的 validated typed unit，共享 unit type context，并按
`DeclarationId -> (SourceUnitId, local body)` locator 对每个 body 运行现有 checker。局部 node/symbol
ID 从不脱离 source/body 使用；结果汇总为带 recovery diagnostics 与 validated codegen gate 的
`OwnershipCheckedUnit`（或等价产物），不重新做名称或类型解析。单文件 API 事实保持精确等价。

## 7. 实施计划

1. [ ] 建立 unit ownership driver 与跨文件 callable facts → 验证：mode/loan 正反矩阵。
   - [x] 建立 source-qualified recovery product、typed-analysis provenance 与 mixed-input 门禁，
     并发布顶层/member/companion/lambda 参数的 Owned/Shared/Exclusive binding 能力；完整 checker
     闭环前不发布 validated/codegen gate。
   - [x] 消费 unit call descriptor，归一化 source-qualified Value/shared-loan/exclusive-loan argument
     contracts，并为 Declaration/Symbol target 回链真实参数声明范围；external/function-value 不伪造
     源码位置，错误 call/argument/parameter locator 由内部门禁拒绝。
   - [ ] 在 body-local 数据流中接通 return/drop、capture formation 与 validated gate；普通 call
     contract、container 特殊交付、Rc effects 与 capture 输入事实已由完成切片覆盖。
   - [x] 在 body-local 数据流中执行普通 typed call contracts，建立 source-qualified shared/
     exclusive loan、Copy/Move/Temporary Value delivery，并复用 L0131–L0136 的适用诊断；任一诊断
     原子清空可执行 loan/delivery facts；container 特殊交付由下方独立完成切片覆盖。
   - [x] 执行 intrinsic Rc `share/value`，发布 source-qualified retain/borrow-payload effect，支持
     named/temporary owner，并复用 L0131/L0132 与跨文件 Value 参数标签；错误 unit 原子清空 effects。
   - [x] 执行 source-qualified constructor descriptor，按源码求值顺序发布 Copy/Move/Temporary
     delivery、MoveOnly inline/heap/shared root obligation 与 `Nothing` 提前终止前缀；复用 L0131–L0133、
     L0135 及 element L0136，并为跨文件 field/payload 保留参数标签，错误 unit 原子清空 construction
     plans。
   - [x] 将 intrinsic container descriptor 归一为既有 source-qualified argument contracts：list-form
     重复执行 Value Copy/Move/Temporary delivery，runtime-length 建立两个同步 shared loan，空
     MutableList 不发布 operand effect；descriptor identity/shape/type/mode 在 dataflow 前 fail-loud，
     错误 unit 原子清空可执行 facts。
   - [x] 发布 source-qualified closure/capture 输入事实：词法 binding 使用 `UnitSymbolId`，字段与显式
     receiver 规范化为 `This`，默认/`move` lambda 分别得到 Borrow 与 Copy/Move effect，并复用 unit
     类型图计算具体 environment `Transferability`；错误 unit 原子清空 capture facts，closure/type
     capability recovery facts 保留供诊断，formation move/loan 留给下一切片。
2. [ ] 接 move/drop/capture formation 与确定性诊断 → 验证：`multifile_ownership_checking`、门禁反例和
   既有 ownership suite。
3. [ ] 同步 Architecture 并跑 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | compilation-unit ownership product | `feat(frontend): check multifile ownership (SPEC-0198)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 roadmap 审计 | 通过 | 补齐 type facts 与 backend 之间缺失的 Phase 3 层 |
| 2026-08-29 实施前审计 | 通过 | SPEC-0029/0030/0032/0197 done、ADR-0020 accepted；旧 checker 使用 file-local identity，第一切片须先建立 source-qualified product/provenance，再接跨文件 callable mode/loan |
| `cargo test -p lang-frontend --test multifile_ownership_checking --locked --offline` | 通过 | 2 tests；source-qualified 顶层/member/companion/lambda binding modes、声明范围、input permutation、mixed-analysis 与 duplicate-input 门禁 |
| `cargo clippy -p lang-frontend --test multifile_ownership_checking --locked --offline -- -D warnings` | 通过 | 第一切片 product/provenance/binding API 与集成测试无 warning |
| `cargo fmt --all -- --check` | 通过 | 第一切片最终源码与文档状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | frontend 全 target 无 warning |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 48 tests |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test ownership_checking --test ownership_closures --test ownership_construction --test ownership_containers --test ownership_rc --test ownership_structural --locked --offline` | 通过 | 54 tests；第一切片与全部既有 Phase 3 所有权集成回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 252 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI dev build |
| `git diff --check` | 通过 | 最终第一切片无空白错误 |
| 独立复审 | 通过 | 原 Low（signature locator/重复 binding 静默覆盖）已修复并复审；最终无 High/Medium/Low |
| Tier 3 判定 | 未触发 | 当前是 SPEC-0198 中间切片，未改通用 parser/harness、共享依赖或未知公共下游；按根 `AGENTS.md` §9 以 Tier 2 作为提交门禁 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --locked --offline` | 通过 | 第二切片 3 tests；Declaration/Symbol/function-value/external contracts、参数声明范围与 input permutation |
| `cargo clippy -p lang-frontend --test multifile_ownership_checking --locked --offline -- -D warnings` | 通过 | 第二切片 API 与测试无 warning |
| `cargo fmt --all -- --check` | 通过 | 第二切片最终源码状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第二切片 frontend 全 target 无 warning |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第二切片后 48 tests |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test ownership_checking --test ownership_closures --test ownership_construction --test ownership_containers --test ownership_rc --test ownership_structural --locked --offline` | 通过 | 第二切片后 55 tests；unit contracts 与全部既有 Phase 3 所有权集成回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第二切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第二切片后 252 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第二切片后 CLI dev build |
| 第二切片独立复审 | 通过 | parameter-index 门禁、rustdoc 边界、具名实参重排 3 个 Low 已修复并复审；最终无 High/Medium/Low |
| 第二切片 Tier 3 判定 | 未触发 | 中间 contract 切片未执行所有权数据流，也未改 parser/harness、依赖或未知下游；按根 `AGENTS.md` §9 使用 Tier 2 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --locked --offline` | 通过 | 第三切片完成时 17 tests；第四切片另增 2 个 Rc 用例后当前为 19。覆盖 Copy/Move/Temporary、shared/exclusive loan、字段/element/Rc payload place、L0131–L0136、跨文件参数 label、lambda/when、错误事实原子清空与 input permutation |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第三切片后 48 tests |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_type_checking --test ownership_checking --test ownership_closures --test ownership_construction --test ownership_containers --test ownership_rc --test ownership_structural --locked --offline` | 通过 | 149 tests；共享 Copyability、unit dataflow 与全部既有 Phase 3 所有权回归 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第三切片 frontend 全 target 无 warning；新生产模块均低于 1000 行软上限 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第三切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第三切片后 252 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第三切片后 CLI dev build |
| 第三切片独立复审 | 通过 | 先后发现并修复 10 个数据流/文档缺口；终局复审无 High/Medium/Low |
| 第三切片 Tier 3 判定 | 未触发 | 中间 ordinary-call 数据流切片未改通用 parser/harness、共享依赖或未知公共下游；按根 `AGENTS.md` §9 使用 Tier 2 |
| `cargo fmt --all -- --check` | 通过 | 第四切片最终 Rust 源码状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第四切片 frontend 全 target 无 warning；同一 Rust 源码状态结果复用，后续只改验收治理 Markdown |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第四切片后 48 tests；同一 Rust 源码状态结果复用 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_type_checking --test ownership_checking --test ownership_rc --locked --offline` | 通过 | 第四切片 118 tests；unit Rc typed/ownership effects、普通数据流与单文件 Rc 回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第四切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第四切片后 252 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第四切片后 CLI dev build |
| 第四切片独立复审 | 通过 | 初审发现入口 rustdoc 与历史 test count 2 个 Low，均已修复；终局复审无 High/Medium/Low |
| 第四切片 Tier 3 判定 | 未触发 | 中间 Rc effect 切片未改 parser/harness、依赖或未知公共下游；按根 `AGENTS.md` §9 使用 Tier 2 |
| `cargo fmt --all -- --check` | 通过 | 第五切片最终 Rust 源码状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第五切片 frontend 全 target 无 warning |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第五切片后 49 tests；含重复 construction descriptor 在 dataflow 前拒绝的内部门禁 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_type_checking --test ownership_checking --test ownership_closures --test ownership_construction --test ownership_containers --test ownership_rc --test ownership_structural --locked --offline` | 通过 | 156 tests；constructor ordered delivery/root obligations、跨调用期 L0135、element L0136、unit typed facts 与全部既有 Phase 3 ownership 回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第五切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第五切片后 253 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第五切片后 CLI dev build |
| 第五切片独立复审 | 通过 | 初审的 descriptor first-wins、入口 rustdoc、L0135 覆盖及后续 O(S×D)/延迟门禁 Low 均已修复；终局无 High/Medium/Low |
| 第五切片 Tier 3 判定 | 未触发 | 中间 constructor ownership 切片未改 parser/harness、依赖或未知公共下游；按根 `AGENTS.md` §9 使用 Tier 2 |
| `cargo fmt --all -- --check` | 通过 | 第六切片最终 Rust 源码状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第六切片 frontend 全 target 无 warning |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第六切片 50 tests；含重复 descriptor 与非法 container shape/mode 的 dataflow 前门禁 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_type_checking --test ownership_checking --test ownership_closures --test ownership_construction --test ownership_containers --test ownership_rc --test ownership_structural --locked --offline` | 通过 | 159 tests；container Copy/Move/Borrow、`Nothing` 前缀、unit typed facts 与全部既有 Phase 3 ownership 回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第六切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第六切片 254 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第六切片 CLI dev build |
| 第六切片复审 | 通过 | 发现并修复 local initializer 丢失 `Nothing` 终止 flow、单元素未证明重复 delivery 两项缺口；补齐 descriptor shape/mode 门禁后未发现剩余问题 |
| 第六切片 Tier 3 判定 | 未触发 | 私有 contract/dataflow helper 与定向 Phase 3 套件的可界定中间切片；未改 parser/harness、依赖、guide 语义或未知公共下游，按根 `AGENTS.md` §9 使用 Tier 2 |
| `cargo fmt --all -- --check` | 通过 | 第七切片最终 Rust 源码状态 |
| `cargo clippy -p lang-frontend --all-targets --locked --offline -- -D warnings` | 通过 | 第七切片 frontend 全 target 无 warning |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 第七切片 50 tests；含 compilation-unit ownership 内部门禁回归 |
| `cargo test -p lang-frontend --test multifile_ownership_checking --test multifile_type_checking --test ownership_closures --test ownership_checking --locked --offline` | 通过 | 第七切片 135 tests；source-qualified capture/`This`、Borrow/Copy/Move、unit `Transferability`、错误事实原子清空、输入排列确定性及相关单文件回归 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 第七切片跨 crate 编译兼容 |
| `cargo test --workspace --lib --bins --locked --offline` | 通过 | 第七切片 218 passed、1 ignored；ignored 为既有 LLDB task-port 权限用例 |
| `cargo build -p lang-cli --locked --offline` | 通过 | 第七切片 CLI dev build |
| 第七切片复审 | 通过 | 发现并修复非法 `UnitTypeId` 查询可能越界、ownership facade 新增职责过重、测试 fixture 语法/契约偏差及显式 `this` 覆盖缺口；终局复审未发现剩余问题 |
| 第七切片 Tier 3 判定 | 未触发 | source-qualified capture 输入与私有 capability 查询的可界定中间切片；未改 parser/harness、依赖、guide 语义或未知公共下游，按根 `AGENTS.md` §9 使用 Tier 2 |
