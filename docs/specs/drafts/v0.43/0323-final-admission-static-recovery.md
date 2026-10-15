# SPEC-0323: 最终类型准入的静态非值角色与失败调用恢复

> **性质**：有界诊断修复草案 · **状态**：draft · **读取时机**：审查最终准入静态角色与恢复根因时 · **唯一真源**：本合同；不改变现行Guide

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0323` |
| 所属 Phase | Phase 2最终类型准入；保持先于Phase 3 ownership |
| 语言规范 | [名称身份](../../../guide/02-names-files-packages.md)、[类型规则](../../../guide/03-types-generics.md) |
| 批准依据 | 用户2026-10-15要求检查并解决增量review问题；当前仅草案与tests-only准备，运行与生产待固定独审/批准 |
| 基线 | actual main `3104baa47845a9ec738c06aa60cf61f5d1a3772e` / tree `64ebc102c8237f74f87db8311b14949b9c7b0105`；独立`fix/spec-0323`，不混std规划 |
| 前置 Spec | [0319最终类型准入](../../../archive/specs/0319-final-type-admission-diagnostics.md) |
| 编号 | 本基线最大0322；官方分支只读核查未见0323/0324；官方开放PR检索仅见103/64，未占0323；本片登记0323草案 |
| 语言语义变更 | 否 |
| 阻塞项 | 新tests-only尚未编译/运行；原diagnostic与raw Deferred/source身份须真实采集；最小生产需独审 |

## 1. Goal与报告边界

review报告原p28 companion single、原p16 Result传播single/project共3个入口出现
`final type admission is missing original diagnostic for required Error type`。
这是外部增量review的实测报告，不冒称本分支已经重现。附件原probe字节保持在真实fixture，
另收review §3.1两份最小源码；不执行附件matrix.sh、99×2矩阵或任何产物。

最终准入只豁免由真实名称/已选事实证明的非值静态限定符；失败调用的恢复不能把这类子壳
错当必需runtime值，亦不能丢失原诊断或已积累的真实Deferred根因。真实value Error必须
继续fail closed，动态receiver/实参照常递归检查；不得按类型名拼写跳树。

## 2. 静态候选与非目标

`final_admission/walk.rs`的Member对未选static qualifier进入普通value检查；Call的失败callee
恢复标记没有覆盖该限定符子壳，可能触发缺原diagnostic Internal。这里是静态根因候选，
待真实raw/complete结果确认后才能冻结修复，不以统一Error→L0165掩盖内部事实缺失。

- 不开放companion/Result传播后端能力，不改verifier、CFG、所有权顺序或基础类型准入规则。
- 真实必需call仍Deferred时必须源级拒绝，不得因限定符豁免获得最终完成资格。
- p25/p42的L0133/L0135遮蔽另属推断补齐；不倒置final-before-ownership。
- L0165面向用户措辞另立后继；本片不混18种reason文案变更、Guide或std规划。

## 3. 首轮tests-only与安全范围

新增模块`crates/lang-frontend/tests/final_type_admission/static_recovery.rs`，只通过现有
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
- [ ] tests/helper/argv独审及运行授权。
- [ ] 固定tests-only真实红与既有绿分账，收紧确定根因code/Span。
- [ ] 最小生产独立提交与审查，原精确目标和受影响普通控制回归。
- [ ] 按适用入口补足CLI/constant证据，工程门禁、双宿主CI与有限归档。

若需要放宽全部静态类型名、抹除value Error、改变语言支持或 ownership 顺序，停止并请求独立决定。
本草案不是整份review闭合，也不把未运行项、后端旧拒绝或外部报告当作本分支通过。
