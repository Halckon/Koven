# P2 首片：LSP server 私有测试搬迁验收

> **性质**：有界测试搬迁验收记录 · **状态**：PR16 已合并；结构与正确性已验，性能与 P2 整体未完成 · **读取时机**：评审、复现或回退 LSP 测试首片时 · **唯一真源**：本页记录本片身份映射与实测；整体阶段账本由后续合并批次统一更新

## 范围与固定源码

2026-10-02 在 `main 34189046319a8b727285d471596647d5de56996e` 上建立独立分支。
本地纯搬迁提交为 `eab6f05ec696bb8826fed1f8923d3ce428fa98de`，只修改六个 LSP Rust 文件。
通过已连接 Halckon 账户的 GitHub connector 重建的远端纯搬迁提交为
`1f3991b6bbaeaf0a5f485bdb139b5b7efa07d3da`；两者 tree 都是
`888a221864020e86a015983753e47eaacb9ad001`，源文件 blob 与完整tree逐项验证相同。
本地使用仓库配置的提交身份；远端使用连接账户的提交归属，不把两者的元数据或SHA混为一谈。
没有生产行为、Cargo manifest/target/依赖、公开 API、测试数量或测试覆盖范围变化。
本记录另作 docs 提交，避免把文档状态维护混入代码搬迁；不改并行文档批次的阶段账本。

已批准整体架构治理计划中的软上限政策、
baseline/growth guard、其他 crate 迁移与受控性能样本仍需后继交付。
本片只证明私有测试外置与领域拆分方法的有界正确性，不代表 P2 整体完成或性能改善。

## 结构与私有边界

路径相对 `crates/lang-lsp/src/`；行数按 UTF-8 文本 `splitlines()`，包含空行、注释和字符串。

| 文件 | 物理行数 | 职责 / 实际测试数 |
|---|---:|---|
| `server.rs` | 499（原1747） | 生产实现原始字节不变；末尾只保留 `#[cfg(test)] mod tests;` |
| `server/tests.rs` | 224 | 共享 imports、TIMEOUT、12个私有 helper，四个私有子模块入口 |
| `server/tests/initialization.rs` | 80 | source-set 初始化成功与 InvalidParams；2项 |
| `server/tests/lifecycle.rs` | 139 | legacy 文档版本/关闭/shutdown、未知协议请求；2项 |
| `server/tests/definition.rs` | 287 | UTF-16、最新版本、跨文件目标与拒绝；3项 |
| `server/tests/source_set.rs` | 526 | overlay/base、协议错误、last-good、validation gates；4项及3个专用 helper |

四个子模块只经 `server::tests` 的 `cfg(test)` 入口可达，不使用 `include!` 拼接。
共享 `assert_scalar_definition` 和 `position_of` 留在私有父模块，同时服务 definition 与
source-set last-good 场景。`assert_publication_order`、`has_code`、`code_count` 只服务 source_set，
随该领域移动；不复制 helper，不增加 `pub` 或 `pub(crate)`。

## 完整测试身份映射

所有条目的 package 与 target 都是 `lang-lsp`，target kind 是 `bin`，继承 `cfg(test)`，
无平台 cfg、ignore 或 should_panic。LSP 没有 lib target，本批不使用 `--lib`。
11个叶函数名不变；完整身份按下表一对一增加领域段。其余15个LSP测试身份完全不变。

| 原 full_test_name | 新 full_test_name |
|---|---|
| `server::tests::source_set_initialization_accepts_valid_options_and_enters_session` | `server::tests::initialization::source_set_initialization_accepts_valid_options_and_enters_session` |
| `server::tests::source_set_initialization_returns_invalid_params_before_session_start` | `server::tests::initialization::source_set_initialization_returns_invalid_params_before_session_start` |
| `server::tests::memory_session_publishes_versions_clears_close_and_shuts_down` | `server::tests::lifecycle::memory_session_publishes_versions_clears_close_and_shuts_down` |
| `server::tests::unknown_request_gets_method_not_found_and_unopened_change_is_ignored` | `server::tests::lifecycle::unknown_request_gets_method_not_found_and_unopened_change_is_ignored` |
| `server::tests::definition_uses_utf16_latest_version_and_open_document_lifecycle` | `server::tests::definition::definition_uses_utf16_latest_version_and_open_document_lifecycle` |
| `server::tests::source_set_definition_resolves_imports_qualified_and_same_package` | `server::tests::definition::source_set_definition_resolves_imports_qualified_and_same_package` |
| `server::tests::source_set_definition_rejects_private_and_unresolved_targets` | `server::tests::definition::source_set_definition_rejects_private_and_unresolved_targets` |
| `server::tests::source_set_lifecycle_rebuilds_all_diagnostics_and_restores_base_text` | `server::tests::source_set::source_set_lifecycle_rebuilds_all_diagnostics_and_restores_base_text` |
| `server::tests::source_set_protocol_events_log_and_preserve_last_good_state` | `server::tests::source_set::source_set_protocol_events_log_and_preserve_last_good_state` |
| `server::tests::source_set_internal_analysis_failure_preserves_last_good_snapshot` | `server::tests::source_set::source_set_internal_analysis_failure_preserves_last_good_snapshot` |
| `server::tests::source_set_diagnostics_follow_parser_type_and_ownership_validation_gates` | `server::tests::source_set::source_set_diagnostics_follow_parser_type_and_ownership_validation_gates` |

已有 `server::tests` 过滤器实跑命中11项，原叶名称仍可单独过滤。
旧完整路径不再作为精确身份使用，应按本表替换；不能对旧完整名的零命中宣称通过。

## Move-aware 保真核验

