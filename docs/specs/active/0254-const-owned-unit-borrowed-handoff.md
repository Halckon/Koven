# SPEC-0254：const owned-unit 封闭借用交接

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 const unit 的 frontend→codegen 交接时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-254` |
| 所属 Phase | Phase 3→4 capability 交接及 Phase 6 CLI project 适配；治理 P3a const 剩余 |
| 语言规范 | [现行 v0.40](../../guide/README.md) |
| 前置 Spec | SPEC-0249、0252、0253 `done` |
| 前置 ADR | [ADR-0020](../../adr/accepted/0020-multifile-compilation-unit.md) `accepted` |
| 阻塞项 | 本地实现、验收与独立review通过；发布及精确head双宿主CI待完成 |
| 语言语义变更 | 否 |

## 1. Goal 与范围

从 PR34 真实 main `c6b84ecb46563b7de2bfeb9f481e75cf4a861323` 建立 `feature/spec-0254`，
先确认远端无同编号分支或PR；独立审阅的0253归档本地提交随本片一份 Draft PR 发布。
遵循[治理计划](../../development/engineering-governance-plan.md#p3-封闭交接与共享编排-l-分两片)：
只把 const unit 六输入封闭为受控只读借用，消除交接重复 index，保全所有来源与能力检查。
只迁 CLI project const 分支；保留旧 const native/lower 签名与错误、副作用顺序。

不改所有权/类型/短路算法、ABI、runtime、Guide、dependency、basic能力或LSP const无owned。
零常量声明不等于basic：专用ownership仍有来源标记及短路事实，同源码const loans=0而basic=1。
不新增bool统一模式、raw/recovery升级、empty-const转换、自引用owner或公开SSA/planner API。
不关闭0182、其余P2/P4/P5；外部审计仍在整个计划后，不从index计数宣称耗时/RSS改善。

## 2. 受控工厂

frontend独立 `ownership_checking/const_handoff.rs` 定义并由门面重导出：

```rust
pub struct ConstOwnedCompilationUnitView<'view, 'parsed: 'view> { /* 私有六借用 */ }
pub fn const_owned_compilation_unit_view<'view, 'parsed: 'view>(
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ConstEnabledTypedUnit,
    owned: &'view ConstEnabledOwnedUnit,
) -> Result<ConstOwnedCompilationUnitView<'view, 'parsed>, OwnedCompilationUnitViewError>;
```

字段`sources/inputs/names/_environment/typed/owned`全部私有不可变借用。只读getter为
`sources/inputs/names/types/ownership/constant_ownership`；后者返回原`&ConstEnabledOwnedUnit`，
保留materializations与short_circuits，其他facts仍为既有只读引用。不复制AST或第二份index。

工厂顺序：一次`index_compilation_unit`，失败`MismatchedSource`；比较names index；复用
`signatures().is_compatible_with_index`核saved/input/name index、environment owner、names
owner长度和逐项身份；最后`owned.is_compatible_with(typed)`。后三类为`MismatchedAnalysis`。
合法clone、重排inputs、同T0重查ownership与完整fresh链允许；fresh T1+O0等混轮拒绝。
使用既有错误enum与From映射，不增加新的provenance算法或暴露indexed helper。

## 3. 消费者与副作用合同

新公开`emit_native_const_owned_unit_object(&view, entry, output)`；旧
`emit_native_constant_unit_object`先执行用户`entry.into()`恰一次，再factory后转接。
新crate-private `lower_const_owned_unit_with_entry`把原facts与`Some(view.constant_ownership())`
送入既有私有driver；旧`lower_constant_unit_with_entry`一次factory转接，保持可见性。

factory→entry shape→lower/SSA verify→native entry plan→reserve→LLVM/layout/emission→commit。
前半拒绝reserve增量0；LLVM/layout仍在reserve之后。`SiblingObject`算法与发布尾部不改。
复用native既有错误kind、span=None、diagnostic=None与完整Display；CLI factory错误继续
走codegen_error，原project entry selection先于factory。实际link/emission失败边界不扩大。

## 4. 实施与唯一验收账本

先独立设计review，再旧生产六输入oracle与动态index基线；新API真实编译红后才最小实现。
全部Cargo串行、复用既有工具链、不clean；新文件小于1000行、不提高历史baseline或例外。

| ID / 合同 | 目标与精确证据 | 实际结果 |
|---|---|---|
| A1 旧六输入失配 | const native/lower六维替换、root/duplicate/missing，合法/非法entry、新旧output与missing parent；kind/span/diagnostic/Display及目录/bytes | 旧生产9/9通过：50 native/20 lower失配；实现后两路100 native失配，精确kind/span/diagnostic=None/Display及目录、bytes全部通过 |
| A2 接缝成功等价 | clone分别/全部、重排/重建/同源同字节reparse inputs、same typed重check、完整fresh链；facts/verified SSA/entry/object/link/stdout/stderr/exit | 10合法链×双路20次实际link/run通过；完整const事实、SSA/entry和object bytes相等；同map同字节reparse保留合法，fresh names/typed混轮仍拒绝 |
| A3 Into与reserve | 用户Into成功/失败恰一次且先identity；exact子进程读取既有counter，拒绝delta0/成功delta1 | 旧exact子进程reserve最终6；双路最终12，每次失败delta0/成功delta1；用户Into四格原顺序通过 |
| A4 工厂完整facts | const_owned_compilation_unit_view：六getter原指针、物化/短路列表和查询、零常量loans0/1及owned来源保护 | 两新frontend target真实E0432红后最小实现；本target4/4通过，六原借用、物化/短路查询、零常量loans0/1及owned来源保护 |
| A5 外部编译正负合同 | const_owned_unit_view_compile_contracts：全部private、basic/recovery/view互换、六产品/ParsedFile/root/path/temporary借用逃逸、只读getter，精确错误码及正控 | frontend compile10/10，E0451/E0616/E0308/E0597/E0716/E0624均配正控；实际公开native API另2/2，两类view及旧typed/owned分别隔离 |
| A6 动态index | 固定两const fixtures；setup、显式index校准、native/lower、factory、预建view native/lower；函数入口和constructor双counter | 22 exact进程成功，双counter两fixture均native4→1、lower2→1、factory1、view下游0；原始计数/fixture/hash及6次object/link/run见[测量页](../../development/const-owned-unit-handoff-measurement.md)，未做可支持结论的耗时/RSS实验 |
| A7 能力与资源回归 | 原constant materialization、短路、String drop/alloc/free、native原子输出、完整CLI与LSP const无owned | 完整codegen748+新native compile2+docs4、CLI82、LSP45全部通过；const String资源计数/短路与原basic资源oracle、输出失败原子性及LSP const无owned均保留 |
| A8 工程验收 | fmt、workspace all-targets check、受影响三crate strict clippy、frontend/codegen docs、定向frontend、完整codegen/CLI/LSP及CLI build | fmt、workspace check、三crate严格Clippy及CLI build通过；11个frontend完整targets267、frontend docs12通过；完整下游见A7，均0failed/ignored/filtered |
| A9 文档/尺寸/CI接线 | docs、Python policy、固定main尺寸、diff；新增两target及multifile_constant_ownership在stage各恰一次完整token无filter，先红再绿 | docs478、Python102/102、固定main尺寸707手写/48旧超限/0生成通过，无额度变化；selection未接线0命中真红后绿，并补同命令重复token反例；diff通过 |
| A10 交付 | 独立实现review；本地验证后一次Draft PR；精确head双宿主实际CI，归档与最终主干核验 | 设计与实现独立review均Approve；diagnosticNone与token计数缺口已修复并重验；未发布Draft PR、未跑本片双宿主CI |

本地Linux不替代macOS；未执行/失败/filtered/ignored分别留证。性能/RSS不由静态调用数或
插桩耗时推导。没有本片精确head双宿主证据前不归档、不宣称完整P3完成。

## 5. 实际执行与失败历史

旧生产oracle commit `eb1c2fe`，最小实现 `0d5ba88`；后继文档不改变该生产/测试字节。
新API实际编译红不是旧语言bug；旧native首次7/9的两个失败均为测试误以为同源同字节
reparse会改变index，改为成功控制后9/9。保留names/typed/owned混轮拒绝，没有扩大生产语义。
测量首次fixture的package/path错误及修正只记录于测量页。独立review补足diagnostic=None
与同命令重复`--test`计数反例；完成后完整codegen和Python政策重跑，不把此前结果代替新断言。

本地x86_64 Linux，Rust/Cargo1.96.0与LLVM/Clang21.1.8。全部Cargo串行，未clean。
定向frontend完整选择11个targets：compilation_unit_index11、multifile_constant_facts6、
multifile_constant_ownership20、multifile_ownership_checking72、multifile_two_phase_borrows27、
multifile_type_checking107、multifile_type_signature_provenance2、ordinary view1/compile7、
const view4/compile10，共267；没有用frontend/workspace全量替代定向合同。

```sh
cargo fmt --all -- --check
cargo check --locked --offline --workspace --all-targets
cargo clippy --locked --offline -p lang-frontend -p lang-codegen -p lang-cli --all-targets -- -D warnings
cargo test --locked --offline -p lang-frontend --no-fail-fast --test compilation_unit_index --test multifile_constant_facts --test multifile_constant_ownership --test multifile_ownership_checking --test multifile_two_phase_borrows --test multifile_type_checking --test multifile_type_signature_provenance --test owned_compilation_unit_view --test owned_unit_view_compile_contracts --test const_owned_compilation_unit_view --test const_owned_unit_view_compile_contracts
cargo test --locked --offline -p lang-codegen
cargo test --locked --offline -p lang-cli
cargo test --locked --offline -p lang-lsp
cargo test --locked --offline -p lang-frontend --doc
cargo build --locked --offline -p lang-cli
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_rust_sizes.py --base c6b84ecb46563b7de2bfeb9f481e75cf4a861323
git diff --check
```

codegen本地748 lib +2 integration+4 docs；CLI48 bin+3format+9native+9project+9unit oracle+
4single-file oracle=82；LSP45、frontend docs12，所有完整targets无ignore/filter。
focused旧/新native oracle各9通过时739 filtered单列；测量22进程各1 passed，旧739/新748
filtered不是完整suite。宏观stage/Guide脚本、frontend全量、workspace全量与macOS本地未运行；
远端精确head需实际执行配置选集，不能用本地或PR34结果替代。0253归档随本片一次PR发布。
