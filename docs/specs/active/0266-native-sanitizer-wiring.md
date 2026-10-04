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
| W2 | Linux ASan 三类故意错误非零退出且类别准确；clean 输出和资源计数准确；关闭组不误报检测成功 | Mac普通clean与资源计数已通过；CI run37198053946的Linux ASan三种错误、clean及关闭组均通过 |
| W3 | Linux LSan 正常无环无报告、故意泄漏有报告且非零、关闭检测无报告 | CI run37198053946的clean开/关组通过，故意泄漏启用组exit0无报告；故意泄漏关闭组未运行；后续已证明栈根，worker修复待原生CI |
| W4 | LLVM21.1.8/compiler-rt 固定安装与运行；缺工具、超时、零测试匹配、普通失败不可算检测成功 | 脚本测试12/12（含缺工具、真实超时与进程树终止、零匹配、错误类别/普通失败、Debian runtime布局、诊断不替换原失败、C worker入口）；原CI政策18/18；Linux固定安装在run37198053946通过 |
| W5 | 限时与完整失败产物、Python 负向门禁、docs/inventory/尺寸/fmt/定向 Clippy、独立评审 | 合并前Python123/脚本9/docs507通过；当前Python129、脚本12、docs508通过；尺寸、改动Rust文件fmt、codegen all-targets严格Clippy通过；独立评审P2 counter失败材料缺口已修复并复审通过；安装路径修复已通过独立窄复审 |
| W6 | PR关联head与实际编译SHA/tree核对、Linux动态CI、双宿主普通测试、同步 Architecture 后按 PR 闭环归档 | PR #44 run37198053946编译merge tree与关联head tree一致；安装与双宿主sanitizer模块2测试通过，额外Linux步骤在LSan故意泄漏漏报处失败；后续诊断已证明栈根，新worker入口待CI |

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

PR #44 首轮 CI（run `37197238891`，关联 head `080b660`）在 Ubuntu LLVM 安装后的
runtime 路径检查退出1；日志确认固定版本 `libclang-rt-21-dev` 已成功安装，但双宿主测试
因前置失败未运行。官方 apt 包索引与下载包 SHA256 一致：
`d6f84c34953b0404ec9648ba7364004a82454f79cc447f67894b9a27317ef6ce`，其 ASan/LSan
静态库实际位于 resource `lib/linux` 下。LLVM21.1.8 的
[Driver 查询](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/Driver/Driver.cpp#L2340)
返回可能不存在的 per-target runtime 路径，而
[链接器查找](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/Driver/ToolChain.cpp#L710)
会回退到旧布局；本地同版本 Clang 对解包目录的 Linux target `-###` 也确认这一差异。
安装和验收现在统一用 `--print-resource-dir` 加固定 Debian `lib/linux` 布局，并明确输出缺失
archive 路径。模拟该目录差异的回归先红后绿；8项脚本测试、全部122项Python测试、
`bash -n` 与diff检查通过。这些是路径修复和本地命令计划证据，不能替代 W2/W3 的 Linux
动态运行；独立窄复审已通过，当前仍待 PR CI 重跑。

路径修复后的 PR CI run `37198053946`（关联 head `07b5a04`，实际编译 merge SHA
`a81558feb948009b46fb6319a638deb5960b0bea`）已通过 Linux LLVM 安装和双宿主普通
sanitizer 模块测试；额外 Linux 动态步骤的 ASan 三种错误及关闭组完成，但 LSan 故意泄漏
启用组返回0、stdout为 `read\ndrop\n`、stderr为空，W3尚未通过。失败产物
[artifact 11302335740](https://github.com/Halckon/Koven/actions/runs/37198053946/artifacts/11302335740)
的实际 IR/ELF 均确认 `f1.make` 调用拦截器 `malloc(4)`，Cell drop只保留deinit而无free；
`.preinit_array` 指向 `__lsan_init`，执行环境明确 `detect_leaks=1:exitcode=87`。
反汇编还显示Cell地址残留在已返回函数的栈槽；这是保守根扫描漏报的候选解释，尚未获得
LSan扫描日志证实。当前仅为原始漏报追加一次10秒内诊断运行，开启官方debug选项
`verbosity/log_threads/log_pointers`，保留原执行结果并仍让原断言失败；不关闭任何根扫描，
不把诊断重跑当成验收成功。待下一轮原生CI采集原因后再决定fixture修复。

已通过 GitHub API 核对上述编译 merge SHA `a81558feb948009b46fb6319a638deb5960b0bea`
与 PR head `07b5a04bdf9c6e69336cb93d659a293ee7c2e6c6` 的 tree 均为
`b9d5645717d5e155f873fc154e702f51d645b09b`：源码树一致，两个提交 SHA 并不相同。
诊断回归现使用真实检测器，覆盖诊断返回有效exit87泄漏报告、普通exit0、spawn失败和超时四种
结果，均保留首次失败异常且不生成验收成功文件。临时将实现改成“用诊断结果替代原结果并再次
断言”的mutant，四种子场景全部失败；恢复实现后脚本9项通过。

诊断补丁独立窄审确认生产异常路径保留原失败，指出原回归 mock 存在盲点；
修订后主协调者独立以内存 mutation 复核，四种子场景均杀死替代结果错误，真实脚本9项通过。

诊断 CI run `37199384909`（关联 head `56527d1`，实际编译 SHA
`10bd0cf8d44735262378841e16092074f56c6947`）的
[artifact 11301664626](https://github.com/Halckon/Koven/actions/runs/37199384909/artifacts/11301664626)
中，`diagnose-leak-lsan.stderr`第27–29行明确记录主线程STACK范围内的槽位指向
size4分配，随后扫描该HEAP；原始运行与诊断均exit0。至此已证明保守栈根导致漏报，
不再只是反汇编猜测。官方同版本
[LSan线程回归](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/compiler-rt/test/lsan/TestCases/create_thread_leak.cpp)
已使用pthread创建/join后的泄漏检查；仓库无可复用的线程入口adapter，既有生产LLVM main
wrapper和allocator counter不承担此生命周期控制。

本轮整合 `origin/main 5fbc664`，保留0267归档、editor job及双方required集合，
重生成Spec DAG。仅LSan测试链接增加小C adapter与`--wrap=main`：原Koven IR不改，
clean/leak及各自检测开关均在worker调用原main，精确匹配当前无参数main ABI并保留退出码；join后才返回。
线程创建/join失败以普通exit90结束，避免基础设施错误被退出时LSan报告掩盖；无全局抑制或
root扫描关闭，也不声明Koven并发语义覆盖。C入口源一同保存在失败artifact。
接线回归先因缺少worker链接与CI路径过滤而红，接入后通过；将adapter临时改成原线程直接
调用main的mutant被四种子场景全部拒绝，恢复后C入口正反测试通过。后续原生Linux CI仍需
证明实际4字节分配报告、clean输出、关闭对照与原counter，不能以本机C oracle代替。

最终无参数入口版的本机验证：`python3 -m unittest discover -s scripts/tests` 129 passed，
其中sanitizer脚本12项含真实C编译/运行与main ABI负向检查；`python3 scripts/check_docs.py`
通过508页，DAG重新生成及diff检查通过。此前123项/507页是合并前记录，本轮未运行Cargo；
继承的0267编辑器实现来自已合并主干，未声称本轮重跑其独立Rust/Tree-sitter验收。
worker入口独立复审已通过，核ABI、join同步、失败分流、同入口对照与CI接线；
复审独立运行sanitizer和CI政策33项通过。下一轮PR原生动态CI仍待验。
