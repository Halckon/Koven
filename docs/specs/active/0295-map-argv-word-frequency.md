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
| 前置 ADR | ADR-0017、ADR-0018、ADR-0019、ADR-0023、ADR-0027 |
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
| H1 | 唯一三文件 Map 程序，原 8 组 byte oracle、decimal/非法 UTF-8 的 37 命令 | Linux 全通过；下述 40 命令包含全部原合同 |
| H2 | 空表、交错重复、32 distinct 增长及首次顺序；普通源码/静态合同证明两遍 Map | Linux 新旧共 40 public commands 通过；3 个纯静态 Python 合同通过 |
| H3 | Copyable nullable→Borrow 红转绿与直接消费者；保留通用 String/MoveOnly Map | 4 个新 native、20 个 frontend、13 个相关 SSA/native 通过，0 ignored |
| H4 | fmt、codegen/frontend 严格 clippy、docs、diff、Rust 尺寸静态检查 | 本机通过；docs 611 页，尺寸欠账按既有护栏报告，无新增例外 |
| H5 | 原教程、preview 消费者和双宿主完整必需 CI；独立审阅、证据、归档和精确 head 交付 | Linux 教程 32 执行/1 planned；独立实现审阅通过；preview/双宿主/归档仍待 CI |

## 6. 实施与提交

1. 合同、原 oracle 的真实程序红测；若已有能力直接通过，不为制造失败修改编译器。
2. 只对真实缺口补最小通用实现及定向回归，再验收应用和直接消费者。
3. 同步 Architecture 与验收事实；根线程负责及时推送和 PR。最终双宿主 CI 验收后才归档。

每个逻辑切片独立提交并包含 SPEC-0295；共享 Cargo target 串行使用，不执行 cargo clean。

## 7. 当前记录

2026-10-11 首次红测：基线 CLI `cargo build --locked -p lang-cli` 成功；Rust 1.96.0、
LLVM 21.1.8。教程两遍实现通过类型/所有权后，在第一个空 argv 项目的 build 阶段因
`native object InvalidModel: frontend lowering failed with InvalidSsa` 失败，未执行 artifact。
原 8 组 oracle 无修改。另追加固定 32 不同词、非排序次序及尾部重复的第 9 组，独立手写
期待字节；原 37 命令仍可按原项目/argv 识别，新增 3 命令后总数 40。正在定位实际 SSA 缺口。

已定位并补齐的真实缺口是：Copyable Map result 在非空 edge 传默认 Borrow 参数时，旧 lowering
把 presence/payload aggregate 错当 inner 的 RootPlace。两入口只消费 typed non-null descriptor，
核对 root、声明/窄化类型、注册结果和 Copyable 能力，再复制 inner 建立同步 loan；不改 verifier。
普通 Int/Boolean/value-class 用例由 InvalidSsa 红转绿；最终矩阵与 public CLI 仍待记录。

额外探索的 MoveOnly String remove + for-body null 比较在未改生产的 main CLI 同样失败：
结果先 drop 后比较，verifier 报 ValueUnavailable。完整正常源码、基线 compiler/source 哈希和
公开 build 原始失败见[独立边界记录](../../development/evidence/map-word-frequency-0295/known-boundary.json)。
它不是本次 Copyable Borrow 修复造成，且本片不扩成一般 MoveOnly CFG 清理工程；没有计为通过。
现有通用 Map 合同继续保留，新增 String/Resource owner carry、覆盖及 loop 外移出另行验收。

## 8. 本机验收 checkpoint（2026-10-11）

[本机汇总与红绿原账本哈希](../../development/evidence/map-word-frequency-0295/local-summary.json)
链接实际运行身份，完整 [red](../../development/evidence/map-word-frequency-0295/local-red.json) / [green](../../development/evidence/map-word-frequency-0295/local-green.json) 保留 argv/cwd、compiler/source SHA256、exit 和 stdout/stderr 原始字节。
绿色账本 10 项目、40 条 public commands，新增正常增长项目占 3 条；原 37 条仍按原 argv 与项目辨识。
两遍实现只新增 existing String.clone；没有修改 metadata 中原 8 组预期。

| 命令 / 选择 | 实际结果 |
|---|---|
| `cargo test --locked -p lang-codegen --lib map_word_frequency_tests` | 4/4；single 与两 unit 入口、正逆 source 排列、完整 native stdout；无 counter |
| `cargo test --locked -p lang-frontend --test type_map --test ownership_map --no-fail-fast` | 15+5；未判空 get/remove 的 L0084/Span 与原 Map 能力负例保留 |
| codegen `--lib ssa::unit_lower_borrow_tests` / `borrow_call_lowering_tests` | 2+5；原 shared root/loan forwarding/grouped Borrow 消费者 |
| codegen `--lib rejects_inline_nullable_lowering_without_panicking` / `nullable_map_promotion_preserves_direct_inline_nullable_rejection` | 1+1；普通 inline nullable 及 nullable Map storage 边界不扩张 |
| codegen `--lib native_map_copyable_` / `string_clone_cross_file_borrow_container_and_return_reach_native` | 3+1；缺失/0/false/value-class 与原跨文件 clone |
| `cargo build --locked -p lang-cli`；`check_word_frequency.py` / `check_tutorial.py` | CLI 成功；40 命令 / 32 执行病例，1 planned 明确未执行 |
| `cargo clippy --locked -p lang-codegen --all-targets -- -D warnings`；frontend 同形命令；fmt | 通过 |
| word-frequency Python reference / two-pass-growth / dual-host-wiring 三个指定 test | 3/3；未执行该文件 fault/timeout/signal 注入测试 |

所有新增 Rust 文件低于1000行；两个既有超限 test 入口仍在既有精确例外内，未抬高baseline。
未执行 macOS、完整 CI、preview、故障生成/缩减/注入/校准、LLVM 改写、counter 与 aggregate；
本机通过不是这些项目通过。Spec 保持 in-progress，远端最终验收与归档由后续交付闭环完成。
