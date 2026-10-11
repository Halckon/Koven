# SPEC-0296: owned nullable 最后判空与循环清理

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施或验收 owned nullable 判空 cleanup 修复时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-296` |
| 所属 Phase | Phase 3 具名事实交接与临时结果清理事实、Phase 4 SSA/native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[所有权与双轨析构](../../guide/10-ownership-borrowing-drop.md)、[空安全](../../guide/09-nullability-errors.md) |
| 批准依据 | 用户要求继续下一里程碑；本片修复既有 MoveOnly Map.remove 普通源码缺口，本地实现与验证已授权，发布另由父任务确认 |
| 前置 Spec | SPEC-0288、SPEC-0295 |
| 前置 ADR | ADR-0017 |
| 影响范围 | 两 frontend cleanup planner、通用 nullable comparison 两入口 lowering、普通源码回归、事实文档 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 main `e168c485a5379764cf927433452f8b7059f7b334` 新建 `fix/spec-0296`，
分阶段修复 owned nullable 最后判空：具名 owner 的读取/清理交接，以及临时结果缺失的清理事实，使 MoveOnly Map.remove
结果在支持的普通源码循环与非循环判空中保持 owner 可用，并保全已有显式消费路径。
0295 [原始边界源码与失败](../../development/evidence/map-word-frequency-0295/known-boundary.json)
保留历史，不重写其当时结论。修复不得按 Map/String 名称、源码形状或循环特判。

## 2. 合同与范围

- presence 读取必须先于该读取对应的最后使用 drop；沿用前端 typed null descriptor、
  source-qualified ownership/drop facts 和已有 aggregate/pointer 表示，不重新猜测 liveness。
- 纯内存 MoveOnly 遵循现行 ASAP；具名资源遵循词法退出逆序析构，资源临时值在完整表达式末清理。消费交付后原绑定不能重复 drop。
- 单文件、普通 unit 与常量 unit 使用同一不变量；循环回边保留必要 owner/loan，
  unit 循环的分支出口、判空、分组/交换操作数、显式消费及缺失结果均有正常源码覆盖；
  single for 的既有 guarded-owner 门独立记录。
- 先重放原项目并补最小红测，区分 for 根因与一般最后判空；任何更广问题单独记录。

## 3. 非目标

Guide/Map API/nullable 语义修改、Map 迭代、consume、新布局或新借用能力、LLVM/runtime 改写、
完整 CFG/NLL 重构、通用 eager binary 或条件借用扩展、放松 verifier、性能宣称。云本机只跑正常源码、普通负例与静态检查；
不跑故障注入、校准、malformed IR、LLVM/allocator counter、reducer 或聚合驱动。双宿主 CI 留远端。

## 4. 第一阶段验收账本

| ID | 验收项 / 目标 | 实际结果 |
|---|---|---|
| R1 | 原 0295 known-boundary public build/run，记录基线红与修复绿 | 原项目与single scalar合计4条公开build/run，exit0及精确stdout/stderr |
| R2 | 正常源码 SSA/native：单文件及两 unit，String/Resource、for/非循环、分支、判空、消费与缺失 | 12/12，0 ignored；unit source 正逆排列，Resource 4 类 loop 退出，详见 matrix green |
| R3 | frontend 普通能力负例与相邻通用 nullable/Map 回归 | 33项codegen直接消费者及20项frontend Map正负例通过，0 ignored |
| R4 | fmt、受影响 crate 严格 clippy、docs、diff、Rust 尺寸静态检查 | 全通过；codegen --all-targets -D warnings；37项docs checker测试；无尺寸新例外 |
| R5 | 独立审查、远端双宿主 CI、最终归档与精确 head 交付 | 最终本机证据已独立只读复核；远端CI及生命周期仍待授权/执行 |

## 5. 实施与提交

1. 合同与基线 red；核实通用事实消费边界。
2. 最小生产修复和同源定向回归；真实 native stdout 验证。
3. Architecture/验收记录和独立审查；获授权后由父任务负责 PR 与 CI 闭环。

每个逻辑切片独立提交并标记 SPEC-0296。共享 Cargo target 串行使用，不执行 cargo clean。
完成全套必需验收前保持 active；本机结果不替代远端 CI。

## 6. 调查与验证记录

编号已确认未占用；没有仓库级 `.agents/skills`。工具链 Rust 1.96.0、LLVM 21.1.8。
基线 native CLI 构建成功，原项目 build exit 1 / InvalidSsa。进一步对照确认 unit 最后 if 判空
也失败；single 直接 if 已有 NullableBranch 保护，但 Boolean 绑定判空同样失败。
原样 7 项正常源码测试为 1 通过、6 InvalidSsa 失败；最小 descriptor/表示接线修复后 7/7 通过。
原始命令、日志和 SHA256 见[本机 checkpoint](../../development/evidence/owned-null-cleanup-0296/local-checkpoint.json)。
循环原项目由 unit/basic 与 const 原样验收；single 外层 Map owner 进入 for 在更早的既有
UnsupportedNode 门禁被拒绝，另存正常源码，暂不纳入本片能力扩展或通过计数。

扩展探索另确认直接 `m.remove(1) != null` 不发布结果 Temporary drop，和 named 的提前
drop 不同；两入口仍为 InvalidSsa。这是第一阶段明确保留的 Phase 3 缺口；用户随后要求并入本 Spec 第二阶段，不能将先前红测计入第一阶段绿验收。

扩展[正常源码矩阵](../../development/evidence/owned-null-cleanup-0296/local-matrix-green.json)
12/12：6 种相等/不等、左右/group 形态分别覆盖 String 与 MoveOnly Text 的 present/missing，
SSA 精确核对 MapRemove 的第二个 owned result 先读取 presence 后唯一次 drop；不复制 owner。
Resource 的消费通过既有显式 `!!` 交付，正常/continue/break/return 的词法次序由 stdout 验证。
条件分支中的 Resource 消费仍受既有 ResourceLifetime 门限制，不从本片推定通用支持。

第一阶段完整[验收记录](../../development/evidence/owned-null-cleanup-0296/phase-one-acceptance.json)
保留各命令完整输出及actual compiler哈希；12项新增矩阵、33项直接消费者、20项frontend，
四条公开build/run及静态门禁通过。独立首轮审查未见阻塞；远端双宿主与生命周期仍未执行。

## 7. 第二阶段：临时 nullable 结果（2026-10-11 追加授权）

用户在第一阶段checkpoint后要求把先前直接 `m.remove(k) != null` 的清理缺口也并入0296。
本阶段承接该候选；其他 key 提前 return、N1a 标量 CFG、窄化 MoveOnly Borrow 与 single for
carrier 缺口不扩入本片。第一阶段证据和当时边界保留，不追改已执行结果。

- 两 frontend planner 只在合法 `==`/`!=` nullable-vs-null 比较的实际 operand 完成后，
  根据已有 type/category/ownership 事实为 MoveOnly temporary 发布显式 `AfterBinaryOperands`。
- 另一侧必须是透明 Group 归一后的 null literal；它无副作用且不创建 payload owner。
  实际 operand 也归一 Group 叶与 origin；复用既有 `is_move_only_temporary` 排除 borrowed
  result/View 等非 owning 值，不从 AST 外观推定所有权，不泛化所有二元操作。
- Operand 不继续或 Abort 时不发布正常结果 cleanup。没有第二个有作用的 operand，因此
  不新增 pending frame，也不扩展任意 nullable closure/control temporary 的来源运输能力。
- 后端只在 representation 对应的 presence 读取后消费上述 facts：aggregate 读取 presence，
  pointer handle 使用 NullableIsNull；不改 verifier、ABI、runtime，不由后端自行造 drop。
- 具名资源仍词法清理；Resource temporary 在本次比较完成后、if body 前析构。命中一次、
  Missing 不产生 payload 析构；Group alias 不重复清理。

| ID | 第二阶段验收项 | 实际结果 |
|---|---|---|
| T1 | 单文件/unit真实facts：actual operand一次、AfterBinaryOperands、精确origin；Copyable/named/borrowed/未完成对照 | 5/5；另锁key CallReturn与结果清理分离、inout receiver负例 |
| T2 | direct/group、null左右、==/!=、String/Text/Resource、命中/Missing；Boolean和if，single与两unit native | 临时9/9；unit循环4退出与0/1/重复键；原项目/scalar/temporary合计6条公开build/run通过 |
| T3 | 第一阶段矩阵及直接消费者、相邻nullable/String ownership；frontend/codegen严格clippy与跨crate静态check | cleanup22、直接消费者33、frontend107+6通过；clippy/workspace all-targets check及其余静态门禁通过 |
| T4 | 独立审查、最终本机证据；远端CI仍独立 | 独立合同/实现/矩阵审查无阻塞；最终本机证据已独立只读复核通过，远端CI未执行 |

先保存本合同与真实红测，再做最小事实和消费接线；每个逻辑切片独立提交。

第二阶段合同先提交 `9c24a3ac`，真实红测 `d9f9f700`：frontend 正例缺结果 fact，
两项排除对照通过；4 项 native 在单文件 InvalidSsa、unit Resource pointer UnsupportedNode。
`ae65b2ce` 只补 frontend 结果事实和表示读取/消费接线，首轮 3+4 项转绿；
`ad0ba270` 扩展为 9 项临时 SSA/native、5 项 frontend facts/普通负例，并保留单文件工厂规划边界。
原[边界账本](../../development/evidence/owned-null-cleanup-0296/known-boundaries.json)保留第一阶段红数据并明确临时结果已闭合。
单文件普通 Resource? 工厂无 Map/判空也在返回类型登记时报 UnsupportedNode，unit 对照通过；
不从本片推定任意 nullable 工厂、grouped null literal 或临时 inout receiver 获得支持。

[第二阶段最终本机证据](../../development/evidence/owned-null-cleanup-0296/phase-two-acceptance.json)
记录全部命令、实际命中数、stdout/stderr、源码与编译器SHA256。22项cleanup含1项保留工厂边界；
113项frontend为107项定向套件与6项String binary对照。首轮严格clippy只报新增测试冗余usize cast，
最小删除后复跑受影响测试、22项cleanup和严格门禁通过；未加allow、未改变生产代码。
原0295项目三文件逐字节对照main基线不变；远端CI、归档及合并尚未执行。

最终独立只读审查核对 `f6b15351`：17个Rust源码SHA256与工作树一致、原0295三文件保持
基线字节、各命令实际命中数/exit与汇总一致，独立diff检查通过；审查者未重跑Cargo。

## 8. 双宿主实现验收与有界归档（2026-10-11）

用户已明确授权本片归档、删除 active 旧路径及最终交付。按 [PR75](https://github.com/Halckon/Koven/pull/75)
[run38145836171 / attempt1](https://github.com/Halckon/Koven/actions/runs/38145836171) 的实际成功归档。
实现 head `258aaa48ee274c24d4378406a38ef2cbf3034c7f`、base `e168c485a5379764cf927433452f8b7059f7b334`，
双宿主 checkout merge `78cb64ac47de69538fce4825bc6b9304396c80a3`；三者中的实现/merge树与冻结本机树
均为 `7137e50471efc896c8c6ba3ab748bd4385c2328e`。15 jobs成功，Tree-sitter输入未变合法skipped；
双host targeted、四preview、Code CI Evidence、CI Passed均实际成功，并非复用跳过本次代码测试。

| Goal | 已核实的实现证据 |
|---|---|
| R1/R2 | 本机6条公开build/run保留原stdout/stderr；双host逐名核22项cleanup（含1项工厂拒绝边界），原0295循环provider按原字节执行于unit native测试 |
| R3/T1/T2 | 每host另33项codegen直接消费者与25项Map frontend；55+25项每项恰一次ok、0 selected ignored；5项临时facts/普通负例包含其中 |
| R4/T3 | 双host workspace check/严格clippy、fmt、docs、尺寸、依赖与规定workflow通过；本机113项frontend与远端选择分别记录 |
| R5/T4 | 本机实现及证据独立只读审查通过；实现CI与逐名远端日志核对完成；归档head/merge/main仍独立待闭环 |

[交付账本](../../development/evidence/owned-null-cleanup-0296/delivery.json)固定run/job、选中清单、
完整decoded日志SHA256与原行摘录；原本机红绿证据作为当时快照保留，不追改历史状态。
Linux全codegen为1271 passed/0 ignored；macOS为1270/1，唯一ignore是既有LLDB debugserver
无task-port权限，未混入本片55项codegen通过计数。

远端generated-resource证据Linux为pass/requirements_met=true，macOS为partial/false：
`address:macos-asan-unsupported`、`leak:macos-counter-only`。两host的16个普通生成病例通过
不等于双平台完整sanitizer能力验收。Linux sanitizer原记录亦明确LLVM IR UBSan **NOT COVERED**。
云归档仅下载和静态读取这些证据，没有本地执行artifact程序、故障注入或聚合驱动。

本次只关闭具名最后判空与已完成临时nullable结果清理的有界Goal；single for guarded owner、
普通Resource?工厂规划及其他记录边界未被扩大。最终归档head CI、merge和actual main CI仍pending，
不能由本次实现run推定通过；后续状态由交付账本记录。
