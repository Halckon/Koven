# 本机工程治理重建

> **性质**：本地恢复验收账本 · **状态**：current · **读取时机**：核对本次恢复范围与真实验证时 · **唯一真源**：本页

基线PR36 main `201d415d126d86183275d86ff2bc45caae6586a4`。丢失HEAD/tree未在本机找到；
按授权清单重新实现，不声称逐字节恢复、保留旧SHA或恢复27个提交。
既有`.gitignore`和未跟踪网站部署workflow保留，未纳入恢复提交。
按用户授权仅本地main逻辑提交，身份halckon/halckon0@hotmail.com；不推送/PR/上传/备份。

## 本地实施提交

| 提交 | 范围 |
|---|---|
| `8a178bd` | 0256–0258：教程、CLI logical root、组合与直接依赖门禁 |
| `5f8e5e3` | 0259–0260：真实LLVM发射失败与共享source查询 |
| `4496702` | 0182/0261：native/资源/ZST/provider矩阵、只读事实校验及集中实测修复 |
| `1084c13` | 0261只读自查的最近callable Return边界修复，真实红转绿与下游CFG定向复验 |

这些是新生成的本机提交，不对应丢失27个提交的逐一重建；最终验收文档另行提交。

## 15组恢复落点

按授权的原15组清单列实际落点，不宣称原文、原patch或原SHA恢复。

| 组 | 恢复内容 | 主要落点与证据 |
|---|---|---|
| 1 | 0256教程、CLI root、编辑器/std接线 | tutorial Markdown/manifest、check_tutorial.py、CLI discovery；7+2实际合同通过 |
| 2 | 0257有界组合 | check_core/integration/stage；77唯一target、10次配置调用；失败传播Python合同通过 |
| 3 | 0258直接依赖与required CI | metadata五成员四边、各种声明形态和拒绝矩阵；6项接线/metadata合同通过 |
| 4 | 0259真实LLVM发射失败 | ordinary/const×legacy/view×存在/不存在八格；真实write_to_file及TLS后续object/link/run |
| 5 | 0260共用source query | unit_source_query/planner/lower；borrow identity、first match与missing None Span通过 |
| 6 | SPEC0182拥有源迭代矩阵与条件drop | owned_source_tests/constant_presence；三容器×两种来源×0/1/3×四退出72格、source-once/last-use与有限presence证明通过 |
| 7 | Cell字段与Borrow交付 | aggregate/borrow_argument/string_clone；Int copy、String Borrow与root存活通过 |
| 8 | 真实for失败原子性 | single/unit actual-for整目录bytes、LLVM/reserve与fresh-chain；unit-for准确拒绝，独立普通unit成功 |
| 9 | 直接资源类容器元素与owner退出 | resource_deinit/phi_state；direct36格、存活非ASAP guard和动态/非source拒绝通过 |
| 10 | 资源值组件/有限嵌套退出 | owned_source_tests；Parts72格、partial/discard/nested实际cleanup与计数通过 |
| 11 | 合成ZST析构测试 | synthetic_zst_tests；18格实际SSA/object/run，保留drop loop及独立logical/storage计数通过 |
| 12 | Provider lifetime/provenance | provider_lifetime/reborrow；实际edge并行重绑定、tombstone、stale拒绝与预算fail-closed通过；旧swap误报保留 |
| 13 | SPEC0261有限只读fact validator | frontend私有mutation、外部sealed编译合同、native优先级；lambda Return红测4/1后5/5转绿，下游CFG3/3通过，最终Clippy通过 |
| 14 | 文档归档/架构映射/本地交付收口 | 0255、新Specs、Architecture、0182六映射与本机验收；751手写/45欠账通过，额度仅收紧；仅本地归档，远端CI未运行 |
| 15 | P2成本证据缺口 | 旧raw丢失、预算接受未授权；缺口未关闭，无新成本实验或外部审计 |

## P0–P5退出映射

本表只映射本轮恢复切片，不是原P0–P5全部退出的声明。
原P2成本raw已丢失，预算接受没有本轮授权，不能依据历史总结关闭；
45项尺寸欠账与后继职责、Linux/远端CI及计划之后的外部审计分别保留。

| 治理目标 | 本次恢复材料 | 实际状态 |
|---|---|---|
| P0 范围与基线 | 本机Git/source身份、工具链与清单 | 只读调查完成；没有丢失后继对象 |
| P1 生命周期 | 0255收口、新0256–0261与0182验收映射 | 本地0 active / 248 archive；不冒充远端发布 |
| P2 职责与成本 | 借用实参、reborrow、scalar materialization职责拆分 | 有界拆分与尺寸门禁通过；成本raw/预算接受未闭合，45项历史欠账保留 |
| P3 已发布交接 | 保留普通/const sealed capability和公共失败原子性 | codegen库、外部2项及frontend五组外部31项均通过 |
| P4 有限共享与provider | 0259/0260、0182矩阵、0261 | codegen787通过/1既有ignore，Return修复后5+3定向与最终严格Clippy通过 |
| P5 当前教程 | Markdown真源、完整JSON预期、真实CLI | 7正例真实build/artifact/run、2完整JSON负例通过；1planned不执行 |

## SPEC0182六项映射

