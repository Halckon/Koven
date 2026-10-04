# SPEC-0269: 有界资源程序生成与独立安全核验

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施或验收 M4b 首片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-269` |
| 所属 Phase | Phase 6 验证；消费现有 Phase 2–5 产物 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[所有权](../../guide/10-ownership-borrowing-drop.md)、[析构](../../guide/08-class-family-members.md#deinit-成员语法与资源析构契约) |
| 批准依据 | 用户持续实施里程碑、满足前置可并行及按实际调整草稿的授权；没有启用新语言语义 |
| 前置 Spec | SPEC-0266 |
| 前置 ADR | ADR-0026 |
| 影响范围 | 私有 codegen 测试、Python 生成/核验脚本、现有双宿主 CI |
| 语言语义变更 | 否 |

## 1. Goal 与基线

固定 seed 生成现行语义内的有限资源程序，由独立模型核验实际输出、资源计数与精确拒绝，
Linux 对实际 Koven LLVM 运行地址/泄漏检测；失败保留源码与环境，可重放并在同一原因下有界缩小。
基线：`origin/main 7c093bb6fbb8e4c0b590b18c7b167d2f094b4c1a`（PR46 合并）。实施分支 `feature/spec-0269`。
本片承接 [M4 起草材料](../../development/memory-safety-validation-spec-draft.md)的选定 S01–S06，
不关闭 M4 全部范围，也不声称完整内存安全证明。

## 2. 范围与固定生成域

- V1：普通资源 `Leaf(val name: String)`、create、move、同步 Borrow/read、consume、词法清理；
  唯一 literal 名称在 deinit 打印，borrow/consume/done 标记分开。最多 12 操作、8 个 Leaf。
- V2：`Holder(var state: Leaf)` 的单层一次 replace；固定 helper 可在 replace 前或后 return，
  Boolean 两值都有见证；继续路径状态一致。可有独立局部资源；entry 的 done 区分 helper 与程序退出。
- I1：从 V1 派生 move 后再次访问，精确 L0131；I2：Borrow 参数被消费，精确 L0133。
  前置分析阶段不得另有错误；primary/唯一关联 label 以 renderer 记录的 UTF-8 byte span 判断。
  固定非法见证在目标 token 前包含非 ASCII 字节，不能使用字符串 find 猜同名 occurrence。
- 日常集合恰为 8 个有效案例（V1/V2 各 4）、8 个非法案例（四个种子各有 I1/I2）；
  显式要求 move/Borrow、replace old/new、两处 return×两值及 done，不依赖随机恰好命中。
- 不生成循环、容器、任意 CFG、条件 owner 合流、闭包、Rc、Box/enum、nullable、动态字符串或线程。
  Abort/Rc 环、跨文件、纯内存 ASAP、UBSan、并发检测、跨版本差分和外审属于未覆盖项。

## 3. 独立 oracle 与物理证据

结构化操作记录是 renderer 和模型的共同输入；模型用 owner 表与 scope 栈独立计算，
不消费 AST、SymbolId、ownership/drop plans 或编译器实际输出。renderer 只写源码及 occurrence。
手写 V1/V2 事件 golden 锁定 moved source 不析构、Borrow 后可用、old/new 区分、return 和逆序退出；
禁止由 evaluator 自动刷新 golden。未在 native 采样的 move/loan 事件标为模型预期。

每个有效案例：完整 stdout bytes/空 stderr/exit0 与模型相等，现有逐指针 counter 检查实际构造数、
唯一释放及最终 live 集为空；Linux 另以未替换 malloc/free 的 raw LLVM 检查 LSan、全函数
sanitize_address LLVM 检查 ASan，不能用 counter 的全局 live roots 跑 LSan。

固定 V2 额外以独立指定的构造序号核对字段→实例物理释放次序，复用现有 counter order 选项；
提前释放 Holder 的真实 LLVM mutant 必须被此见证拒绝。该序号是固定实现回归证据，不是语言规则。
一般生成样本只证明逻辑 deinit 顺序、物理唯一释放，不声称逻辑 owner 与物理 free 次序完整关联。

## 4. 实现边界与复用

使用 Python 标准库，不引入依赖。一个 test-only Rust exporter 复用现有分析、SSA/LLVM verifier，
仅导出真实阶段诊断、raw/ASan LLVM 和计数 C/IR；不在 exporter 内启动无独立预算的 clang/native。
现有计数器最小分离“写产物”与“执行”，原调用方行为和失败留证保持。
共用 ASan 全定义函数属性标记、Linux LSan worker 与已有运行设施，不复制编译流水线。
M4a 固定 probes 原样继续运行，不因新生成器放宽其完整清单或检测类别。

一次 Cargo --no-run JSON 定位真实 exporter 二进制；每 case 串行 exact 调用，必须实际命中一次、
0 ignored，零案例/缺少产物显式失败。普通 codegen suite 只执行最小导出自检，不隐式重复全批次。
CI 复用现有双宿主 Targeted Tests 和工具链，Linux 才运行动态 sanitizer；不新增 cron/矩阵维度。

## 5. 稳定输入、证据与重放

有界选择用 SHA256(generator_version/seed/case_index/decision_key)，记录编码和选择 golden；
不依赖 Python hash、系统随机或无序迭代。两个新目录生成相同源码/操作/oracle bytes。
重放直接使用保存的源码，不要求先由当前生成器重造；结构记录和预期的哈希均核验。

每个新运行目录保存 manifest、原始源码/结构/选择/owner ledger/预期、输入哈希、compiler SHA/dirty
与必要 diff 哈希、Cargo.lock 和 exporter 二进制哈希、OS/arch/toolchain/runtime archives，
以及实际 argv/cwd/env/预算、stdout/stderr/status/signal/超时/耗时/资源观察值、LLVM/C/产物和阶段结果。
成功与失败 CI 包均上传；不覆盖旧目录，不让 dirty 本地记录冒充 clean 提交证据。

## 6. 有界缩小与失败分类

只结构化删除无依赖局部/inspect/marker、缩短 move 链或名称；保留有效域、定义引用和故障见证，
不做任意 token 删除。I1/I2 保留对应根因与 occurrence，parser/backend unsupported 不替代原失败。
失败指纹是阶段、类别、稳定语义见证；sanitizer 包含 detector/category/实际函数角色，
不能只匹配非零退出或机器地址。候选严格变小且指纹相同才接受，顺序确定。
最多 32 候选/120 秒；原始失败与最小候选均保留，最终独立重放三次。
预算耗尽写 minimization_incomplete；原因不稳定写 flaky，不能宣称全局最小或洗掉原失败。

类别分别保留 unexpected_frontend_rejection、unexpected_acceptance、diagnostic_mismatch、
ssa_or_codegen_failure、native_output_mismatch、resource_counter_failure、asan_error、lsan_error、
process_crash_without_detector、timeout、resource_limit、tool_or_harness_failure；最早失败不可被后续诊断覆盖。

## 7. 故障校准与资源预算

生成域真实 LLVM 各选一次地址错误、漏一次 free、漏一次 deinit 输出和提前 Holder free，
保存唯一目标、变异前后哈希与 verifier 通过；clean 先通过，实际指定 detector/oracle 后拒绝。
地址故障检查实际 Leaf(String) layout/operand，不套用旧 Cell 的 4-byte offset。
关闭检测对照不算检出。诊断适配层删码/换码/错主与关联 Span 必须失败。
首次交付另在临时副本禁用实际 I1 路径一个 L0131 报告点：保存 patch/hash、重编译红测，
恢复源码核对 diff/hash、再重编译同案例绿测；不将适配层负例代称杀死 checker mutant。
该专项不逐 seed 或每次 PR 重编译；不提交 mutant 到正式代码。

初始预算待双宿主测量校准：源码 8 KiB、export 20 秒、clang 30 秒、native 5 秒，
Cargo 构建单独 900 秒、生成执行 300 秒；阶段日志各 1 MiB、运行目录 64 MiB。
逐阶段留证、进程组超时终止并 wait 后再运行下个案例。Linux /proc 每 50ms 采样同组 RSS/进程，
超过 1 GiB/16 终止并留证；这是采样防护，非内核硬上限，不限制 ASan 所需巨大虚拟地址空间。
Mac 不声称 Linux RSS/pids 防护；仍有域/数量/时间/日志/产物预算，限制写入 manifest。
受控分配、子进程、输出爆量、timeout 和监控不可用负测验证终止与分类，缺监控不成功跳过。

## 8. 验收与执行顺序

| ID | 完成标准 | 当前证据 |
|---|---|---|
| G1 | 独立模型/renderer 手写 golden、有效/非法域、固定选择和非 ASCII byte span | Python模型18项通过；独立审阅未发现具体模型错误；实际8个非法case诊断和byte span通过 |
| G2 | 8 有效+8 非法真实编译核验；双宿主输出/计数、固定 V2 释放次序 | Mac首轮6有效通过、2有效InvalidSsa；合入已合并0270后8有效及8非法全部通过，固定V2次序通过；Linux未运行 |
| G3 | Linux 生成案例 clean ASan/LSan、真实 IR 四种故障、关闭对照；M4a 保持 | 未执行 |
| G4 | 诊断适配负例与真实 checker mutant 红→恢复→重编译→绿 | 未执行 |
| G5 | 输入稳定、原始源码重放、同因缩小、三次重放、预算/分类负测 | 原始有效case重放、预算/分类单测已执行；同因缩小与Linux实测未完成 |
| G6 | CI 拒绝零命中/缺项/工具/必需跳过，保全证据并记录双宿主成本 | 未执行 |
| G7 | 独立审阅、受影响回归、fmt/Clippy/尺寸/docs、Architecture 与归档 PR 闭环 | 未执行 |

按 G1 → 导出/执行 G2 → 检测 G3/G4 → 重放缩小 G5/G6 → G7 推进。
Cargo 共用一个串行窗口；先失败测试再实现。Rust 只选新 exporter、被提取 helper 原调用方、
M4a 与资源/replace 定向套件，不默认运行 frontend 全量。Python 选择新驱动、M4a 及 CI/docs 合同。
单一目标可拆小提交，独立审查后提交；未完成 Spec 不归档，最终归档 head CI 通过才合并。

## 9. 起草调整与预检边界

M4 起草材料列出容器迭代等候选范围；本片以两个固定形状缩小生成域，保留独立 oracle 和实际检测。
合同独立审阅发现输出+计数不能证明字段→实例 free 次序，增加固定 V2 见证，避免扩建通用 tracing。
PR46 实现版本本机 CLI 曾通过 V1 一次、V2 四次固定 build/run 与 I1/I2 精确 byte span；
这是启动可行性预检，未验证本 Spec 的生成、计数、sanitizer、缩小、mutant 或预算。
该预检当时不构成 G1–G7 正式验收；后续实际结果见验收表和开发检查点，不回写历史。

## 10. 开发检查点与保留项

正式生成首次实际运行使用共享test binary，源码/编译器dirty状态、文件哈希、实际命令与输出
保存在 `/private/tmp/0269-first-generated-run`；剩余案例观察在
`/private/tmp/0269-remaining-generated-run`。V2两个含local移动的案例在条件控制流处InvalidSsa，
单文件CLI也失败。0270从同main基线独立修复旧SSA绑定残留，0269不改生成集合来隐藏它。

独立审阅指出地址被带入失败指纹、最终日志写入可绕过采样预算；两项已加入具体回归并修复。
原始缩小只重复相同InvalidSsa字符串，不能证明相同语义原因；现对没有明确见证的错误停止
自动缩小，保留original并写minimization_incomplete。该记录不构成G5完整验收。

当前driver尚未完成故障校准/CI接线；全batch末尾显式拒绝验收，不能误报完成。
校准子任务曾被平台内容检查终止；用户要求避免触发Daybreak后，此轮不重试被拦截的校准，
其合同保留未完成，不以模拟适配结果代替真实检测证据。继续推进普通编译器修复、正常
程序核验及预算/判定设施。G3/G4未执行，Spec保持in-progress，不归档。

普通导出器新增两条Rust测试通过（2 passed/820 filtered/0 ignored）：正常输入实际导出，
parse/name/type/ownership首个失败阶段只留真实诊断、不产LLVM。共享计数器提取经独立审阅
发现错误新增expected>0限制；既有空容器回归先红，删除共享限制后完整36组合的单个测试通过
（1 passed/821 filtered/0 ignored，69.99s），exporter自身1..=9域限制保留。M4a原2项与
boxed_enum原4项通过；新指纹及进程预算回归已复审关闭。Python当前46项中45通过、
Linux /proc监控1项在Mac跳过；Clippy/fmt/docs通过。这里是未提交开发树检查点，
未替代最终合并基线重放、完整CI或G3–G6校准。

合并后检查点：已合入 PR47 的 main `299469bd7234149eaa233e49e6a90011cdedfd50`，
集成提交 `5466d394b1e6853f292846f6f0f8d85113d23acc`。重建 exporter 后两条测试通过
（2 passed/825 filtered/0 ignored）。实际运行 `check_generated_owners.py --exporter <fresh test binary>
--artifacts /private/tmp/0269-post0270-generated-run`，16 个固定案例全部通过：8 有效的 stdout、
逐指针计数及指定 V2 物理次序，8 非法的精确诊断和 UTF-8 spans。原先失败的两个 seed 保留且转绿。
全批次最终仍按设计 exit1：`calibration-not-yet-implemented`，不是 M4b 完整通过；实际记录
含 dirty 工作树 diff、未追踪文件和 exporter SHA256。原始有效源码重放
`--replay /private/tmp/0269-first-generated-run/case-00` 也通过，记录在 `/private/tmp/0269-replay-valid`。
Python 四模块再验 46 项：45 passed、1 Linux-only skipped；未运行生成案例的 Linux 动态检测。

最终普通设施审阅还复现了两项判定缺陷：缩小重放的 I/O 异常覆盖首个 failure、仅换行字节
差异被误归 harness；分别加入失败测试并修复。后续独立窄复审通过，两模块14项无跳过。
合并基线最终 Python 四模块48项中47通过、1 Linux-only跳过；resource_deinit_ 32项通过、
795 filtered、0 ignored；codegen all-targets Clippy、fmt、尺寸门禁和512页docs通过。
这些是普通设施提交检查点，G3/G4与完整生成验收保持未完成；后续 draft PR 的现有CI
仅验证已接线的门禁，不能代称完整生成批次或故障校准已执行。
