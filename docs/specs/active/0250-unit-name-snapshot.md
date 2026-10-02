# SPEC-0250：封闭 unit 名称前缀 owner

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或验收共享 unit 名称前缀时 · **唯一真源**：本 Spec 的有界合同与验收账本

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P6-250` |
| 所属 Phase | Phase 1→2 名称前缀与 Phase 6 CLI 编排；治理 P3b 首片 |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-02 已批准[整体治理计划 §7](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)及首片设计独立评审后的实施授权 |
| 基线 / 分支 | main `64ace382c2a634ebf19bab66da928242120930cd` / `feature/spec-0250` |
| 前置 Spec | SPEC-0025、0249 `done` |
| 前置 ADR | [ADR-0020](../../adr/accepted/0020-multifile-compilation-unit.md)、[ADR-0022](../../adr/accepted/0022-minimal-project-manifest-source-discovery.md) `accepted` |
| 阻塞项 | 新接口、parity与本地门禁通过；独立实现评审及Draft PR exact-head双宿主CI尚待执行 |
| 影响范围 | frontend analysis 的不可变 owner 与工厂、CLI project 前缀、契约及 stage 选择、架构与治理文档 |
| 语言语义变更 | 否；不改 Guide、长期架构、五 crate 边界、const 或 LSP 行为，不新增 ADR |

## 1. Goal 与非目标

共享已提供内存源码的 lex→parse→index→names 纯前缀，建立不可伪造且按值拥有数据的
`UnitNameSnapshot`；CLI project 是首个消费者。首个非空诊断 gate 仍在 names 后，
basic/const 分流、ownership、0249 借用 view、entry、native、IO 与输出原子性保持。

不迁 bootstrap 或 LSP/legacy，不实现完整 pass manager、AnalysisMode、缓存、generation ID、
VFS、标准库加载或新依赖。不扩 const/unit-for/语言功能，不提前开展排在整体计划之后的外部审计。
本片不声称完成 P3b、P2/P4/P5 或性能改善。

## 2. 输入、owner 与阶段合同

`analysis::analyze_unit_names(SourceMap, Vec<UnitSourceDescriptor>, NameEnvironment)` 返回
`Result<UnitNameSnapshot, UnitNameAnalysisError>`。descriptor 只拥有 root、logical path、
原 map 已注册 SourceId，字段私有且只读。列表显式限定 unit，可为空或 map 子集。

- 工厂移动原 SourceMap，不复制/重注册；逐 descriptor 按输入顺序 lex→parse，完成后才
  index→names。用户源码诊断返回 Ok recovery，不在 lexer/parser 提前停止，也不额外拼诊断
- 复用原 index、resolver、validate 与诊断全序，不改旧接口或内部 index 复核。错误枚举保留
  Lexer、Parser、Input、Name 原始分类；CLI 逐项映射旧 ProjectBuildError 与 Display
- index 成功后 descriptor 与 ParsedFile 同步按 root/path canonical 排序，AST 不 clone。
  snapshot 按值拥有 SourceMap、descriptor Vec、ParsedFile Vec、NameEnvironment 与 names
- names 直接保存既有 validate 返回的 Result（Validated 或 boxed Recovery），不同时保存大 clone；只读 getter 发布
  sources/descriptors/parsed_files/environment/names/validated_names。parsed_files 可直接以
  canonical SourceUnitId 顺序供消费者使用；不得暴露 mutable access 或 unchecked 构造
- inputs() 只创建借 snapshot 的局部 Vec，不保存自引用；消费者持有该 Vec，0249 view
  的生命周期仍覆盖 snapshot 与 inputs slice。禁止临时 Vec 逃逸、Pin/unsafe/leak 或 AST clone
- CLI 只调用一次 standard_environments，将 names 半边移动入工厂，保留同次 type 半边。
  names.validate 只拒 Error，但 CLI 必须继续拒绝全部非空诊断，不能互相替代

新 API 的故意非法 descriptor 合同与直接 index 不同：foreign SourceId 在 lex 阶段产生
`Lexer(Source(InvalidSourceId))`，先于尚未执行 index 才发现的非法 path；两个 foreign ID
按 descriptor 顺序选择。合法 SourceId 到达 index 后保持原 canonical path/source 校验。
混合非法 path/foreign ID 正反序分别精确断言 variant 与 SourceId；旧 index precedence 不改。

frontend 不 read/stat/scan、解析 manifest/URI、选择 entry、渲染消息、加载 LLVM、写文件或 spawn。
源码注册仍由 CLI 完成，其 Source 错误分类保持；实际 host 已验证 source 名称唯一且 ID 合法。

## 3. 实施、文件与交付

1. 本 Spec、inventory/索引先建立；新 API 编译红后最小实现，parity oracle 真失败→修复后通过
2. frontend `src/analysis/{mod,unit_names}.rs`、lib 出口；两新 integration targets 分别测试
   值/facts/诊断与外部 rustc 封闭性/生命周期；CLI 只迁 project_build 前缀
3. stage 选择与最小 policy 红→绿，纳入现有双宿主测试 job，不运行或声称 frontend 全量
4. 工程门禁通过后同步 Architecture/治理事实；代码与文档独立提交，独立 review 后 Draft PR
5. 精确 head 双宿主 CI 通过才完善账本及归档；归档另审、新 head CI，合并由维护者授权决定

所有新生产、测试、helper ≤1000 PLOC；已有超限零增长，不重生 policy 或扩大例外。
所有 Cargo 串行执行，使用现有 target，不 cargo clean，不清理其他 artifacts。

## 4. 唯一验收账本

| ID / 合同 | 精确目标与 oracle | 实际结果 |
|---|---|---|
| A1 同源全事实 parity | `unit_name_snapshot`：旧手工 lex/parse/index/names 与新工厂同 map/同 environment；空unit、双root/import/alias/overload、Unicode、map子集、输入/注册置换；完整 names/index/diagnostics 相等、canonical parsed/source 一一对应 | 7项target全部通过；六组注册/descriptor置换对旧手工前缀完整names/index/diagnostics同源相等，并按source key规范化跨map全事实；canonical AST/source真实红→绿见§5 |
| A2 recovery 与阶段顺序 | 同 target：混合 lexer/parser/name 完整诊断顺序、Span/source identity、details/notes；无重复，validated None；有效 names Some；止于 names 不提前检查 type/ownership | A1同次通过；混合L0001/L0080/L0079/L0009的顺序、完整旧诊断及Unicode后的精确byte span/source identity、关联details保全；错误为None、合法为Some，后续type/ownership不泄漏 |
| A3 输入分类 | 同 target：原 SourceMap move 身份仍有效；foreign ID、mixed path/foreign 正反序、两foreign顺序；合法ID重复key/source与非法path继承index错误 | A1同次通过；两种mixed顺序均精确Lexer(Source(InvalidSourceId))，两foreign按descriptor顺序；move原Span仍可slice、配对type环境合法而fresh同结构环境拒绝；原错误Display/source链逐字相等 |
| A4 封闭与借用 | `unit_name_snapshot_compile_contracts`：snapshot fields forge、inputs逃逸失败原因；positive 合法 move/inputs/0249 caller局部Vec | 完整target4 passed/0 failed/0 ignored/0 filtered；E0451/E0616封闭、E0597逃逸、E0308可变借用均有成功对照；A1同次真实0249局部Vec/view调用通过 |
| A5 CLI 原可观察行为 | 完整 `cargo test --locked -p lang-cli` 与 build；project basic/const 真实stdout/exit、名称/类型/所有权gate先于entry、失败产物/临时文件保全；原native oracle不变 | 旧前缀基线project_cli9/9（含3新gate）先通过；迁移后完整CLI69/69（bin48、format3、native9、project9）及build通过，真实basic/const/argv/String stdout/exit与失败原子性保持 |
| A6 相邻公开合同 | frontend targets `compilation_unit_index`、`multifile_name_resolution`、`owned_compilation_unit_view`、`owned_unit_view_compile_contracts`、`multifile_type_signature_provenance`；frontend doctests；codegen `native::unit_tests` | 七frontend targets合计42 passed（新7+4、旧index11/names10/view1/compile7/provenance2），均0 failed/ignored/filtered；frontend docs12/12；native unit93 passed/646 filtered/0 failed/ignored，保留既有真实link/run/资源oracle |
| A7 工程门禁 | fmt、workspace all-targets check、frontend/CLI严格clippy、docs、全Python policy、diff与base64ace38尺寸guard | fmt、最终workspace all-targets check、frontend/CLI严格Clippy、docs472 Markdown、全Python policy98/98、diff均通过；base64ace38尺寸686手写/48历史超限/0生成物且无增长，policy额度未改 |
| A8 双宿主选择与CI | 两新targets各恰一次未过滤stage选择policy红→绿；Draft exact-head双宿主jobs与逐名输出；归档新head另验 | stage selection policy零命中真红→各恰一次绿，相关policy14/14、全policy98/98；Draft/独立review/精确head双宿主与归档最终CI待执行，不由本地结果替代 |

本地实测 Rust/Cargo1.96.0、LLVM/Clang21.1.8、x86_64 Linux；全部Cargo命令追加`--locked --offline`串行执行，未clean或移除其他artifacts。
完整 frontend、未迁移LSP、性能/分配测量不在本片验收宣称中；macOS 由远端实际 CI 另验。


## 5. 新测试身份与失败历史

`unit_name_snapshot` 的完整测试名：

- `empty_unit_and_explicit_subset_match_the_manual_prefix`
- `registration_and_descriptor_permutations_preserve_all_name_facts`
- `recovery_keeps_complete_mixed_diagnostics_once_and_stops_after_names`
- `moved_snapshot_preserves_source_identity_and_the_matched_environment`
- `foreign_source_precedes_invalid_path_in_both_descriptor_orders`
- `multiple_foreign_sources_follow_descriptor_order_before_index`
- `valid_source_ids_keep_index_path_and_duplicate_classification`

`unit_name_snapshot_compile_contracts` 的完整测试名：

- `factory_and_read_only_getters_allow_snapshot_moves_and_local_inputs`
- `external_snapshot_construction_and_field_access_are_private`
- `projected_inputs_cannot_outlive_the_snapshot_owner`
- `snapshot_getters_do_not_expose_mutable_products`

CLI `project_cli` 新增三项（完整target包含原6项，旧断言保留）：

- `project_names_gate_aggregates_lex_parse_and_names_without_later_diagnostics`
- `project_type_gate_precedes_ownership_and_entry_in_source_key_order`
- `project_ownership_gate_precedes_entry_in_source_key_order`

先运行新API得E0432；首实现漏canonical配对重排，parity的两项真实失败分别为
SourceId(1)/SourceId(0)与SourceId(1)/SourceId(2)，完整配对move/sort后消除。
测试初稿误将合法token `@` 当lexer错误、将EOF表达式缺失误期望L0029；
旧手工与新snapshot完整facts一直相等，修正fixture为`#`和既有L0009，不修改语言行为。
首编译发现SourceMap不实现Debug，删除不必要的snapshot Debug派生；严格Clippy指出
重复自定义两态enum的216/8字节差异，改为直接存原validate Result，无lint例外或额外大clone。
selection policy先因新target零命中失败，接线后通过；这些失败不宣称旧生产bug。


### 本地执行命令与未运行项

```sh
cargo fmt --all -- --check
cargo check --locked --offline --workspace --all-targets
cargo clippy --locked --offline -p lang-frontend -p lang-cli --all-targets -- -D warnings
cargo test --locked --offline -p lang-frontend --no-fail-fast --test unit_name_snapshot --test unit_name_snapshot_compile_contracts --test compilation_unit_index --test multifile_name_resolution --test owned_compilation_unit_view --test owned_unit_view_compile_contracts --test multifile_type_signature_provenance
cargo test --locked --offline -p lang-frontend --doc
cargo test --locked --offline -p lang-cli
cargo build --locked --offline -p lang-cli
cargo test --locked --offline -p lang-codegen --lib native::unit_tests
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
python3 scripts/check_rust_sizes.py --base 64ace382c2a634ebf19bab66da928242120930cd
git diff --check
```

全部上列命令已通过。未运行frontend全量、全部stage/Guide、完整codegen、LSP行为或本地macOS；
公开API通过workspace check，未迁LSP没有行为diff。未新增性能/分配计数测量，不声称提速、
性能等价或index次数减少。P3b后片仍需独立能力/恢复与宿主协议验收。
