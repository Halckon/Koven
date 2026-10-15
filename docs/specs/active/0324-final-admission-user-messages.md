# SPEC-0324: 最终类型准入的用户诊断措辞

> **性质**：有界呈现修复合同 · **状态**：in-progress · **读取时机**：审查L0165消息与分类保持时 · **唯一真源**：本Spec；不改变现行Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0324` |
| 所属 Phase | Phase 2诊断呈现；不改变最终消费资格 |
| 语言规范 | [现行Guide](../../guide/README.md)，语义不变 |
| 批准依据 | 用户2026-10-15要求解决增量review；本片仅措辞，先等价提取与普通数据exact测试，审定后执行 |
| 基线 | actual main `3104baa47845a9ec738c06aa60cf61f5d1a3772e`；独立`fix/spec-0324`，不依赖未合0323 |
| 前置 Spec | [0319最终类型准入](../../archive/specs/0319-final-type-admission-diagnostics.md) |
| 语言语义变更 | 否 |
| 阻塞项 | 同head本地5exact与4工程已通过；发布、双宿主CI及有限归档待完成 |

## 1. Goal与非目标

L0165当前直接显示DeferredReason的Debug枚举名。将18项真实分类映射为稳定用户措辞，
保持code、severity、源Span、顺序、原诊断保留与失败类别。Any在bound角色的既有例外不变。
不推断用户一定写错，不称所有情况unsupported/not implemented，不给未经事实支持的修法。
不修改raw类型事实、成功资格、ownership次序、CLI协议或公开API；不依赖0323静态恢复。

p42普通赋值类型事实、p25循环流事实须分别确认语言合同权威，登记为后继，不在本片改Guide或接纳。

## 2. 最小模块与验收顺序

1. 私有`final_admission/message.rs`提取原Debug格式，walk只委托构造消息；独立小提交。
   一个纯函数exact枚举18个正常DeferredReason值，锁原文本逐字不变；不构造或修改compiler facts。
2. 文案审定后独立tests-only锁18项候选用户文本，采真实断言红；不以缺符号/编译失败代替红。
3. 生产以无通配fallback的穷尽match返回静态后缀，再组成消息；新增reason必须编译失败要求补文案。
4. 同head精确回归：纯映射、普通源码single/unit根诊断、CLI human/json呈现；固定输入及argv，
   独审/授权后串行执行。CLI只build，不运行产物；不为覆盖18原因制造IR或改facts。
5. 适用工程检查与远端CI完成后有限归档；未运行项不写通过。

初始旧文案exact：
`cargo test --locked --offline -p lang-frontend --lib type_checking::final_admission::message::tests::legacy_messages_preserve_all_deferred_reasons -- --exact --nocapture --test-threads=1`。
仅执行这一已审定函数，不宽跑lib或测试target。仓外绝对target与原始收据保留。

## 3. 候选文案原则

统一描述类型检查尚不能完成的具体对象。18项分别覆盖Any值表示、名义/类型参数、限定类型、
函数组成类型、外部符号类型、前向值依赖、重载选择、this类型、成员、调用、索引、赋值、
转换/类型测试、错误传播、when、控制流结果、解构分量和迭代来源。最终精确文本由受审测试表
作为单一实现验收真源；此处不复制整表，避免多个合同漂移。

## 4. 状态

- [x] 基线/独立分支与编号只读核查；官方分支未见0324，本基线无同号文件。
- [x] 私有提取等价exact通过。
- [x] 候选18消息独审与真实红证。
- [x] 最小映射实现、普通源码/CLI/工程精确验证。
- [ ] 双宿主CI、有限归档和正常合并。

## 5. 等价提取实际记录

`eab0a88f47714acb98e4171f220a977002623b59` / tree `a8e6b227bbb75e085c5f5befc782a5395f93f5bf`，
唯一legacy exact实际1命中通过，覆盖全部18旧文本；7登记输入SHA前后同且clean，原始收据仓外保留。
该结果仅证明等价提取，未实现或验证新用户文本。随后tests-only替换18项新oracle，保留原提交与收据；
ForwardValueType使用“the value's type is not yet determined”，避免暗示重排声明能够解决循环依赖。

## 6. 真实红证与实施准入

77811968147a3afea990644f8fd0472bd429f08e / tree b67feeb3f3c8f5a754d63bc8b6691725bbcd44fb，
唯一新oracle exact编译成功、实际1命中、exit101断言红；完整18旧文本与18新文本差集已记录，
7输入SHA前后同且clean。该失败为预期行为红，不修改旧receipt。随后独立4d5603c5仅实施穷尽映射，
无通配fallback，不改变walk分类、Any-bound例外或输入事实。

下一固定名单仅5exact：18消息映射、p34普通源码single/unit消息与Span、新CLI single/project
build human/json各一exact、既有0319 frozen_roots精确控制。CLI只启动编译器build进程，
不执行产物；保持L0165、Error、源40..45和空details，只替换message。工程检查待同head执行。

## 7. 同head本地验收

实际运行`0b15e5462456a94203d3102c9d0349907e72c063` / tree `d7e0823a2e53819707d24e31b1c19f0ad7709444`。
五个唯一exact逐项实际1命中全绿；manifest SHA256
`206a99fc58d4bd0a95e6b3314807b621500e5067ede76c0a0e52add1b261eabc`。
18登记输入SHA前后同、clean，完整argv/raw/hash收据仓外保留；后继docs-only提交不冒称实际测试head。

- 私有纯函数18分类逐项匹配受审文本，生产穷尽match无通配分支或Debug fallback。
- p34普通源码single/unit保持唯一L0165、Error、源40..45、空details；原0319四fixture控制
  仍保持各自code/Span及真实最终分析拒绝，不把未完成类型放行。
- 两个CLI exact共4次build进程（single/project各human/json）；single exit2、project exit1，
  stdout空，新message精确一致、code/Span/协议保持、无Internal、产物或目录字节改写。未运行产物。
- 同head `cargo fmt --all -- --check`、`cargo check --workspace --all-targets --locked --offline`、
  `cargo clippy -p lang-frontend -p lang-cli --all-targets --locked --offline -- -D warnings`、
  `cargo build -p lang-cli --locked --offline`四工程均exit0，不计作额外行为测试。

原legacy等价绿与新文案断言红完整保留。下游helper曾在静态独审发现JSON整对象比较含额外message
键的问题，执行前由0b15e546改为code/primary两侧投影、message/details分别精确断言；未运行该
已知错误版本，不编造失败或通过证据。

CI接线静核：`crates/**`触发Rust changes；`check_integration.sh`经`check_core.sh`选择frontend lib
与lang-cli，`check_stage_integration.sh`选择frontend final_type_admission，覆盖本片新测试。
尚未发布或执行本片远端CI；本地未运行这些聚合脚本。Spec保持active，p42/p25合同与0323修复不在
本片验收范围；这不是整份增量review或v1完成。

## 8. PR108主线后的真实双父整合与重验

PR108正常合入actual main `1409ff851646e1a9012bed0b11d6366990e0410d`后，从真实PR109 head
`a8555ff04ae80426ad3980569e9e36ded33f92b5`正常merge，提交`6430e3bda97e4ed147f7feec61f7c6b0ccfded48`
按上述PR109、main顺序保留两个真实父节点，不重写或合并同内容的local/remote映射历史。
生产walk保留0323静态恢复与0324消息委托；CLI helper合并两边精确断言，0323 archived、0324 active。
初次docs检查发现tooling入口201行超过200上限；0bb5fcbd仅精简重复措辞后677页结构检查通过。

固定实跑`0bb5fcbd78d0ebdeac8e418af61d307547c49ea8` / tree `fa5c5e9eacab521d4b16378657595be069468de9`：
原0323的15与0324的5构成20个唯一exact，逐项实际1命中全绿，全部argv保持原审定名单。
manifest SHA256为`8f9a417cb9519ee568aa933c2cc2f430a6a96f2e49b122eaf840338f58e2f88f`。
同head重跑§7四工程均exit0；24登记输入SHA前后同、clean，原始收据仓外保留。
五个CLI exact合计12次build进程，仅核诊断，不运行产物；两修复的已有oracle均保持。

本节是整合后的新本地证据；原实现head的CI不冒称该整合head通过。后继仅docs账本提交，
仍须发布整合版并核其远端CI通过，之后再另作0324有限归档；不提前改为done，不改Guide或扩大语义。
