# SPEC-0174: overload lambda 候选隔离检查

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-174` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.28 §28](../guide/01-design-decisions.md#28-泛型-callable-实例化与-overload-lambda-隔离v028) |
| 批准依据 | 用户已明确启用 v0.28；当前持续 Goal 对 Spec 有站立授权，前置完成后可据此批准 |
| 前置 Spec | SPEC-0067、SPEC-0173、SPEC-0177 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无；SPEC-0177 已完成 |
| 影响范围 | `lang-frontend` callable candidate trial、typed expression/call/parameter facts、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；只实施现行 v0.28 封闭的 candidate isolation 契约 |

## 1. Goal

完成后，多 overload 调用中的 lambda literal 能在每个已映射、已实例化候选的 expected
function type 下隔离检查；只有唯一成功候选的完整 typed 增量会进入最终产物，失败或歧义
trial 不泄漏诊断与事实。

## 2. 背景

SPEC-0067 为多个候选先以无 expected type 检查每个 operand；带参数 lambda 因此产生 L0083
或 deferred，不能利用候选的函数参数类型。SPEC-0173 只覆盖唯一期望函数类型，并明确把
候选隔离登记为本 Spec。现行 v0.28 要求在泛型实例化完成后建立可回滚的 typed trial，而不是
在共享 checker 表上反复写入后只删除诊断。

## 3. 范围与需求

- 按源码顺序完成既有名称、参数映射、mode、SPEC-0177 泛型实例化及非 lambda 实参过滤，
  再对剩余候选逐一检查 lambda literal。
- trial 必须隔离 expression/type-ref facts、lambda 参数 mode、nested call descriptor、
  deferred facts、flow/capture invalidation 及诊断；失败候选不改变共享产物。
- 一个成功候选时原子提交它的完整增量并记录唯一外层 call；零个使用 L0123，多个使用
  L0124，不发布任一候选的 lambda/nested-call facts。
- 只有一个完成映射的候选时继续走普通 expected-type 路径，保留 L0084 等直接诊断，不将
  普通类型错误降格为 L0123。
- trial 可用 lambda body 的类型相容性淘汰候选，但不读取调用返回 expected type、不插入隐式
  转换，也不按源码顺序或模式偏好打破歧义。

## 4. 非目标

- 不实现泛型 callable 推导或实例 identity；由前置 SPEC-0177 完成。
- 不实现 Kotlin 完整局部双向求解、SAM/interface conversion、返回类型重载、默认参数、
  callable reference、safe call 或跨文件 overload。
- 不改变 runtime 求值顺序，不在 Phase 2 执行 ownership/capture/loan 检查。
- 不增加新的用户诊断码；沿用 L0084、L0123、L0124。

## 5. 验收标准

- [x] 两个不同函数参数类型的 overload 可由 lambda body 相容性选出唯一候选，并发布正确
  lambda 参数 type/mode、nested call 与外层 call descriptor。
- [x] 两个成功 lambda trial 只产生 L0124；零成功只产生 L0123，不泄漏候选内 L0083/L0084/
  operand 诊断，也不保留任一 trial typed facts。
- [x] 唯一映射候选中的 lambda 错误仍产生精确 L0084 和 expected 参数 label。
- [x] 多个 lambda 实参、具名实参、Value/Borrow/Inout 函数参数及已实例化泛型候选均有正反例。
- [x] 每个 case 重复检查产物与诊断顺序一致；operand 的最终 call mapping 保持源码顺序。
- [x] Phase 2 pass/fail fixture、`type_callable` 领域测试与 workspace 标准基线通过。
- [x] Architecture、guide roadmap、Spec 状态与实际实现同步。

## 6. 技术方案与边界

- 在 type checker 内建立候选局部 transaction/snapshot，覆盖本 Spec 所列全部可变 typed 输出；
  只回滚 diagnostics 的方案不满足验收。
- 复用 `check_expression(..., expected, ...)` 和 SPEC-0173 的 lambda contract 发布路径；不建立
  第二套 lambda body checker。
- trial 按候选源码顺序运行以保证确定性，选择只看成功数量；复杂度与候选数和 lambda 子树
  大小线性相关，不做递归式候选组合回溯。

## 7. 实施计划

1. [x] 建立 typed candidate transaction 与完整回滚 characterization tests → 验证：失败 trial 零泄漏。
2. [x] 接入多候选 lambda expected-type 检查及唯一提交 → 验证：选择/无匹配/歧义矩阵。
3. [x] 覆盖多 lambda、named/mode、generic candidate 与确定性 → 验证：`type_callable` 窄测。
4. [x] 补 Phase 2 fixture并运行 workspace 标准基线 → 验证：实际退出状态。
5. [x] 同步 Architecture、guide、Spec 验收并检查 staged diff → 验证：提交仅含 SPEC-0174。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | transaction/snapshot 与回滚不变量 | `refactor(frontend): isolate callable trials (SPEC-0174)` |
| 2 | overload-lambda 选择、fixture、Architecture 与 done 验收 | `feat(frontend): check overload lambdas (SPEC-0174)` |

## 9. 未决问题

- 无语言语义未决项；SPEC-0177 已完成，本 Spec 可进入实施。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-24 现状审计 | 通过 | 确认多候选当前无 expected 检查 operand，带参数 lambda 无法形成可提交候选事实 |
| 2026-08-24 v0.28 启用审计 | 通过 | guide 门禁已解除；SPEC-0177 前置仍未完成 |
| 2026-08-24 实施门禁 | 通过 | SPEC-0177 已完成；依据持续 Goal 的站立授权进入 `in-progress` |
| `cargo test -p lang-frontend --test type_callable` | 通过 | 16 项领域测试覆盖唯一提交、零泄漏、多 lambda/mode、generic 与确定性 |
| Phase 2 type pass/fail fixture | 通过 | 新增 overload-lambda 正反例；每类 fixture 均为 8 个 |
| workspace 标准基线 | 通过 | fmt、check、clippy `-D warnings`、全目标 test 与 `lang-cli` build 均为退出码 0 |
