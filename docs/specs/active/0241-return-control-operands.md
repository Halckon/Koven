# SPEC-0241：Return 控制表达式操作数与单文件 enum 条件

> **性质**：实施与验证 Spec · **状态**：in-progress · **读取时机**：核对 return 控制表达式与 Litmus4 的实际覆盖时 · **唯一真源**：本 Spec 的范围、验收及剩余交付

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0241` |
| 所属 Phase | Phase 1；Litmus4 所需的窄 Phase 4 闭合 |
| 语言规范 | [Guide v0.40 控制流](../../guide/06-blocks-control-flow.md)、[构造](../../guide/11-copyability-layout-construction.md) |
| 批准依据 | 2026-10-01 用户批准继续并发布 return-control 与调用参数迁移两个阶段 |
| 前置 Spec | 无 |
| 前置 ADR | 无；沿用已有 typed SSA 与 enum 布局合同 |
| 关联 Spec | SPEC-0238、SPEC-0242 |
| 基线与分支 | main `e22e11b`；`fix/spec-0241-0242-recovery` |
| 影响范围 | Parser return、单文件 when lowering、直接测试与当前事实账本 |
| 语言语义变更 | 否；不改变 Guide 或资源预算 |
| 阻塞项 | 远端 CI 与发布闭环尚未完成 |

## 1. Goal 与边界

在 block/lambda 内把同行 `return if`、`return when` 保存为 return 的值，保留换行、分号、
else、caller delimiter 与 EOF 边界；让未改写的 Guide15 Litmus4 经单文件 SSA/LLVM 和真实
native 执行验证。测试从 Guide 直接提取源码，仅追加调用者与显式 `Shape` 局部变量。

单文件 enum expression condition 仅接受 frontend 已发布的无 payload case construction，
并要求整体 root 为 Copyable。校验 construction descriptor、ownership plan、root/type 与
EnumCaseId 的身份，再用 enum layout 中的 variant 比较 subject tag；不能按名称或 AST 猜测。

不实现 compilation-unit enum expression condition、直接 case 调用实参、通用 enum 相等、
payload/grouped case 条件或 MoveOnly 条件。初始发布基线未包含SPEC-0240，故其Litmus12
const位运算缺口当时保留；最新main整合后已保留0240的正向验证，见末节。借用调用迁移
由独立SPEC-0242负责。

## 2. 验收

- [x] 新 parser/Guide/native 测试先在旧实现失败，再验证最小修复
- [x] parser 正反例、Span、caller stop 与线性预算回归
- [x] 精确 Guide4 两入口 frontend 与单文件 native 12/20/0
- [x] enum identity/顺序、SSA verifier 与精确未支持边界
- [x] 定向 frontend/codegen、fmt、严格 clippy、下游编译（既有失败见账本）
- [x] Architecture/文档门禁与独立复核
- [ ] 发布、远端 CI 和归档闭环

## 3. 当前恢复说明

原本地阶段提交 `d18ae18752f1957201c239669ce23708f8a77fc4` 在当前环境与远端均不可取。
本变更根据现行合同和保留的阶段说明重新实现，不声称字节级恢复；既有历史运行计数不作为
本次成功证据。以下仅记录当前 worktree 实际运行。

## 4. 验证账本

Cargo 使用共享 target 串行运行；Linux x86_64 + glibc，Rust 1.96.0、LLVM/Clang 21.1.8。
原阶段的计数不复用；未执行 frontend 全量。新例子使用 `println(String)`，整数结果先与
预期值比较后打印固定 marker，不声称已有整数 println overload。

| 验收项 / 命令 | 实际结果 | 限制 |
|---|---|---|
| `cargo test --locked --offline -p lang-frontend --test parser_return_control`，旧实现 | 3 passed / 7 failed | AST operand 与缺 else value-context 红证据 |
| `cargo test --locked --offline -p lang-frontend --test guide_litmus litmus_04`，旧实现 | 0 passed / 1 failed / 20 filtered | L0087，`return` 的 [147,153) 精确范围 |
| 初次 codegen offline 尝试 | 未执行测试 | 缺少依赖缓存；联网 `--locked` 下载后继续，不计为行为红测 |
| `cargo test --locked -p lang-codegen --lib return_control_tests`，初始测试夹具 | 1 passed / 5 failed / 511 filtered | native 夹具误用了不存在的整数 println；先修正夹具，不把这部分当后端证据 |
| `cargo test --locked --offline -p lang-codegen --lib return_control_tests`，修正夹具、旧后端 | 2 passed / 4 failed / 511 filtered | Guide4/enum identity native InvalidModel、Guide4/general enum SSA InvalidSsa |
| `cargo test --locked --offline -p lang-frontend --no-fail-fast --test parser_return_control --test guide_litmus` | 31 passed（10+21） | 最小 parser 修改后；后增两个 frontend oracle 见下一行 |
| `cargo test --locked --offline -p lang-frontend --test guide_litmus` | 23 passed / 0 failed / 0 ignored / 0 filtered | 原Guide4、括号变体、nested return/if/when、newline L0087、缺else L0057；均走单文件/unit |
| `cargo test --locked --offline -p lang-frontend --lib parser::engine::tests` | 19 passed / 0 failed / 161 filtered | 32/64规模与所有访问预算不变；两处裸return序列改为换行，避免变成嵌套jump operand |
| `cargo test --locked --offline -p lang-codegen --lib return_control_tests` | 6 passed / 0 failed / 511 filtered | 3真实native（含Guide4输出12/20/0）+3 SSA；case identity反序、general/payload/grouped/MoveOnly拒绝与direct case MissingFact |
| `cargo test --locked --offline -p lang-codegen --lib guide_litmus_04_unit` | 1 passed / 0 failed / 516 filtered | unit 原例在 `Shape.Point -> 0` 精确 UnsupportedNode；不是unit native正例 |
| 最终 fmt、workspace all-targets check、严格 clippy | 通过 | `--locked`、`-D warnings`，统一 target 串行；未禁用门禁 |
| 最终 frontend lib / codegen / CLI / LSP | 180 / 517+4 doctests / 66 / 26 passed | 0 failed / 0 ignored；Linux 宿主 |
| 最终共享 corpus matrix / stage / Guide gates | 27 targets 29 passed / 55 targets 681 passed / 132 passed | Guide 为126 frontend与6 codegen，codegen另有511 filtered；不代表frontend全量 |
| `python3 scripts/check_docs.py` / 检查器测试 | 最终445 Markdown / 45 tests passed | 已同步0241 inventory、索引与DAG；SSA架构页保持200行上限 |
| `bash -n scripts/check_guide_litmus.sh` / `git diff --check` | 通过 | 未改Guide源码或冻结历史验收 |
| macOS、远端 CI | 未运行 | 待对应环境与发布闭环；不由本地Linux结果推导 |

状态保持 in-progress，直至整合门禁、独立复核与发布/远端 CI 完成。

## 联合分支检查点

基于 main `e22e11b736aab1231209e3403bd0c931b9ddb940` 的
`fix/spec-0241-0242-recovery` 已同时整合两项修复。本轮 14 个直接 suite 实际
456 passed / 5 failed；唯一失败目标 `multifile_type_checking` 为 99 passed / 5 failed，
五个失败名称与已知基线相同，未修改这些失败断言。其余 13 个目标全部通过：fixtures、
单/unit ownership、ownership_containers、parser_call_argument/contextual_type_ref/expression/
return_control/diagnostic_witness、type_callable/type_checking、tree_sitter_grammar/textmate_grammar。
其中 return-control 新增同行 return 链与深层 return-if 的确定性递归上限回归，11 项通过；
调用参数31项通过。文档结构445篇、脚本测试45项和 diff 检查通过。

两个切片各自经过独立只读合同复核，未发现阻断；复核不冒称运行过未执行的 Cargo。
最终本地 workspace check/严格 clippy、fmt、完整 core、共享 corpus 矩阵与 stage/Guide
门禁已通过，实际计数见上表。共享合法输入移除调用侧 `borrow ` 后，26个matrix文件
的精确字符/token枚举计数同步调整；首轮旧计数失败后，最终27个matrix目标全部通过，
没有删除测试、弱化恢复断言或放宽线性预算。双平台PR CI仍待发布后验证，保持in-progress，
不代表main已交付或已可合并。

## 最新 main 整合验收

初始发布head `9e54af2fcd8f39331621306435f154238a18aa3d` 的
[PR #9完整双平台CI](https://github.com/Halckon/koven/actions/runs/36949600712) 已通过，
8个jobs全部success、无job skip。Linux实际core793、stage681、Guide132 passed；
macOS core791、stage681、Guide132 passed，另1项既有LLDB测试因debugserver
权限限制ignored。Guide定向codegen每个平台另511 filtered，不计为通过。

其后main合入SPEC-0240至 `8f3e460ba2e186a4fbbcb8ea63a671e1c318b7fa`，本PR保留
原head并合入最新main；10处文档与门禁冲突按两侧合同合并，生产Rust代码自动合并。
Guide脚本同时运行return-control与Litmus12 native过滤器，所有12项规范例子的诊断/
ownership检查转为正向，仍保留typed快照与各自native未支持范围。

最新组合的本地验证（Linux x86_64 + glibc、Rust1.96.0、LLVM/Clang21.1.8，共享target串行）：

- fmt、workspace all-targets check、严格clippy通过；无warning豁免。
- frontend lib180、codegen538+4 doctests、CLI66、LSP26全部passed，0 failed/ignored。
- stage为57个targets / 697 passed；Guide为142 frontend + 8 codegen passed。
  两个Guide codegen过滤器分别532与536 filtered，不计为通过；两个原始Litmus的native均实际link/run。
- 文档447 Markdown、检查器45项、脚本bash -n、git diff --check全部通过。
- 独立只读整合复核未发现阻断；90个不重叠crate文件与各自来源一致，4个重叠文件保留双方模块、实现和测试。

新head的双平台PR CI仍须在发布后验证；上方初始head绿灯不代替这一组合的远端验收。
既有multifile/editor失败及未支持native边界不由上述定向结果抹去，未运行frontend全量。
