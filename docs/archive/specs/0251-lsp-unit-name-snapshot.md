# SPEC-0251：LSP unit 消费共享名称快照

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或验收 LSP unit 名称前缀迁移时 · **唯一真源**：本 Spec 的有界合同与验收账本

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P6-251` |
| 所属 Phase | Phase 1→2 名称前缀与 Phase 6 LSP 编排；治理 P3b 有界后片 |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-02 已批准[整体计划 §7](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)，名称快照首片后的 LSP 最小消费设计已独立只读评审 |
| 基线 / 分支 | main `08c7b0966115f2f709cbd8a9257a6e8b2d704429` / `feature/spec-0251` |
| 前置 Spec | SPEC-0187、0250 `done` |
| 前置 ADR | [ADR-0021](../../adr/accepted/0021-lsp-explicit-source-set-protocol.md) `accepted` |
| 阻塞项 | 本Goal按§6首轮双宿主证据有界完成；归档文档新head仍须独立窄审和最终CI，不自动合并 |
| 影响范围 | LSP 私有 unit_session owner、私有回归及当前工具架构/治理文档 |
| 语言语义变更 | 否；不改 Guide、frontend 公共 API、长期架构或五 crate 边界 |

## 1. Goal 与非目标

LSP `UnitSnapshot` 按值组合 SPEC-0250 的 `UnitNameSnapshot`，替代独立 sources/parsed/names
及重复 lex→parse→index→names 编排。保留 source_ids 的轻量 URI 对照、宿主聚合诊断、typed、
owned 与 definitions。同次 standard_environments 的 names 半边移动入快照，types 半边留本轮；
临时 inputs Vec 仅在后续分析借用期持有，不自引用、不 clone AST、不新增 mutable/unchecked API。

只改 unit_session 生产代码；legacy、CLI/bootstrap、source-set wire、definition 算法、位置/诊断
adapter、const ownership/fallback、VFS、异步 generation、新依赖及性能优化均非目标。
不宣称 P3b/P2/P4/P5 或整体计划完成；外部审计仍排在整体治理完成之后。

## 2. 不变合同

- names 仅由 validated_names 控制下一阶段，不采用 CLI 的非空诊断 gate；names 已含完整
  lexer/parser/index/name 诊断，不额外拼接。普通错误仍产生可提交 recovery 与有效名称导航
- typed.validate 失败仍保存 Some(typed) 并供 definition 收窄成功调用/成员；失败调用保留
  有序名称候选。仅基础 typed validated 进入旧 ownership；const 仍 owned=None
- source 注册保留 Source 错误；工厂 Lexer/Parser/Input/Name 四类逐项映射原 UnitSessionError
  variant 与 Display，不新增 wrapper。其余错误链保持
- sources、descriptor、canonical parsed、names、typed/owned 与 definitions 属于同轮；URI
  只是展示地址，source key 排序、空 unit、多 root、固定 membership 与无磁盘读取保持
- 先准备完整 snapshot 和全部 payload，按 source key 逐项 send，全部成功再 commit。分析/
  mapping/send 失败或 prepared drop 不提交。此为内存 last-good 原子替换，不是 wire 事务
- unit 版本严格递增，stale 拒绝不改导航；close 恢复 immutable base 与 None version。
  UTF-16/surrogate InvalidParams、越界/未知 URI null 及 legacy 原行为保持

## 3. 两层 oracle 与实施顺序

1. 旧完整 LSP 26 项基线和新增宿主行为 oracle 在旧前缀通过；消费 owner 的结构合同先编译红
2. 新生产 owner 内同 SourceMap/environment 重跑旧前缀，完整 names/index/diagnostics Eq；
   package/imports/roots/diagnostics/完整 AST 与 SourceId 配对，inputs 指针同源
3. cfg(test) 冻结旧全链，跨 map 比完整 publications（全部 URI/version/diagnostic 字段和顺序）
   与真实 URI/UTF-16 Locations、有序候选及 typed/owned presence，不抹除 Debug owner
4. 最小实现后复跑同一组；新测试按领域分文件≤1000 PLOC，不改旧断言/ignore 或扩大生产 pub
5. 本地门禁、独立实现 review 后 Draft PR；首轮 exact-head 双宿主逐名验收后补账本/归档，
   归档另审、新 head 最终 CI，合并由维护者门禁决定。代码与文档独立提交

## 4. 唯一验收账本

| ID / 合同 | 目标与 oracle | 实际结果 |
|---|---|---|
| A1 旧基线 | 完整 lang-lsp | main08c7b096 上26 passed/0 failed/0 ignored/0 filtered |
| A2 真实 owner 消费 | unit_session 私有同源全事实与 AST/source/descriptor/input 配对 | owner_contract 两项由E0609/E0277编译红到绿；完整37项通过，5类源码同map全部names/index/diagnostics与完整AST配对相等 |
| A3 宿主差分/恢复 | 旧全链：双 root/逆序/空unit、mixed lex/parser/names、typed recovery、ownership 与 const | 5项unit_session行为oracle旧前缀绿、新owner绿；完整publication字段/顺序及每个UTF16位置的Locations/error相等，成功overload/成员收窄、失败候选与阶段presence精确保持 |
| A4 生命周期 | 移位 overlay/close、v5/v4拒绝再v6、prepared drop、transport失败last-good、UTF16/null | 4项新增server与prepared-drop旧/新皆绿；完整Locations/publications/version核验，发送失败后原v6可重试；旧26含内部分析失败与surrogate InvalidParams保持 |
| A5 错误分类 | 四 variant 与原 Display 完整相等 | owner_contract精确discriminant、Display、Debug及原Error::source(None)全部保持；未新增生产注入入口 |
| A6 相邻公开合同 | frontend unit_name_snapshot、unit_name_snapshot_compile_contracts 完整 targets | 两完整targets分别7/7与4/4 passed，0 ignored/filtered |
| A7 工程门禁 | fmt、LSP strict Clippy、workspace all-targets check、docs、全Python policy、尺寸与diff | 全部通过；docs473、policy98/98、尺寸690手写/48历史超限/0生成，旧欠账零增长，policy无改动 |
| A8 双宿主 | 既有完整lang-lsp选集，exact-head Ubuntu/macOS 新旧测试身份逐名核验 | PR32首轮head9443e00的CI37068719547为9/9 success；两宿主原26+新增11共37名各恰一次ok，0 ignored/filtered；归档新head最终CI另验，详见§6 |

当前生产无 warning emitter；不虚构 warning 源码或开放封闭构造口，Error-only gate 由原
validate 合同及调用结构保全。未运行项、失败历史与后继结果将在本账本记录，不以设计评审
替代测试或用基底 CI 冒充新 head 验收。


## 5. 实际命令、测试身份与红绿历史

本地 x86_64 Linux，Rust/Cargo1.96.0、LLVM/Clang21.1.8；全部 Cargo 串行使用既有target，
无 clean。旧完整26与旧前缀35、新前缀37均0 failed/ignored/filtered。新增11个完整身份：

- `unit_session::tests::unit_snapshot_matches_old_pipeline_outputs_for_every_recovery_gate`
- `unit_session::tests::empty_unit_matches_old_pipeline_without_publications_or_targets`
- `unit_session::tests::typed_recovery_session_keeps_narrowed_calls_members_and_failed_candidates`
- `unit_session::tests::name_recovery_commits_complete_diagnostics_and_valid_navigation`
- `unit_session::tests::dropping_prepared_update_preserves_complete_last_good_state`
- `unit_session::tests::owner_contract::session_name_owner_matches_complete_same_source_manual_prefix_and_ast_pairing`
- `unit_session::tests::owner_contract::unit_name_analysis_errors_preserve_session_variants_and_messages`
- `server::tests::snapshot_lifecycle::source_set_provider_overlay_moves_definition_with_unicode_crlf_and_close_restores_base`
- `server::tests::snapshot_lifecycle::source_set_equal_and_older_changes_preserve_navigation_before_newer_commit`
- `server::tests::snapshot_lifecycle::source_set_publication_send_failure_preserves_committed_navigation_and_version`
- `server::tests::snapshot_lifecycle::source_set_definition_returns_null_for_out_of_range_utf16_positions`

新旧完整事实差分始终相等。测试初稿的“合法”provider直接返回借用String，实际有原L0133；
改为返回Int后合法。失败overload原码为L0123，初稿误期望L0096，修正fixture oracle后旧35全绿。
这些是测试修正，不是发现或修复生产语言bug。随后新增真实owner字段/From合同分别得到
E0609/E0277；最小实现消除编译红且37项全部通过。无删除旧断言、扩大ignore或新lint例外。

```sh
cargo test --locked --offline -p lang-lsp
cargo test --locked --offline -p lang-frontend --no-fail-fast --test unit_name_snapshot --test unit_name_snapshot_compile_contracts
cargo fmt --all -- --check
cargo clippy --locked --offline -p lang-lsp --all-targets -- -D warnings
cargo check --locked --offline --workspace --all-targets
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_rust_sizes.py --base 08c7b0966115f2f709cbd8a9257a6e8b2d704429
git diff --check
```

上列本地命令均通过。新测试/参考helper为292/204/123/344行，生产unit_session485行；
唯一生产diff在unit_session，frontend/legacy/source-set/position/definition算法/CLI零diff。
CI已完整选lang-lsp，未添加target或stage规则。完整frontend/CLI/codegen、native执行、
本地macOS和性能/分配测量未运行且不在本片本地验收宣称中；精确head双宿主远端另验。


## 6. 首轮精确 head 双宿主验收与有界归档（2026-10-02）

[PR32](https://github.com/Halckon/Koven/pull/32)保持Draft，首轮head
`9443e00e95f69b797e48dee986a7ba037f49245b`的
[CI37068719547](https://github.com/Halckon/Koven/actions/runs/37068719547)已9/9 jobs success。
双宿主check/严格Clippy、core、ownership iteration、stage、Guide均实际成功。
独立实现review无阻断，另外从当前worktree复跑完整37、docs473、policy98和尺寸690/48；
核验Cargo fresh路径/二进制SHA256与当前代码一致、原26身份保全、测试旧链机械等价。
发送失败测试动态覆盖首send失败；未改的同步循环保证任一send错误不commit，不宣称wire事务。

### 提交内容与父链

| 内容 | 本地提交 | GitHub提交 | 完整tree |
|---|---|---|---|
| 代码/测试 | `2be0370f13f6f5aba43f862a7bb9dcffb5a41658` | `94b69b3538e797337f72372591c477260fb7bd57` | `37cd91443fc7e4876d046b4ab43d8eed3ec8e7c4` |
| Spec/当前事实 | `04852e77d2ec88355c41090adf3592504b6ae489` | `9443e00e95f69b797e48dee986a7ba037f49245b` | `9794ca21162c1254819be4a65ca049fd98b59ab5` |

逐提交tree完全相同，fetch后两对diff均为空。GitHub父链为固定main08c7b096→94b69b3→9443e00。
两宿主test job实际checkout合成merge `c62393e7ede67d9fe0d57032551346044e814bfd`，fetch核实
双parent恰为base08c7b096与head9443e00，完整tree9794ca21与head相同、diff为空。
它是PR测试合成merge，不是真实合并记录；验收覆盖此精确head内容。

### 双宿主实际执行

| 检查 | Ubuntu 24.04 x86_64 | macOS 14 AArch64 |
|---|---|---|
| 完整LSP | 原26＋§5新增11，共37名各恰一次ok；37 passed/0 failed/ignored/filtered | 同37个身份各恰一次ok，37 passed/0 failed/ignored/filtered |
| frontend core / ownership iteration | 187 / 184 passed | 187 / 184 passed |
| codegen core / docs | 739 / 4 passed | 738 passed＋既有LLDB1 ignored / 4 docs passed |
| 完整CLI | bin48＋format3＋native9＋project9＝69 passed | bin47＋format3＋native9＋project9＝68 passed |
| stage / Guide步骤 | 两步success | 两步success |

实际日志为[Ubuntu job111043390469](https://github.com/Halckon/Koven/actions/runs/37068719547/job/111043390469)
与[macOS job111043390510](https://github.com/Halckon/Koven/actions/runs/37068719547/job/111043390510)。
以本地旧26与新37完整身份集合逐名核验每宿主恰一次，不凭总数或配置推断；未改target/CI选集。
macOS原LLDB ignore原因仍为CI缺debugserver task-port权限，未扩ignore，不把ignored计作passed。

本片只完成LSP unit名称前缀消费，归档后当前1 active/237 archive。bootstrap、legacy、const
完整owner、0182及其余P2/P3b/P4/P5仍未自动完成，不宣称性能改善，外部审计次序保持。
§4/§5本地命令及未运行记录保留；远端实际覆盖单列本节，不倒填成本地执行。
归档仅改文档/inventory/生成图，Rust与CI零diff；新head另行窄review及最终CI，终态留PR，
不能用本节首轮9/9代替最终验收，不自动转Ready或开启auto-merge。
