# SPEC-0266: Koven native 地址与泄漏检测接线

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施或验收 M4a Linux 检测首片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-266` |
| 所属 Phase | Phase 6 验证；消费现有 Phase 2–5 产物 |
| 语言规范 | [Guide v0.40 runtime](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户于 2026-10-04 持续授权实施里程碑，允许前置满足的独立切片并行推进 |
| 前置 Spec | SPEC-0228 |
| 前置 ADR | ADR-0026 |
| 影响范围 | `lang-codegen` 私有 native 测试；Linux CI 与检测脚本 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

Linux x86_64 上，实际 Koven 生成的用户函数、runtime 初始化和 drop glue 中的选定
地址错误能被 ASan 检测，正常无环程序通过；LSan 独立证明选定泄漏的正反与关闭对照。
每次检测保留真实工具链、参数、产物和稳定错误类别，接线缺失、超时、零匹配显式失败。
起始基线为 `origin/main 11acf62`，后整合已归档0264的 `f0effcd`
及纯文档交接 `8408d7`；实施分支
`feature/spec-0266`。这是 M4a 独立首片，
不关闭 [M4 起草材料](../../development/memory-safety-validation-spec-draft.md)其余范围。
现行 class/字段/native 分配和 drop 已足够构成 fixture，不依赖尚未完成的 M1A unit for。

## 2. 范围与边界

复用既有分析、verified SSA、LLVM render 和 `ir_clang`，仅测试 helper 对实际 LLVM
有定义函数添加 `sanitize_address`。用户函数越界读、缩小实际 malloc 保留 runtime store、
drop free 后 volatile 读各有故意错误、clean 与移除属性但保留 runtime 的关闭对照。
原始 LLVM、修改后 LLVM、插桩后 LLVM、实际编译/链接与执行结果均留证；机器地址不入 golden。
资源计数另跑，不让 counter 的全局 live pointer 隐藏 LSan 泄漏。泄漏 fixture 删除一个
实际 free，正常返回后检查；显式 `detect_leaks=1/0` 对照，不从 ASan clean 推导泄漏覆盖。

UBSan 对 Koven LLVM IR 未覆盖：现有 Clang flags 不能自动恢复源语言检查，C 探针
不替代 Koven 证据。该缺口、随机生成 M4b、外审 M4c、并发、所有平台、容器逻辑长度内
越界和栈 lifetime 检测均属后继。本片不新增 CLI 参数、生产不安全语法或依赖。
Abort 不展开、Rc 环允许未释放的语义保留；本片只选择正常无环路径，不用全局抑制规避泄漏。

## 3. 验收账本

| ID | 合同 | 实际结果 |
|---|---|---|
| W1 | 实际 Koven fixture 正常分析/lower，三个目标定义函数各有访存检查；关闭属性无检查 | Mac LLVM21.1.8：最终两项2 passed/796 filtered；导出1 passed/797 filtered及外部三target/三off IR检查通过，均0 ignored；counter 1分配/1释放；属性换成nounwind有效红测失败，恢复后通过 |
| W2 | Linux ASan 三类故意错误非零退出且类别准确；clean 输出和资源计数准确；关闭组不误报检测成功 | Mac普通clean与资源计数已通过；Linux ASan动态待CI |
| W3 | Linux LSan 正常无环无报告、故意泄漏有报告且非零、关闭检测无报告 | 待 CI |
| W4 | LLVM21.1.8/compiler-rt 固定安装与运行；缺工具、超时、零测试匹配、普通失败不可算检测成功 | 脚本负向测试7/7（含缺工具、真实超时与进程树终止、零匹配、错误类别/普通失败）；原CI政策18/18；Linux安装/执行待CI |
| W5 | 限时与完整失败产物、Python 负向门禁、docs/inventory/尺寸/fmt/定向 Clippy、独立评审 | Python全部121 passed，最终脚本7复跑通过；docs507、尺寸、改动Rust文件fmt、codegen all-targets严格Clippy通过；独立评审P2 counter失败材料缺口已修复并复审通过 |
| W6 | PR关联head与实际编译SHA/tree核对、Linux动态CI、双宿主普通测试、同步 Architecture 后按 PR 闭环归档 | 待 CI；本轮不自行提交或远端写入 |

## 4. 实施顺序

1. 登记合同与红测：移除属性必须失去指定访存检查，普通进程失败不能伪装 sanitizer 报告。
2. 最小测试私有 LLVM 修改/导出、Clang 插桩核验与 Linux 正反执行，保存失败材料。
3. CI 固定 compiler-rt、必需 Linux 步骤和直接政策测试；同步事实与独立评审。

## 5. 当前证据边界

前置调查在 Darwin arm64 LLVM21.1.8 证明 LLVM 函数属性加 Clang ASan flags 才插入指定
访存检查；仅 flags 不插入。UBSan C overflow 有 handler 和报告，手写 LLVM add nsw
仅加 flags 没有 handler 且 exit0。ASan/LSan 本机动态初始化后超时，包括 clean 和沙箱外
重试；这些不是本 Spec 的 Linux 验收，也不是实际 Koven fixture 已通过的证据。

## 6. 实施验证记录

定向命令：`cargo test --locked --offline -p lang-codegen --lib native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop -- --exact --nocapture`。
本机复用共享 `CARGO_TARGET_DIR`，LLVM prefix 为 Homebrew llvm@21；没有增加target或依赖。
首轮fixture选择假设暴露String literal glue也有条件free，现按实际owner参数身份唯一选择Cell
free，而不按函数编号或所有free数量猜测。随后有效红测只移除ASan属性，报告明确为
`f2.read: expected ASan load checks=True, got False`；恢复后重新通过。测试失败目录按合同保留。
脚本CI接线测试先因必需Linux步骤缺失失败，补步骤后通过；编译期检查不能代替 W2/W3。

最终复基后还执行 `cargo test --locked --offline -p lang-codegen --lib native_tests::boxed_enum_tests:: -- --nocapture`：
共享counter普通调用方4 passed/794 filtered、0 ignored；两条sanitizer模块测试覆盖正常IR接线及
真实counter编译/运行失败后留存输入、精确NUL argv/cwd、版本、stdout/stderr和status。
`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`通过；新增Rust文件低于1000行，
未增加尺寸例外，现存45个超限文件只报告旧欠账。独立评审还明确本地普通counter无独立期限，
Linux driver则对Cargo整体限时900秒，对后续检测命令逐项限时；文案已按实现收窄。
