# SPEC-0240：整数具名位运算与取反端到端执行

> **性质**：实施与验证 Spec · **状态**：in-progress · **读取时机**：实施或核验整数位运算时 · **唯一真源**：本 Spec 的范围、验收与交付限制

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-BITWISE-0240` |
| 所属 Phase | Phase 2 常量与类型、Phase 3 receiver 读取、Phase 4 SSA/LLVM/native |
| 语言规范 | [Guide v0.40](../../guide/README.md)，不启用新语义 |
| 批准依据 | 2026-10-01 用户批准移位规则，并在合并 PR #7 后要求继续实施未完成计划 |
| 基线 / 分支 | main `e22e11b736aab1231209e3403bd0c931b9ddb940` / `feature/spec-0240-bitwise-execution` |
| 范围 | 六个具名二元位运算的 const/SSA/native、整数 `inv()` 的 typed 身份及两入口执行；Litmus12 正向门禁 |
| 非目标 | 不扩展 const call 白名单、不改变同类型 operand 规则、不修复 return-when 或其他既有失败、不自动合并 |
| 交付状态 | PR #8 双平台CI已通过并合入main `8f3e460`；归档状态另行闭环；与0241/0242的新整合验收见下 |

## 合同与边界

以 [整数位运算规则](../../guide/04-expressions-operators.md#整数具名位运算与移位) 为唯一语义真源。
常量资格仍遵守 [封闭 const expression](../../guide/05-declarations-callables.md#363-封闭-const-expression-与求值失败)：
具名二元位运算在白名单内，`inv()` 是 call，仍不允许出现在 const initializer。

共享纯值内核负责两条前端路径的整数常量计算。整数取反发布稳定、可回滚的 typed descriptor，
ownership 按整数只读路径处理 receiver，后端不凭成员拼写选择 intrinsic。SSA 明确区分位级操作
与 checked 算术；LLVM 在移位前屏蔽位数，不能产生由非法 shift count 导致的 poison。

测试覆盖所有整数类型、MIN/MAX、负数和超界移位、const/runtime 一致性、operand 一次求值及
错误类型/参数等独立负例。Guide Litmus12 直接读取规范源码，通过真实 native 后再更正覆盖账本。

## 验收账本

以下保留PR #8发布前本切片的原始本地快照；后续远端与整合状态见末节，旧限制不覆盖新切片成果。

本地 Debian 13 x86_64 + glibc，Rust 1.96.0、LLVM/Clang 21.1.8；通过工作区 `rust-dev/activate.sh`
启用工具，`CARGO_TARGET_DIR=/workspace/shared/koven/target`、`CARGO_NET_OFFLINE=true`，Cargo 门禁串行。
frontend 使用受影响的定向套件，
不运行全量 frontend；五项 multifile 与三项 call-argument 的既有失败不属于本切片。

| 验收项 | 命令 / 证据 | 实际结果 |
|---|---|---|
| inv 行为红测 | `cargo test --locked -p lang-frontend --test integer_inv` | 原实现 2 passed / 4 failed；非法参数未拒绝、ownership deferred、独占借用读取漏报 |
| const 红测 | `cargo test --locked -p lang-frontend --test bitwise_constants` | 原实现 1 passed / 5 failed；六操作被 L0156 拒绝 |
| SSA 红测 | `cargo test --locked -p lang-codegen --lib bitwise_lowering` | 两入口各一个失败，均为 UnsupportedNode |
| const / inv 前端正反例 | `cargo test --locked -p lang-frontend --test bitwise_constants --test integer_inv`，由最终阶段脚本再次覆盖 | 6 + 10 passed；每条常量入口 236 个整数计算，全部 0 failed / ignored / filtered |
| 后端与真实 native | `cargo test --locked -p lang-codegen` | 531 tests + 4 doctests passed；全部 0 failed / ignored / filtered，含本次 21 条 bitwise 测试 |
| fmt | `cargo fmt --all -- --check` | passed |
| 全 workspace 编译 | `cargo check --locked --workspace --all-targets` | passed |
| 全 workspace 严格 lint | `cargo clippy --locked --workspace --all-targets -- -D warnings` | passed，未添加 lint 豁免 |
| frontend 核心 | `cargo test --locked -p lang-frontend --lib` | 180 passed；不是全量 frontend |
| CLI / LSP | `cargo test --locked -p lang-cli -p lang-lsp` | CLI 66 passed（48 + 3 + 9 + 6），LSP 26 passed；0 failed / ignored / filtered |
| 阶段整合 | `bash scripts/check_stage_integration.sh` | 52 targets / 645 passed；0 failed / ignored / filtered |
| Guide 与原生 Litmus12 | `bash scripts/check_guide_litmus.sh` | 文档445；frontend 140 passed（6 + 21 + 10 + 72 + 31），native 2 passed / 529 filtered；0 failed / ignored |
| 文档结构 | `python3 scripts/check_docs.py` | 445 Markdown passed；结构不证明语义等价 |
| 文档与 CI 脚本策略 | `python3 -m unittest discover -s scripts/tests -v` | 45 passed |
| 格式 / shell | `git diff --check`、`bash -n scripts/check_stage_integration.sh scripts/check_guide_litmus.sh` | passed |
| 独立只读审查 | 对 const、typed facts、ownership、SSA/LLVM 与测试逐层检查 | 无确认的生产阻塞；补充跨文件局部 ID 碰撞与输入反转测试已通过 |
| 发布、双平台 CI、合并 | 单独核验 | 未运行 |

## 运行证据与遗留范围

- 每条 native 入口覆盖 304 个二元边界结果、六个 eager operand 顺序检查，40 个 inv 边界及
  receiver 一次求值；另以 48 个样例逐项比较 const、runtime 与独立十进制 oracle。
- 两 native 入口直接从 Guide15 提取 Litmus12 原源码并执行，stdout 为 `Mask initialized`；
  原 L0156 known-gap 改为正向检查。Litmus4 的 return-when 缺口和其他 typed 快照继续保留。
- 命名父 owner 字段、literal chain、带 return 的 receiver/RHS 已实际 native；unit 另验证
  Borrow/命名 List 元素及源码同名 inv 方法。单文件通用整数 Index value lowering 和 source
  member call 仍是既有 UnsupportedNode 边界，各有裸操作/同名方法对照负例，没有通过删掉
  失败或放宽断言宣称支持。临时对象字段的既有 ownership deferred 边界亦未扩大；没有新增 deinit/drop 可观察时序的证明。
- 本地仅 Linux；macOS 未运行，新分支尚未 push/创建 PR，未请求合并或启用自动合并。

保持 in-progress；本地通过不等于 PR CI 或 main 已交付。

## PR发布与后继整合状态

PR #8的最终head `57a181695f4408378140fae1cea40caaa396bfae` 已通过
[双平台pull_request CI](https://github.com/Halckon/koven/actions/runs/36884583273)，
8个jobs全部success；2026-10-02核实其已合入main `8f3e460ba2e186a4fbbcb8ea63a671e1c318b7fa`。
以上旧本地账本中的未发布、未运行macOS及Litmus4缺口属于当时切片快照。

PR #9正在把该main与SPEC-0241/0242合并：保留位运算与inv合同，同时由0241修复
return-control并执行Litmus4单文件native，由0242移除调用侧Borrow marker。
原始Litmus4与12现在均为两入口frontend正向检查；native仍分别限定为4的单文件入口
和12的双入口，不扩张其他typed/native边界。组合后的实际验收统一见
[SPEC-0241整合账本](0241-return-control-operands.md#最新-main-整合验收)。
