# SPEC-0298: N1a 范围绑定与有界局部控制流

> **性质**：有界变更合同 · **状态**：done · **读取时机**：准备范围绑定与标量条件组合时 · **唯一真源**：本页的冻结首片与开放后继；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0298` |
| 所属 Phase | Phase 3来源/终止事实消费、Phase 4 SSA/native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[N1a](../../guide/12-collections-destructuring.md#n1a-单来源连续范围-carrier) |
| 批准依据 | 2026-10-12按用户持续主线授权，在0299合并后从实际main新建`fix/spec-0298`；只实施本页冻结首片，不授权发布 |
| 前置 Spec | [SPEC-0289](0289-n1a-range-carrier.md) |
| 前置 ADR | [ADR-0030](../../adr/accepted/0030-range-carrier-source-facts.md) |
| 阻塞项 | 冻结首片已完成本地验证；发布待独立批准，原广义目标继续开放 |
| 影响范围 | 两入口borrow-result能力门与直接测试；复用既有CFG lowering |
| 语言语义变更 | 否；若所选程序超出现行Guide，先停下形成独立语义决定 |

## 1. Goal 与重起草来源

让已批准的单来源新范围描述符绑定，与有界标量表达式及局部if组合共存；只开放具有完整
origin、loan continuation/end和SSA运输证明的路径，不直接删除全函数能力门。

2026-10-12原未发布草案文件丢失，本页依据已核Goal/非目标、现行Guide与源码重新起草，
不是原文件的逐字恢复。原审计固定事实见[历史摘要](../migrations/pr72-74-followup-audit.md)；
摘要中的旧静态检查不能作为本页或功能验收。

## 2. 起点与选择边界

基线`8c6907c2`的`ssa/borrow_result_support.rs::binding`按包含binding的最小函数Span
扫描，不是只扫描loan活跃区间。除真实Int五种算术及同root范围调用前缀if/return外，
Binary/If/When/Return和while/loop仍可被拒绝。因此binding前、存活期间、已结束后
各位置都需普通源码对照，不能从“不相关语句”直接推定其loan已结束。

本次冻结首片先选择真实Int的`==`与`<`，分别核对双操作数Int、结果Boolean；
其余比较运算符不批量开放。随后只接同根、两arm正常完成的statement-if：显式词法scope
内保持描述符及来源loan，合流后继续读取，在scope结束后恢复root消费权限。
不从最后一次read猜结束，不开放条件来源或跨branch一般NLL。
具体白名单为直接Block内完整if、显式双ControlBody，各arm仅Owned Unit调用语句；
该scope每个直接borrow声明均须为同一精确origin的新descriptor、有source continuation、
唯一`AfterStatement(scope)`结束事实，且先于if。if整个子树的调用仅接Owned
Int/Boolean/Unit结果；Name、literal、Group、Prefix、普通Member及已批准Int Binary可用。
nullable断言、传播、索引、cast、插值、lambda及嵌套CFG不在首片；循环内新if也不开放。若所选if需要新ABI或一般
NLL，暂停请求独立决定。普通借用结果/借用已有metadata绑定的整函数门禁保持不变。

实施基线为actual main `4eb38d74f85b36dbac09f4c947f4c1ee762adbaa`，tree
`13e784258a2e179b358582f05324c692497cef94`；0299已由PR80合并，main CI
[38275520368](https://github.com/Halckon/Koven/actions/runs/38275520368)在激活时pending，
独立跟踪；本页首片本地验收不复用该CI结论。

沿用已发布事实、显式block参数和verifier；不靠语法形状或相同TypeId猜来源，不从AST补loan。
父子/兄弟/元素依赖仍保护根；一条分支结束不能释放另一存活依赖。正常结束先解除相应
结果/元素loan，再解除来源依赖，最后清理root owner；其他存活父/子/兄弟依赖继续保护根。
事实不足时维持带Span拒绝。

## 3. 非目标与仍开放的候选

不做一般CFG/NLL、条件/多来源合流、普通借用结果全面运输、任意when/return、escape、
consume、Clone、N1b、用户扩展或新算法；不以0289归档推定这些能力完成。

while/loop及更广循环组合继续作为后继候选；`break`/`continue`并非语言禁用，但不纳入
本片新增支持。后继须先封闭循环fixed-point/owner版本、目标loop身份、backedge上的根与
父子loan、continue保留provider/source、break结束最近provider及逐边清理，再独立验收。
不能把if正例或移除gate当作循环证明。

## 4. 冻结首片验收与交付

2026-10-12本地Rust验收输入为`52db978e05e25b7e5c4988125da2bedadf8b14c5`，tree
`8cf56a779c202ef9d3b70a47233793a1f5bc8dd4`；后续只同步本页与Architecture。
宿主为Linux x86_64，Rust 1.96.0、LLVM/Clang 21.1.8，Cargo均`--locked --offline`，
共用独占target串行执行。以下测试选择的实际命中均非零，无忽略项。

| 验收项 | 测试目标/选择 | 实际结果 |
|---|---|---|
| 两入口Int ==/<三位置、typed Int/Boolean与精确首拒绝 | `ssa::range_lowering_tests::local_control::`的18项比较 | 红测提交`fb6e8c67`的18/18均在精确Span拒绝；最小实现后同选择18/18通过SSA/verifier与未改写LLVM/native（每项运行真/假） |
| 同根双正常arm if逐边descriptor/root/source-loan与scope终止 | 同模块3项`if_both_arms` | 红测提交`fb6e8c67`的3/3均在完整if精确Span拒绝；最小实现后3/3通过，另9项同根父子兄弟通过双边/合流identity与cleanup断言 |
| 完整首片矩阵：String/MoveOnly/Resource、条件求值一次、双边选择与拒绝 | `cargo test -p lang-codegen --lib ssa::range_lowering_tests::local_control::` | 52/52通过：39项真实SSA/verifier与未改写LLVM/native正例（每例两个条件值）、13组精确Span拒绝，后者均核single/unit正逆provider顺序 |
| frontend词法scope、来源终止与普通borrow边界 | `cargo test -p lang-frontend --test ownership_borrow_last_use --test ownership_range_construction` | 8+14=22/22通过；新增scope内消费L0135、scope外恢复及普通borrow调用L0164证据，两入口均核验 |
| 0289五运算、同root调用前缀退出与普通borrow基线 | `cargo test -p lang-codegen --lib`分别选择`ssa::range_lowering_tests::scalar_counts::`、`ssa::range_lowering_tests::call_prefix::`、`ssa::borrow_result_boundary_tests::` | 5+6+4=15/15通过；普通源码与语言定义Abort，普通borrow/metadata alias整函数门禁另由首片负例覆盖 |
| 0299严格proof suffix与0300 Map owner carrier | 同codegen命令分别选择`owned_map_borrow`、`map_loop`；先列名确认选择 | 23+19=42/42通过；普通源码SSA/native、frontend/结构化拒绝和定义Abort，无注入 |
| 严格clippy | `cargo clippy -p lang-codegen --all-targets -- -D warnings`；`cargo clippy -p lang-frontend --test ownership_range_construction -- -D warnings` | 两项通过；只编译检查，未执行未选测试 |
| fmt、docs、尺寸与diff | `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base 4eb38d74f85b36dbac09f4c947f4c1ee762adbaa`、`git diff --check` | 均通过；653篇Markdown、1054个手写Rust文件，38项既有超限债务未增加或调整例外 |
| docs脚本合同 | `python3 -m unittest scripts.tests.test_check_docs scripts.tests.test_gen_spec_dag` | 42/42通过 |
| 独立review | 已提交生产实现及完整测试/文档diff，只读审阅 | 无未决阻塞；隐式CFG与Owned结果模式边界修正后通过。review未运行Cargo/native，不代替上述实跑 |

不运行故障注入、校准、坏IR、allocator/sanitizer/reducer或聚合drivers；native仅运行
普通源码产生的真实SSA与未改写LLVM。本地只验证当前Linux，macOS及PR/发布另行闭环。
未运行frontend/workspace全量或CLI教程：本片没有frontend生产、公共API、CFG、LLVM或ABI
变更，采用以上直接消费者验收，不将定向结果称为全量。可观察正常析构不等同于完整内存安全证明。
每个小提交的payload、raw commit、源码归档、bundle与原始日志保留仓外；scratch和tmp副本
仅提供当前工作期恢复，不承诺永久可取，仓内不复制raw JSON或日志。

## 5. 原广义目标：后继仍开放

以下保留原draft广义验收，不计为本冻结首片完成条件；本次即便首片验收通过也不声称整体完成。
Boolean短路、String拼接及复杂RHS、循环等需独立冻结与证明。

- [ ] 两入口普通源码分别覆盖上述四类表达式在binding前/期间/后的结果；静态选择必须有实际命中。
- [ ] if两支与短路两值有真实SSA/verifier、未改写LLVM及正常native结果，求值各一次、未选后缀不执行。
- [ ] String/MoveOnly/Resource范围保留唯一根、上述结果/元素loan→来源依赖→root清理顺序、父子/兄弟依赖和结束后权限恢复；非法move/变异/逃逸仍拒绝。
- [ ] 0289计数、同root前缀退出及普通borrow/metadata别名回归不退化；未选CFG继续结构化拒绝。
- [ ] 独立分支先红测、最小事实消费、直接消费者回归、Architecture与本页验收；发布按单独交付闭环。

激活时的静态边界复核不作为功能验收；本片实跑记录仅采用§4输入与结果，不沿用0303
恢复记录、历史649/42或0289的通过数字。禁止的注入/聚合不作为本次验收。

## 6. PR81 首轮失败与公开CLI回归补齐

本节追加实际远端失败及修复，不改写§4的本地检查点，不表示CI已绿或可归档。
[PR81](https://github.com/Halckon/Koven/pull/81)首轮实现head为
`acb306c357f526a071a771726b2adfcd80986e2c`，tree与本地冻结首片一致，为
`657df5384964d027b54846895c552fab826c9e0f`；base仍为`4eb38d74`。
[run38321740009](https://github.com/Halckon/Koven/actions/runs/38321740009) attempt1
以synthetic `0944f68904e61d338247ac7dce0625e840cbdf4e`运行，父为上述base/实现head，
tree相同，`reuse=false`，最终completed/failure。

[Linux](https://github.com/Halckon/Koven/actions/runs/38321740009/job/115051846036)与
[macOS](https://github.com/Halckon/Koven/actions/runs/38321740009/job/115051845987)各自命中全部
52项codegen测试，但随后`range_project_cli`各31通过/1失败：旧
`scalar_counts::public_std_range_scalar_counts_keep_other_binary_capability_gates`
仍断言`n == 2`应拒绝，实际build成功，于`scalar_counts.rs:103`panic。后续`n < 3`
未执行；两者均是已冻结Int比较的陈旧拒绝预期。stage中的8项last-use与14项range-construction
尚未到达，不计为远端通过；后续缺少artifact上传失败是前序中断的结果，不作第二个产品根因。

此前本地未选择该公开CLI目标。修复从真实remote head独立起步，仅改CLI测试，生产能力不变：
将两比较改为独立接受测试，保留`val other=...`语法，在binding前/存活期间/显式scope结束后
分别执行真/假结果；另以Resource普通源码核对条件一次、双arm、合流读及scope外consume/deinit。
短路和String拼接继续拒绝，补全`!= <= > >=`及Boolean/Long比较拒绝；普通borrow/metadata
alias的整函数门禁不改。独立只读审阅通过，未发现其他同类陈旧负例。

修复测试输入`eaae05988de87cdab69476da41288a2f18d4d2fe`，tree
`e354aeea548aaa29dbc82af43f552fbe65c93e84`；同一Linux工具链、独占target串行执行：

| 选择 | 实际结果 |
|---|---|
| 原head精确选择旧失败测试（`--exact --nocapture`） | 0通过/1失败；原样复现`n == 2`的build成功与断言失败，后续`n < 3`未到达 |
| `cargo test --locked --offline -p lang-cli --test range_project_cli scalar_counts::` | 修后6/6通过，含3项新增正常native与原五运算/定义Abort/拒绝边界 |
| `cargo test --locked --offline -p lang-cli --test range_project_cli` | 35/35通过、无ignored/filtered；事先核对全部是正常源码build/native、定义Abort或源码拒绝，无注入 |
| `cargo clippy --locked --offline -p lang-cli --test range_project_cli -- -D warnings`；`cargo build --locked --offline -p lang-cli`；fmt | 均通过 |
| docs/尺寸/diff与文档合同 | 均通过：653篇Markdown、42项脚本合同；1054个手写Rust文件，38项旧超限债务未增，尺寸基线为真实remote head |

原CI失败收据与本地红绿日志保留仓外；修复后的远端CI尚待实际运行，禁止将本地35项通过
替换为双宿主CI通过。Spec继续active，广义后继不因修复关闭。


## 7. PR81 修复后实现验收与冻结首片有界归档

除顶部生命周期状态及迁移必需链接外，§1–6保留本地与首轮CI检查点原文；首轮
`38321740009`失败、CLI中断后frontend目标未到达及后续修复均不覆盖。用户已批准实施CI
通过后的有界归档，`done`仅关闭§2/§4的真实Int `==`/`<`与同根词法statement-if首片。
§5广义目标仍开放，当前候选入口见[v0.43后继](../../specs/drafts/v0.43/README.md#0298首片之外的开放后继)，
不因本次归档获得语义启用或实施授权。

| 项目 | 精确输入与实际结果 |
|---|---|
| PR / 修复后实现head | [PR81](https://github.com/Halckon/Koven/pull/81)，`0aaa544a950d0be979841d9a61fb534f83ab7f54`；直接parent为CLI修复`e45d60b3261e5b488cc3841a5b291bfada221fa6`，再前为首轮失败head `acb306c3` |
| tree / base | tree `f2d3deb8f4914febf5cd99e6f50f118156c1784f`；base `4eb38d74f85b36dbac09f4c947f4c1ee762adbaa` |
| CI输入 | [run38323477725](https://github.com/Halckon/Koven/actions/runs/38323477725)，attempt1，pull_request；synthetic `5df93629bc0a829e529ef931d04883086a115364`的父为base/实现head，tree相同；`reuse=false` |
| 实现CI终态与时间 | completed/success；最后job的completed_at为2026-10-12 23:57:21 UTC，run updated_at也为该时间；API无run completed_at，不将updated_at冒作完成字段 |
| 实际job范围 | 15个job成功；唯一Tree-sitter因`editors=false`合法skip；代码与preview实际执行 |
| Linux逐目标 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115057472440)：在bounded composition步骤内，52个local_control完整名各唯一ok；last-use完整8、range-construction完整14、range_project_cli完整35各自全通过，无ignored/filtered |
| macOS逐目标 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115057472513)：同52/8/14/35分别逐名唯一ok，不跨host汇总或沿用首轮结果，无本片ignored/filtered |
| 完整codegen lib目标 | Linux 1407通过/0失败/0ignored/0filtered；macOS 1406通过/0失败/0filtered，1项既有LLDB task-port权限ignored，与本片52项无关 |
| 其他门禁 | 变更检测、依赖、尺寸、Docs/Fmt、双宿主Check/严格Clippy通过；[Code CI Evidence](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115060842851)与[CI Passed](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115060842867)均成功 |
| preview链 | Linux候选artifact `11779629691`与[Linux consumer](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115060409018)、macOS候选`11780750270`与[macOS consumer](https://github.com/Halckon/Koven/actions/runs/38323477725/job/115060573591)实际指定下载ID一致，producer package和consumer install均通过 |

Code CI Evidence绑定上述head/base/synthetic，代码输入指纹为
`80ae1c61b1782ece086735d612085f4799bd26a4d62c07d151a217026d46db60`。
两条preview的上传收据、artifact API及consumer预期/实际下载ZIP digest相符；核验对象是
GitHub artifact ZIP，未另行下载并独立核包内manifest。原始API/日志和逐名匹配结果留仓外。
macOS的Linux-only检测步骤依平台条件合法skip，不构成macOS动态检测通过。
完整CI成功、普通native的值/清理断言与通用检测各守证据边界，均不宣称完整内存安全证明。

本归档只迁移合同、同步索引/库存/DAG、机械修正链接并追加真实验收；不改Guide、生产或测试。
最终归档head CI、正常merge和actual-main CI仍待分别核验，本节不提前认领其通过。
