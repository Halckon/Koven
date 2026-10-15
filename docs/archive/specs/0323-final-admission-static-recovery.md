# SPEC-0323: 最终类型准入的静态非值角色与失败调用恢复

> **性质**：有界诊断修复 · **状态**：done · **读取时机**：审查最终准入静态角色与恢复根因时 · **唯一真源**：本合同；不改变现行Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-0323` |
| 所属 Phase | Phase 2最终类型准入；保持先于Phase 3 ownership |
| 语言规范 | [名称身份](../../guide/02-names-files-packages.md)、[类型规则](../../guide/03-types-generics.md) |
| 批准依据 | 用户2026-10-15要求检查并解决增量review问题；首轮tests-only已独审/获准执行并采红；据此准入最小生产patch准备，固定diff独审后才重跑 |
| 基线 | actual main `3104baa47845a9ec738c06aa60cf61f5d1a3772e` / tree `64ebc102c8237f74f87db8311b14949b9c7b0105`；独立`fix/spec-0323`，不混std规划 |
| 前置 Spec | [0319最终类型准入](0319-final-type-admission-diagnostics.md) |
| 编号 | 本基线最大0322；官方分支只读核查未见0323/0324；官方开放PR检索仅见103/64，未占0323；本片登记0323 |
| 语言语义变更 | 否 |
| 阻塞项 | 实现head双宿主CI与有限验收已通过；归档head CI及正常合并/main交付待核 |

## 1. Goal与报告边界

review报告原p28 companion single、原p16 Result传播single/project共3个入口出现
`final type admission is missing original diagnostic for required Error type`。
上述为外部增量review的报告表述；本分支重现区分single缺原诊断与unit缺expression type，见§5。附件原probe字节保持在真实fixture，
另收review §3.1两份最小源码；不执行附件matrix.sh、99×2矩阵或任何产物。

最终准入只豁免由真实名称/已选事实证明的非值静态限定符；失败调用的恢复不能把这类子壳
错当必需runtime值，亦不能丢失原诊断或已积累的真实Deferred根因。真实value Error必须
继续fail closed，动态receiver/实参照常递归检查；不得按类型名拼写跳树。

## 2. 静态候选与非目标

`final_admission/walk.rs`的Member对未选static qualifier进入普通value检查；Call的失败callee
恢复标记没有覆盖该限定符子壳，可能触发缺原diagnostic Internal。首轮以静态根因候选准入，
后续真实raw/complete与修复结果见§5–9；不以统一Error→L0165掩盖内部事实缺失。

- 不开放companion/Result传播后端能力，不改verifier、CFG、所有权顺序或基础类型准入规则。
- 真实必需call仍Deferred时必须源级拒绝，不得因限定符豁免获得最终完成资格。
- p25/p42的L0133/L0135遮蔽另属推断补齐；不倒置final-before-ownership。
- L0165面向用户措辞另立后继；本片不混18种reason文案变更、Guide或std规划。

## 3. 首轮tests-only与安全范围

首轮新增模块`crates/lang-frontend/tests/final_type_admission/static_recovery.rs`，只通过现有
lex/parse/name/raw type/complete API构造真实分析，不改facts/IR，不进ownership、LLVM、链接或运行。
原p16/p28及两份最小fixture在`tests/fixtures/final-admission-static-recovery-0323/`。
每个exact先采single和unit-basic两份结果，再断言；本轮没有constant qualification或CLI覆盖，
不能将加常量前缀当作constant pipeline。之后若需constant需明确API与独审名单。

| exact前缀`static_recovery::` | 预期与首红要求 |
|---|---|
| `original_companion_preserves_source_rejection_and_unit_control` | single仍有raw Deferred且应Diagnostics；unit完整已选对照应Ok，实际采集若不符先报告 |
| `minimal_companion_preserves_source_rejection_and_unit_control` | 同上，最小源码区别于原probe |
| `original_result_propagation_preserves_source_rejections` | 两入口都保留raw Deferred，应源Diagnostics而非Internal/Ok |
| `minimal_result_propagation_preserves_source_rejections` | 同上，最小源码 |
| `dynamic_receiver_real_value_error_is_not_hidden` | 普通非法Boolean算术生成真实原诊断，动态receiver恢复不得丢失 |
| `static_constructor_real_argument_error_is_not_hidden` | P(Int)收到Boolean的真实参数诊断不得隐藏 |

首轮oracle核原诊断code/Span/message原样保留；L0165主Span必须精确匹配该次raw分析的
真实Deferred表达式，不能用任意同名token。记录完整结果后再将选定根表达式/顺序收紧为
固定code/Span oracle；当前不编造未运行的诊断集合。无raw错误的companion unit正控必须Ok，
其余冻结预期为Diagnostics，不使用宽泛Ok或Diagnostics任选的通过条件。

拟执行仅六条，每条完整argv为：
`cargo test --locked --offline -p lang-frontend --test final_type_admission static_recovery::EXACT -- --exact --nocapture --test-threads=1`。
使用恢复工具链与仓外绝对target、串行；固定head/fixture/helper SHA及runner独审后再请求root放行。
编译/依赖错误或非1实际命中立即停；行为断言红保留真实日志，不扩大filter或伪造facts。

## 4. 验收与停止条件

- [x] 独立main分支与普通源码准备、静态根因及非目标登记。
- [x] 官方开放PR与分支编号只读复核，未发现0323占用。
- [x] 首轮tests/helper/argv独审及六exact运行授权。
- [x] 固定tests-only真实红与既有绿分账，收紧确定根因code/Span。
- [x] 最小生产独立提交与审查，原精确目标和受影响普通控制回归。
- [x] 按适用入口补足CLI/含const前置诊断证据，工程门禁、双宿主CI与有限归档。

若需要放宽全部静态类型名、抹除value Error、改变语言支持或 ownership 顺序，停止并请求独立决定。
本草案不是整份review闭合，也不把未运行项、后端旧拒绝或外部报告当作本分支通过。

## 5. 首轮真实红账本

2026-10-15固定`e6c1280d1100d466cb8c060db16e0ba2249dc5f5` / tree
`8797e28c185fe79cace9be483e8b834278614866`，上述六exact各实际1命中，4断言红、2控制绿。
编译成功，11个登记输入SHA前后相同、工作树干净；完整argv/stdout/stderr/hash收据仓外保留。

- 原/最小p28 single在`P` SourceId0 95..96报缺original diagnostic Internal；unit-basic均Ok。
  raw Member `P.origin`95..103及call95..105仍Deferred，修复应保留前者L0165独立根，不完成调用。
- 原p16两入口在`Result`203..209内部失败；最小为173..179。single缺original diagnostic，
  unit缺expression type，不能把两种Internal原因混为一谈。真实传播根`parse(n)?`原185..194、
  最小155..164，两入口均Deferred；应保留该L0165根，不新增限定符错误或v级联。
- 动态receiver普通错误两入口L0085，`+`34..35；静态P(true)参数两入口L0084，`true`55..59。
  源诊断保留，后续oracle加强完整Diagnostic相等，包括labels/details，不只比较主消息。

本轮未执行CLI/constant/native。收紧tests-only不修改首轮receipt或将未运行新head称为已验。

## 6. 红证后最小实现准入

首轮红证及984c9b33的精确根oracle已交独审；仅在Member无selection、真实名称事实证明
receiver为静态限定符，且该Member本身Deferred或已有失败callee recovery上下文时恢复该非值壳。
Member自身最终类型仍检查；不处理任意Name/Error，不跳动态receiver/实参，不取得未完成调用资格。
新增两个普通源码控制仅锁非call静态`P.unknown`根与局部值P遮蔽类型P的原错误保留；
exact为`unresolved_static_member_without_call_keeps_its_own_root`、
`local_value_shadowing_type_keeps_real_operand_diagnostic`，待固定名单独审/批准运行。

## 7. 八目标精确红轮与控制前置修正

固定`52fd8e8955a019ab79c464d146aff088895c742e` / tree `19b6471c357feac701f934a205115691ad4da394`
八exact各实际1命中；原四目标Internal红、原两控制及同名局部P遮蔽控制绿。第七无调用
`P.unknown`的unit在名称阶段已有L0080、unknown49..56，未到complete；helper在validate
处失败，不能计为本生产缺陷红。其raw完整保留，输入SHA前后一致。
按真实阶段仅修该tests-only控制：single独立采真实complete，unit锁已有name诊断并止于names。
不将unit强行送入complete、不降低其他oracle；修订后八目标须同head另经审定执行。

## 8. 非调用控制的真实恢复根因

411b75da的八目标重跑保留原四Internal红与三控制绿，但第七single并非预设Deferred，
在raw断言前置失败；该失败同样不算已验证生产缺陷。随后独审/批准的d9723229唯一exact
仅作普通前端观察，命令exit0/1命中不计回归绿：single `P.unknown`为Error，原L0080
`unresolved associated constant`在unknown49..56，complete却在P47..48报缺原诊断Internal。
unit的L0080 `unresolved qualified name`仍在name阶段，validate拒绝，不能进入complete。

现将该控制锁为single完整原Diagnostic保留、unit原name拒绝；同根恢复只额外纳入本Member
name上的精确L0080原诊断与Error形状，仍须真实静态限定符身份，不以全局任意错误作豁免。
两次前置假设失败及独立观察收据保持原样，不改写为成功；其余精确根oracle不变。

## 9. 最小修复与同head本地验收

生产提交`b0f60399cce3d9f2c105ab0e361ce16ad252a977`仅改final-admission Member恢复：
未选Member、真实静态限定符身份，以及失败recovery、Member Deferred或本name精确L0080的Error
共同界定恢复；Member最终类型、动态receiver和实参仍检查。独审后原八exact及四个既有普通控制全绿。

固定`521f38746833d85845955eb0e19acd78e45416b7` / tree `8acd8f0414e8459b32fa37713d2c3f5fb8f16da8`
统一重跑15个唯一exact，每个实际1命中通过：八个本片frontend、四个既有final-admission控制、
三个公开CLI build-only诊断测试。使用`--locked --offline`、串行及仓外绝对target；21个登记输入
SHA前后同、工作树干净。manifest SHA256为`5f5c2b8e37b8d7d6aa827672df59af07161f7553222da7149b9317cb8a173c09`。
原始stdout/stderr、完整argv和前后输入身份仓外保留，不把后续docs-only提交冒称实跑head。

- single p28与p16各human/json、project p16基础/含const各human/json，共8次编译器build进程；
  分别exit2/exit1，stdout空，唯一L0165主Span精确，无Internal、产物或目录字节改写，未执行产物。
- 含const项目的真实Span为232..241，原185..194加47字节；最终准入先于constant fallback，
  此证据只覆盖含常量源码的前置源诊断，不能宣称执行constant ownership。p28 unit合法typing
  保持正控；其后端能力不在本片接纳范围。
- 同head `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked --offline`、
  `cargo clippy -p lang-frontend -p lang-cli --all-targets --locked --offline -- -D warnings`、
  `cargo build -p lang-cli --locked --offline`均exit0。工程门禁不计为额外行为测试。
- 旧b0工程fmt/check通过、clippy因新测试两处bool.then lint失败，build未执行；原失败保留。
  ed7a9c67仅将这两处改为等价if/Some/None后才取得上述同head工程结果。

CI静态接线已核：`crates/**`命中Rust changes，双宿主Targeted Tests的`check_integration.sh`
经`check_stage_integration.sh`选择frontend final_type_admission，且`check_core.sh`选择lang-cli
全部集成targets，覆盖新八frontend与三CLI。尚未触发本分支远端CI，不把接线当执行通过；
本地未运行这些聚合脚本。仍待远端CI、有限归档及正常合并；p25/p42推断、L0165措辞和整份review
其余能力不由本片宣称闭合。

## 10. 实现head CI与有限归档

用户已批准本片归档及正常合并。PR108实现head
`f71a23a92837622531b336e40213c3f6e2887d80` / tree `b61b5552b00145ea185546bb9b7a5c8b257a3cec`
通过[CI 38714525342/a1](https://github.com/Halckon/Koven/actions/runs/38714525342/attempts/1)：
15个job实际success、Tree-sitter范围skip，reuse=false；Linux/macOS均逐项核验本地清单15个
exact实际各1命中通过。synthetic merge为`8c8c7a20c4d148a4f6a642dff2a1dc7bb971a7b2`，
基线main3104。独立summary SHA256为`df18748d3118e7d24788448f4821e9412ac494cbbb27d2a610fa581bb554a40e`；
原始日志仓外保留，不计skip为实际执行，也不把此实现head证据冒称归档head已实跑。

本次仅关闭静态非值角色恢复导致的Internal及原诊断丢失；companion/Result传播的原生能力、
p25/p42推断合同和整份review/v1均未因此完成。保留§5–9真实红、控制helper假设修正、独立观察
与clippy失败记录。归档提交直接继承实际PR head；归档head CI、正常合并及main交付仍须另核。