| 原验收项 | 恢复证据 | 状态 |
|---|---|---|
| source once与provider | 3容器×owned/Borrow×0/1/3×四退出及factory/last-use | 完整codegen库内72格与source-once/last-use通过 |
| element Binding/Copy/Borrow | Cell字段及Parts部分/丢弃components | Cell与Parts72格在完整codegen库内通过 |
| CFG/cleanup | 资源元素、有限nested、退出path只读事实和source依赖 | direct resource36格、Parts72格和独立provider provenance测试通过 |
| ZST/资源精确计数 | 合成test-only MoveOnly ZST、空glue插桩、独立storage pointer计数 | 完整codegen库内18格通过；无源码ZST能力或ZST逆序可观测声明 |
| 拒绝与落盘原子性 | 实际unit-for混合SourceMap、畸形SourceUnitInput、整目录bytes/LLVM/reserve/正例 | single实际for成功及拒绝矩阵通过；unit实际for准确拒绝，独立普通unit正例成功恢复，未扩展unit-for |
| 确定性 | 独立分析链既有determinism测试与共享查询身份合同 | 完整codegen库内fresh-chain与共享查询合同通过 |

## 成本与外部范围

旧7f73357/ccd7538与raw丢失，不伪造。历史31预算/27通过/4噪声仅为背景，
本次未跑成本实验；功能恢复后再讨论是否重测及有界规模。外部仓库审计不在本次范围。
所有旧云端结果仅是历史；本次未运行/filtered/ignored和Linux CI分别记账。

## 本机集中验证

以下为本次 Mac 实际执行结果，旧云端结果不计入：

| 命令 | 结果 |
|---|---|
| `python3 -m unittest discover -s scripts/tests -v` | 108项通过；0失败 |
| `python3 scripts/check_workspace_dependencies.py` | 5成员、4条内部直接声明边通过 |
| `python3 scripts/check_docs.py` | 最终归档后494页结构检查通过；不代表语义验收 |
| `python3 -m unittest discover -s scripts/tests -p 'test_check_docs.py' -v` | inventory变更后37项通过；其他不变policy复用集中108项和接线补充6项结果 |
| `python3 scripts/check_rust_sizes.py --base 201d415d126d86183275d86ff2bc45caae6586a4` | 最终751手写Rust文件、45项超千历史欠账；通过，无新增额度 |
| `cargo fmt --all -- --check`、`rustfmt --check scripts/rust_test_artifact.rs` | 最终Rust输入通过 |
| `cargo check --locked --offline --workspace --all-targets` | 实测修复后集中check通过，1分44秒；之后Return边界修复以5+3定向与最终all-targets Clippy覆盖，不重复无关测试 |
| frontend/core库 | 190通过；0失败/忽略/过滤 |
| codegen/core库 | 787通过；0失败/过滤，1项既有LLDB权限忽略；554.15秒 |
| codegen外部编译合同、doctest | 2、4项分别通过；0失败/忽略/过滤 |
| CLI、LSP、std | CLI各目标合计82，LSP45，std1；均通过，std doctest为0项 |
| ownership_iteration | 184通过；0失败/忽略/过滤 |
| `bash scripts/check_stage_integration.sh` | 75个target、994项通过；0失败/忽略/过滤；第一组39/681，第二组19/289，第三组17/24；frontend五组外部合同共31项均通过 |
| `cargo test --locked -p lang-frontend --test guide_litmus` | 23通过；0失败/忽略/过滤 |
| `python3 scripts/check_tutorial.py` | 7正例真实build/artifact/run、2完整JSON诊断负例exit2且无artifact通过；1planned未执行 |
| Return边界定向复验 | frontend validator5通过/187过滤；下游source CFG3通过/785过滤；均0失败/忽略，原红测4通过/1失败保留 |
| 最终workspace all-targets严格Clippy | 修复后的最终代码通过，24m 23s；0警告/失败 |

环境为Rust 1.96.0、macOS arm64、LLVM/Clang 21.1.8。Cargo串行，未执行`cargo clean`。

首轮组合在codegen停止：frontend190通过；codegen777通过/6失败/1既有LLDB权限ignore，
437.16秒。失败分别是test-only variant词法guard、两个已终结block的负例构造、
unit-for正例超出现有能力及两项resource支持缺口。已修复构造与guard；unit按原能力准确分开
真实for拒绝和普通unit恢复；resource门禁、仍存活非ASAP owner出口保留及有限presence证明已修复。
随后direct resource36格与Parts72格各1项定向测试通过；临时test诊断打印已删除。
修后完整库已经通过。组合在codegen外部合同遇到旧Rust flags产生的两份有效frontend rlib，
原helper要求目录内唯一artifact而失败。已改为读取当前测试Cargo 1.96 fingerprint，
选择实际所链接的唯一依赖；保留旧variant，不clean、不按时间戳选择。
该外部合同2项定向复验通过，再按组合原顺序续跑未执行目标；已通过且输入未变的库不重复运行。
续跑覆盖codegen doctest、CLI/LSP/std、ownership_iteration、完整stage、Guide与教程，退出0。
两次组合尝试与有序续跑共同覆盖组合全部77个唯一frontend integration target；
这不是一次完整组合脚本退出0的声明。workspace check通过；Return最小修复后最终all-targets严格Clippy通过。


## 最终本地状态与保留项

本次代码重建及Mac选定范围验证完成；不宣称原27提交/patch/SHA恢复，
不宣称原P0–P5全部验收完成。P2成本raw/预算接受、45项历史尺寸欠账、
Linux/远端CI与后置外部审计保留，未启动新成本实验。
只读自查定位并修复最近callable Return误拒绝；这是本轮自查，不冒充独立第三方review。