旧源码从固定 main 的 Git blob 读取。对11个测试与15个 helper逐个提取完整 attributes、
签名和函数体，以去共同缩进的 UTF-8 SHA-256 对照新文件：

- 26个函数无增减；24个去共同缩进后字节hash完全相等
- `definition_uses_utf16_latest_version_and_open_document_lifecycle` 的 `.is_none()` 换到上一行；
  `assert_scalar_definition` 的 `let definition = ...expect(...)` 被 rustfmt 换行。
  两者均将旧函数去缩进后经同版 rustfmt，输出与新函数逐字相等，未改表达式或断言
- 全部 attributes 和88处 assert宏保留；369个普通字符串literal逐项一致，只规范化Rust反斜杠物理续行空白。
  内联 fixture、诊断码、URI、请求ID与期望位置保持；没有外部fixture或 include_str 路径需要迁移
- `TIMEOUT = Duration::from_secs(5)` 与未知请求静默检查的100ms保持；线程join和shutdown不变
- 生产 `server.rs` 的 cfg(test) 前缀字节完全不变；变化仅是内联测试块替换为私有文件模块入口
- 新旧 Cargo metadata的129个target所有字段相等，比较时只规范化worktree根路径。
  实际libtest list的26项集合精确等于本表11项映射加15项不变身份

复核可以用 `git diff 1f3991b6^ 1f3991b6 --color-moved=zebra --color-moved-ws=allow-indentation-change`
定位移动，再对固定旧/新源码逐函数比较。数量、hash与静态扫描是复核辅助，不是完整Rust语义证明；
实际编译、测试与普通release检查的结果另列如下。

## 本地命令与实际结果

宿主为 `x86_64-unknown-linux-gnu`。使用既有开发工具链：Rust/Cargo1.96.0、rustfmt1.9.0、
clippy0.1.96；共享 Cargo target目录，`CARGO_INCREMENTAL=0`，所有Cargo命令串行。
LLVM/Clang21.1.8用于workspace工具前提；本片没有执行LLVM/native行为测试。
下列命令均在最终代码上执行，退出码均为0；提交前后没有再改Rust文件。

| 实际命令 | 实际结果 | 单次墙钟秒 |
|---|---|---:|
| `cargo metadata --locked --offline --no-deps --format-version 1` | 129 targets；全部字段与基线一致 | 0.025 |
| `cargo test --locked --offline -p lang-lsp --bin lang-lsp -- --list` | 26 tests / 0 benchmarks；身份集合与映射一致 | 17.396 |
| `cargo test --locked --offline -p lang-lsp --bin lang-lsp server::tests` | 11 passed / 0 failed / 0 ignored / 15 filtered | 0.149 |
| `cargo test --locked --offline -p lang-lsp --bin lang-lsp` | 26 passed / 0 failed / 0 ignored / 0 filtered | 0.156 |
| `cargo fmt --all -- --check` | 通过 | 3.061 |
| `cargo clippy --locked --offline -p lang-lsp --all-targets -- -D warnings` | 通过；warnings视为error | 9.138 |
| `cargo check --locked --offline --workspace --all-targets` | 通过；仅编译/静态检查，不表示测试执行 | 11.187 |
| `cargo check --locked --offline -p lang-lsp --release` | 普通release配置通过；不是release tests | 8.324 |
| `git diff --check` | 通过 | 0.005 |

server窄测libtest报告0.10s；完整LSP报告0.11s。暂存后的 `git diff --cached --check` 亦通过，
提交后工作树干净。首次fmt检查只提示上述两处缩进改变引起的换行，修正后最终fmt通过。
测试清单命令含新worktree路径的编译，17.396s不能命名为受控冷编译基线。

## 明确未测、后继门禁与回退

- 冷/热 compile与link分段、RSS、重复样本、噪声区间与退化预算尚未采集。
  本片单次命令墙钟只用于执行记账，不能替代性能基线、证明无性能退化或计算提速百分比
- 性能基线明确留作后继待办：在进一步整合测试target或提出性能收益前，固定同一输入/工具链，
  分别采集旧/新结构的可比冷/热compile/link、执行与RSS样本，再确定噪声/预算。
  不通过cargo clean或复制共享target来制造冷样本；采样方法应先单独审阅
- 没有运行frontend全量、codegen/CLI/native测试、macOS、release tests、性能或二进制体积测量。
  workspace `--all-targets` check不能替代这些测试
- 本地提交尚未发布PR，未核验远端exact-head CI；P2代码首片的对外交付仍等后续PR与必需CI。
  正式发布前先与父任务协调main基底和docs批次，重核任何源码变更后的验证
- 可按纯搬迁commit单独回退六个Rust文件；执行账本与本记录单独维护。
  不用降低assert、增加ignore、改变target集合或生产API来绕过检查

## 合并后的交付核验（2026-10-02）

以上“待PR/CI”和本地未测项保留为发布前快照，不倒填为当时已运行。
[PR16](https://github.com/Halckon/Koven/pull/16) 最终head为
`3d1c2a24ef92afdf35d1aec3982b58aa0b02fc06`，tree与本地docs head
`9ec0bc51b323c0bf96f084d76b9bb5d5723105fe`一致。
[exact-head CI 36991173937](https://github.com/Halckon/Koven/actions/runs/36991173937)
8/8 jobs success，两宿主check、严格Clippy、core、stage及Guide步骤实际成功，
两宿主完整LSP均26 passed/0 failed/0 ignored/0 filtered，迁移的11项逐名核对一致。
用户随后合并PR，merge为`4383509dbfb805f774581a29136fc46dd62504a4`；
[该main CI 36993317772](https://github.com/Halckon/Koven/actions/runs/36993317772)
亦8/8 jobs success。以上不替代冷/热compile/link、RSS、重复性能样本或frontend全量验收。
