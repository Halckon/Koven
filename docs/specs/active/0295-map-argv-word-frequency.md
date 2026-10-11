# SPEC-0295: 两遍 argv Map 词频应用

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施或验收 M3B K-H 应用替换时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-295` |
| 所属 Phase | Phase 2/3 既有事实交接、Phase 4 native、Phase 6 应用验收 |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[Map 与 for](../../guide/12-collections-destructuring.md)、[nullable](../../guide/09-nullability-errors.md)、[String](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户要求继续里程碑并选择现行 Guide 允许的 M3B K-H 真实 argv 词频 Map 改写 |
| 前置 Spec | SPEC-0274、SPEC-0288、SPEC-0294 |
| 前置 ADR | ADR-0017、ADR-0018、ADR-0019、ADR-0027、ADR-0030 |
| 影响范围 | 教程唯一三文件源码、普通源码/native 验收、实际触发的阶段交接 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 main `4321e99e401a3d8b9099f087c577840822a64512` 的独立分支 `feature/spec-0295`
交付两遍 argv 的 MutableMap 词频实现，保持已经验收的输入、输出、首次次序及错误合同。
这只完成 [M3B 起草材料](../../development/map-collections-spec-draft.md) 的 K-H 应用替换，
不把历史草稿视作现行语言规则，也不重新声称全部 K-A–K-G 或后继借用能力完成。

## 2. 应用与所有权合同

- [教程](../../tutorials/koven-tour.md#argv-word-frequency) fence 继续作为三个 `.ko` 文件的唯一源码。
  每个 argv 是完整词，UTF-8 字节相等即同词；不分词、大小写折叠、normalization 或排序。
- 每个不同词按首次出现次序输出一次：原始词、TAB、无前导零十进制计数、LF；空 argv 无输出。
  空词和词内 TAB/LF/CR/空格/引号/反斜杠/非 ASCII 保持原字节。正常 exit 0、stderr 空。
- 非法 UTF-8 在 entry 前拒绝；NUL 不能进入主机 argv。分配/输出错误和负 decimal 沿用既有
  Abort，不承诺 stderr 或沿栈展开；decimal 的 0 到 Int.MAX_VALUE 边界及最多十层递归不变。
- 第一遍借用 argv 的 `for` binding：`get(word)` 复制 `Int?`，缺失时 1，存在时加 1，
  `put(word.clone(), count)` 把独立 owned String key 交给 Map。clone 是现行 String intrinsic。
- 第二遍仍按 argv 顺序借用 word；`remove(word)` 移出 `Int?`，仅 present 输出。首次 remove
  后相同词缺失，因此无须 Map 迭代或排序。Map 与 argv 为不同 root，无 entry loan 跨写入。
- 计数不超过 argv.size，保持非负 Int 域；两遍各线性遍历，合计 O(n) 次 Map 操作。
  字符串复制、字节哈希/相等和碰撞成本另计，不宣称总时间 O(n) 或性能倍数。

## 3. 阶段交接与边界

先对真实三文件程序运行失败测试，区分 loop owner carry、for Borrow binding 的显式 clone、
Copyable nullable 分支的已有支持。只补实际缺失的通用事实消费，不在 SSA/LLVM 按 String/Int
或应用名称特化，不重新推导 frontend 语义，不放松 verifier。若存在 Guide 真空则停止请用户决定。
已有 Map<String,String> 与 MoveOnly value 回归必须保留，应用选择 Int 不缩减通用 Map 合同。

## 4. 非目标

Map 迭代 API、consume、通用 Clone、Option、新 String/IO API、语义或 Guide 修改、性能比较。
云执行仅正常源码和静态门禁；生成器、缩减、注入、校准、LLVM 改写、资源 counter 和 aggregate
仅交给 CI，不能把未运行记成通过。原错误合同的直接公开命令验收不改写源码或 LLVM。

## 5. 验收账本

| ID | 验收目标与命令 | 实际结果 |
|---|---|---|
| H1 | 唯一三文件 Map 程序，现有 `scripts/check_word_frequency.py` 的 8 组原 byte oracle、decimal/非法 UTF-8，共 37 public commands | 待运行 |
| H2 | 空表、交错重复、更多不同词的增长及首次顺序；新增普通源码/静态合同证明两遍 Map | 待运行 |
| H3 | 实际缺口对应 frontend/SSA/native 红转绿及直接消费者；保留通用 String/MoveOnly Map | 待定位 |
| H4 | `cargo fmt --all -- --check`、受影响 crate 严格 clippy、docs、diff、Rust 尺寸静态检查 | 待运行 |
| H5 | 原教程、preview 消费者和双宿主完整必需 CI；独立审阅、证据、归档和精确 head 交付 | 待 CI；本机结果不替代 |

## 6. 实施与提交

1. 合同、原 oracle 的真实程序红测；若已有能力直接通过，不为制造失败修改编译器。
2. 只对真实缺口补最小通用实现及定向回归，再验收应用和直接消费者。
3. 同步 Architecture 与验收事实；根线程负责及时推送和 PR。最终双宿主 CI 验收后才归档。

每个逻辑切片独立提交并包含 SPEC-0295；共享 Cargo target 串行使用，不执行 cargo clean。

## 7. 当前记录

2026-10-11 已确认基线及独立分支；尚未运行验收，无语言规则未决项。
