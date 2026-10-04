# SPEC-0264: Unit owned class 一级字段直接 Borrow

> **性质**：有界变更合同 · **状态**：done · **读取时机**：追溯 M1A 字段 Borrow 验收时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-264` |
| 所属 Phase | Phase 4；现行 Borrow 语义的 unit 消费 |
| 语言规范 | [Guide v0.40 所有权](../../guide/10-ownership-borrowing-drop.md)、[成员](../../guide/08-class-family-members.md) |
| 批准依据 | 用户于 2026-10-04 要求持续实施里程碑，满足前置时并行；沿用提交、PR 和合并授权 |
| 前置 Spec | SPEC-0263 |
| 前置 ADR | ADR-0016 |
| 影响范围 | `lang-codegen` unit lowering；CLI project 验收 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

跨文件定义的普通非泛型 class，其 owned local 一级字段可直接作为同步调用的 Borrow
实参；callee 实际读取字段，父 owner 覆盖整个借用期，正常与控制退出按已有事实结束 loan
和清理 owner。保持已支持的 basic / constant-enabled capability 边界。

基线为 PR40 merge `11acf62363d120b61700cd89d7b5db937d9ebabe`，PR 最终 head
`a88576d` 的 CI37193016600 全部10个任务成功。`feature/spec-0264` 从该 main 新建；
本地与远端均未发现0264占用。治理 P2 成本和其它工作区未提交项继续保留。

## 2. 证据与实施范围

[M1A](../../development/multifile-program-spec-draft.md) 的 A2 已完成，本片承接 A3 与直接
相关的 A6/A7/A9/A10。A4/A5 unit for 及完整三文件程序 A8 继续开放，不以本片替代。
实施前基线的 `lower_borrow_place` 接受 root、temporary、容器元素和 Rc 投影，但拒绝普通非根
place；前端已发布 typed aggregate projection 与 source-qualified loan target。

复用现有 heap payload / field place 和 BorrowBegin/End，不复制或提前取出字段 owner。
以同轮字段、receiver、source、类型与 layout 事实核对身份；错误事实显式拒绝。父绑定可为
val/var，字段 val/var 都允许只读 Borrow；调用结束后仍可合法读取及 replace var 字段。
调用参数 CFG 继续使用现有 pending loan 槽位和 frontend drop facts，Abort 不展开。

## 3. 非目标与边界

不扩展 nested/index/temporary receiver、借用参数 receiver、generic/value-class receiver、
普通字段 Inout ABI、字段赋值、借用返回或 unit for。已有 `this` 字段及容器元素能力保持。
SSA 按共同父 root 判别的 sibling exclusive 冲突仍明确拒绝，不假装支持全部 disjoint 投影。
基础入口原先不支持的参数控制退出不因本片放宽；常量专用入口的既有控制退出须覆盖新字段 loan。
不新增依赖、runtime ABI 或语言规则；ADR 历史 Abort 表述不取代现行 Guide 的不展开规则。

## 4. 单一验收账本

| ID | 合同与测试选择 | 实际结果 |
|---|---|---|
| B1 | 字段 Borrow 最小红测 | 新增 codegen native 测试在实现前返回 UnsupportedNode（source0，span248..259）；CLI 用例首次执行在实现后，不另计 CLI 红测 |
| B2 | 两入口 SSA/LLVM：callee 读取、连续借用、结束后 replace；source-qualified 身份与边界拒绝 | `unit_field_borrow` 七项通过；跨 source 同名不同 layout、正逆 inputs 得到同一 SSA；nested/generic/inline/parameter 与 sibling exclusive 保持拒绝 |
| B3 | native 按 pointer 核父对象及字段唯一释放；后续参数 CFG 正常、return、break、continue、Abort；输出 oracle 证明控制流 | 包含在上述七项：正常路径逐 pointer 唯一 free、旧字段→新字段→父对象顺序；constant CFG 双 flag 下输出区分三种退出；Abort 两入口2次分配/0次free且不执行callee/后继 |
| B4 | CLI project 跨文件 val/var 字段及父绑定、literal/const；build/artifact/run 精确 UTF-8 输出；借用冲突与输出保全 | 新增2项通过；正例8组合均build/artifact/run输出精确，冲突L0135及既有目标preflight保全通过；完整 `project_cli` 12 passed/0 failed/0 ignored/0 filtered |
| B5 | 相邻 field replace、Borrow/receiver 与 capability/交接回归 | 6个codegen过滤器共122个唯一测试通过，均0 failed/ignored；含新七项与新native身份矩阵，详见§7 |
| B6 | fmt、codegen/CLI Clippy、docs/inventory/尺寸、独立审阅；精确 head 双宿主 PR CI 与归档合并 | 本地fmt/严格Clippy/docs506/Python37+47/尺寸/diff通过，独立审阅发现已修并复核；实现 head 6cdaa2c 的 PR41 CI37194916797 全部10个job通过；本次归档后仍须核最终head门禁才合并 |

## 5. 执行与交付

1. 固定 B1/B4 输入；B1 新增 codegen 测试先断言失败，再实施。
2. 接通已有事实与 SSA 操作，B2/B3 证明生命周期；核验 B4/B5 直接消费者。
3. 更新 Architecture、M1A 承接关系与本表；独立审阅代码、测试 oracle 和尺寸例外。
4. 依用户授权提交、推送 PR，核对实际双宿主结果后归档；最终 head CI 全绿且无未决状态才合并。

## 6. 实施记录与剩余验收

新增私有 `field_borrow` 模块消费 projection/loan identity，现有 BorrowBegin/End 不变。
初始实现对“字段共享借用前缀后再 replace 同父 sibling”返回 InvalidSsa；新增回归证明后，
调用帧记录稳定父 symbol，两个方向都在后端入口明确 Unsupported。嵌套调用正常返回后
允许同一外层调用后续实参 replace，避免保守拒绝标记泄漏。

独立审阅发现 CLI 原保全断言未真正向既有目标 build、break/continue 输出不能区分。
前者新增实际目标 preflight 失败与 bytes 核对，后者用不同循环头次数证明 continue 回边；
新增 native handoff 矩阵另验证同字段程序的错配分析在既有/缺席目标下均失败原子。
最终复核又发现 Abort shim 的 `_Exit` 会丢弃缓冲输出；新增 `fflush(stdout)` 后，错误提前
执行 callee/后继将触发精确空 stdout 断言。受影响 native 单项（两入口）重跑1 passed、
795 filtered，fmt check 与 codegen 严格 Clippy 再次通过。
审阅复核 source/root/field/type、父 owner/loan、CFG 帧、测试 oracle 和单行尺寸例外通过。
`unit_lower.rs` 1252→1253 仅挂接新模块，原 baseline 保留；完整实现放在独立文件。

最新夹具、handoff 新矩阵及相邻回归/Clippy 已统一验证。完整 M1A CLI 再次返回 UnsupportedSource/UnsupportedNode，
unit for 尚未交付；不能把本片进展当作完整应用完成。

## 7. 本地命令与覆盖边界

本机 macOS arm64，LLVM21.1.8，Rust1.96.0；Cargo 使用 `--locked --offline` 与
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，全部串行执行。codegen 选择实际结果：

| `cargo test -p lang-codegen --lib` 过滤器 | passed | filtered |
|---|---:|---:|
| `unit_field` | 24 | 772 |
| `unit_lower_borrow_tests` | 2 | 794 |
| `unit_lower_receiver_tests` | 46 | 750 |
| `handoff_contracts` | 21 | 775 |
| `call_lifetimes_tests` | 2 | 794 |
| `unit_constant_tests` | 27 | 769 |

```sh
cargo test --locked --offline -p lang-cli --test project_cli
cargo clippy --locked --offline -p lang-codegen -p lang-cli --all-targets -- -D warnings
cargo fmt --all
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -p test_check_docs.py
python3 -m unittest discover -s scripts/tests -p test_check_rust_sizes.py
python3 scripts/check_rust_sizes.py --base origin/main
git diff --check
```

尺寸检查为754份手写 Rust、45份历史超千行；有限例外不清除历史欠账。
未运行本地 frontend/LSP 全量、Linux、sanitizer 或成本实验；没有公开类型变化，不额外
运行本地 workspace check。首轮双宿主 CI 证据见§8；不把本地定向结果替代远端记录。

## 8. 首轮双宿主与归档

[PR41](https://github.com/Halckon/Koven/pull/41) 实现 head
`6cdaa2c99f058c159fcf0dd8e0496c2ec1a4efaa` 的
[CI37194916797](https://github.com/Halckon/Koven/actions/runs/37194916797) completed/success，
全部10个job成功，包含两宿主workspace check/Clippy和Targeted Tests，无未决任务。
轻量CI子agent读取两份原始日志：七项 `ssa::unit_field_borrow_tests`、新增native
`direct_field_borrow_handoff_rejects_mixed_sources_and_preserves_targets`及两项
`project_cross_file_field_borrow`每host各恰一次ok。Ubuntu无ignored；macOS仅既有LLDB
断点测试因debugserver task-port限制ignored，新增用例未跳过。

独立实现与文档审阅已完成，发现及修复见§6。当前迁移只修改Spec状态、索引/inventory、
生成DAG和进度链接，不改变生产或测试。归档后的精确head仍需最终PR门禁全绿才合并，
不能用以上实现head的结果代替。M1A unit for与完整三文件验收继续开放。
