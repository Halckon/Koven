# SPEC-0366: CLI 最终调试产物与源码断点

> **性质**：CLI 产物合同草案 · **状态**：in-progress · **读取时机**：交付最终 executable 的源码调试能力时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0366` |
| 所属 Phase | Phase 6 CLI；复用 Phase 4 调试行表 |
| 语言规范 | [Guide v0.46](../../guide/README.md) |
| 授权范围 | 持续推进编辑器里程碑，补齐消费方 SPEC-0011 的 debug build 前置；草案不授权未审结方案实施 |
| 前置合同 | [ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md)、[ADR-0011](../../adr/accepted/0011-first-dwarf-line-mapping.md)、[ADR-0026](../../adr/accepted/0026-linux-x86-64-native-host.md) |
| 阻塞项 | macOS 实现与定向验收已推进；Linux、双宿主 CI 及 PR/main 交付闭环待验证 |
| 影响范围 | lang-cli linker、single/project 构建产物生命周期及真实 CLI 测试 |
| 语言语义变更 | 否 |

## 1. 目标与基线

基线 main `48adcfab59e3d32be305e0f9a6d1127c157bbe71`，分支
`feature/spec-0366-final-debug-artifacts`。完成后，由真实 CLI 构建的最终可执行文件，
在编译临时对象已清理后仍能被受支持宿主的 LLDB 按实际源码文件/行解析并命中断点。
不以 codegen 对象行表测试代替最终产物验收。

预检 producer `3bd6abd3fa340ee0aef788923c97f69e23452b21` 在 macOS arm64
构建三行 main.ko 成功，但最终 executable 的 dwarfdump 无源码行记录，LLDB 第 2 行断点
pending；dsymutil 报告临时 `.o` 已不存在。当前基线只做源码核查：链接器仍仅链接对象，
project commit 在发布前删除对象。尚未把旧输入的运行结果当作当前基线或 Linux 的结果。

### 当前基线实测（2026-10-29）

在上述基线加本片纯文档提交的工作树，以 LLVM 21、Rust 1.96 执行
`cargo build --locked -p lang-cli` 成功（23.84 秒）。CLI SHA-256：
`89669e6cb1abcfa4ce716e66d0612e888bebdc1b5fd01568d83d4052b7f28f7d`。
同一三行无 package 的 public main 源文件，single/project 显式 entry 均 build exit 0；
从另一个 cwd 以实际绝对源码路径在第 2 行设断点，两者均报告 pending/no locations。
随后 dsymutil 均报告无调试符号，single 另报告其临时对象缺失。
LLDB 命令 exit 0 不代表断点解析成功，本次明确按其 pending 结果登记为未满足合同。

首次夹具声明 package app 却位于不匹配的逻辑目录，构建被 L0146 拒绝；修正夹具后才执行
上述产物探针，不把该前端拒绝当作调试缺口证据。探针及原始结果保留仓库外临时目录。
尚未执行实际 breakpoint run/hit、Linux，亦尚未添加实现回归测试。

## 2. 行为合同

- single 与 project 共用最终调试产物处理；现有入口、argv、退出码及 JSON 诊断协议保持。
  复用已有 SourceMap/行表，不增加语言解析、局部变量或类型调试语义。
- 构建成功意味着最终 executable 及宿主必需调试信息均已就绪，临时对象删除后仍可调试。
  无需用户手动从 CLI 临时目录抢救对象；失败不得返回构建成功。
- macOS 必须在对象仍可访问时完成所需调试信息收集；伴随产物必须与最终 executable
  匹配。Linux 验证最终 ELF 的源码行表和实际 LLDB，不能只依据平台代码路径存在判定通过。
- 产物路径由显式输出确定，不覆盖无关文件、目录、符号链接或既有调试包。
  构建、链接或调试工具失败必须保留既有产物，并清理本轮拥有的临时文件。
- run 的私有临时目录在进程结束、取消及启动失败后释放；build 产物保留用于随后调试。
  不因本片修改已明确的 child argv/退出码合同。

## 3. 发布方案待决

macOS executable 与 dSYM 是多路径产物，不能把两个 rename 宣称为单一原子事务。
实施前须明确：输出命名、已有任一路径的拒绝、工具失败与第二步发布失败的回滚、
并发创建或替换时的所有权识别，以及回滚失败的结构化 operational error。
优先复用现有 BuildArtifacts 与临时目录设施；不得为方便清理而删除用户旧输出。
方案若改变 accepted ADR 的决定，须先新增 ADR，不能改写历史决定。

### 已选发布条款（2026-10-29）

1. 现有 build 参数形状保持；macOS 成功 build 交付指定 executable 及同名追加
   `.dSYM` 的调试包，Linux 成功 build 交付含行表的 ELF。成功仅在两者需要的调试信息
   均准备并发布完成后返回，不以对象级成功代替最终产物成功。
2. 构建开始时，已有 executable 或对应 dSYM 路径（含悬空 symlink）均拒绝；发布也须
   no-clobber，不以事先检查代替发布时的冲突保护。
3. object、链接输出和调试收集先置于本轮私有 staging，保留 object 至调试收集完成；
   对外发布调试包就绪后，最后发布 executable，期间不启动程序。
4. 若调试包已经发布而 executable 发布失败，命令必须失败，明确报告已发布调试包位置。
   允许该调试包遗留；不得递归删除可能被外部替换的公开路径。未发布的本轮私有 staging
   仍需清理，清理失败也应具体报告。该例外已由 ADR-0035 与 Guide 明确启用。
5. run 的私有产物继续在进程生命周期内持有，结束后清理。项目的实际调试源码来源由
   CLI 明确提供，诊断与源集合逻辑身份保持原合同；codegen 不推测磁盘布局。

用户已选择上述默认伴随调试包方案，依据见 ADR-0035；单目录包及新增 debug 入口未选择。

### 项目源码位置待决

独立只读复核确认 `prepare_project_unit` 目前以 `{root_identity}/{logical_path}`
作为 SourceMap source name，ADR-0011 又要求 codegen 原样写入 DIFile。
生成 dSYM 本身不保证 LLDB 能定位项目实际磁盘文件。
实施合同必须区分稳定诊断/项目逻辑身份与调试器磁盘来源，验证多 source-root、
Unicode/空格路径及从不同 cwd 启动 LLDB；不得为方便调试悄悄改写诊断协议或名称解析。
需要改变 ADR 的来源模型时先形成相应决策，不在 codegen 中猜测磁盘路径。

用户已选普通 build 新增调试伴随物；Guide 的 CLI 参数形状保持，不新增 debug 参数。现有 native_cli 的首个输出残留断言只计 `.o`，
不能把它误读为禁止所有伴随产物的证据。

### 独立设计复核结论

2026-10-29 独立只读复核确认三项 P1 前置：项目调试源码的物理来源、single/project
统一的本轮产物所有权，以及两路径发布的失败边界。single 当前构建路径的清理不能
不加区分地沿用到新方案；需验证悬空符号链接、已有路径和并发创建，避免清理非本轮文件。

不能用 inode 检查后 remove_dir_all 来宣称安全回滚公开 dSYM：检查与删除之间仍有
外部替换窗口。候选方案为先准备 dSYM、最后 no-clobber 发布 executable；第二步失败时
若无法证明公开路径独占，则返回具体 operational error 并报告遗留位置，而非递归删除。
该例外已由用户选择、ADR-0035 与 Guide 明确规定；实现仍须独立复核与验证。

已向用户呈现三种交付取舍，用户已选默认伴随产物及报告部分发布失败；
期间不写入依赖该决定的生产代码。独立复核未构建或执行 LLDB，不计为运行验收。

## 4. 验证与完成条件

1. 在当前基线增加真实 single/project 失败测试，证明临时对象清理后的源码断点缺口；
   记录红测，随后用同一测试证明修复，零命中不计成功。
2. macOS dwarfdump/LLDB 验证最终产物与匹配调试包；Linux llvm-dwarfdump/LLDB 验证
   ELF。两者均按源码行解析并实际运行命中，核对 Koven frame、源码路径与行号。
3. single/project、多文件、含空格/Unicode 路径、显式 entry 均覆盖；检查既有 executable/
   调试路径、链接失败、调试工具失败、发布失败及清理边界，旧产物字节保持且无旧程序启动。
4. 定向 native_cli/project_cli、受影响 linker/生命周期测试和 CLI build 通过；独立上下文
   复核，文档门禁、PR CI、归档与实际 main CI 闭环。未运行或忽略的 LLDB 不计为通过。

## 5. 非目标与证据

不实现 IDE adapter、DAP、变量/参数/类型显示、表达式求值、优化、交叉编译、源码嵌入、
任意路径映射或商店发布。不增加通用构建配置与依赖。

草案形成时仅有预检结论，尚未实施或运行自动红测；后续实际进展按下文账本记录，基线 macOS 探针见 §1。


## 6. 用户决策补记（2026-10-29）

用户明确选择默认 build 生成 dSYM；失败可保留并明确报告已发布调试包，禁止删除他人产物。
发布合同以 [ADR-0035](../../adr/accepted/0035-final-debug-artifact-publication.md) 和 Guide 的
明确例外为准，§3 的候选/待决描述是此前设计背景，不再表示发布方案仍等待选择。
本 Spec 根据持续授权与已批准发布决定进入实施：项目物理调试来源、统一产物所有权工程方案及双宿主验收尚未完成。


## 7. 自动回归红测

真实 CLI 的 `native_cli::final_debug_artifacts` 两项测试分别覆盖 single/project：
构建后从不同 cwd 按实际绝对源码路径设第 2 行断点，执行 run 并要求停止在该源码行。
2026-10-29 初次自动测试结果为 0 passed / 2 failed / 0 ignored / 34 filtered。
single 实际启动程序但断点 pending；project 夹具错误使用源码根 ["."]，在加载时被拒，
未进入 LLDB，不能计作有效项目断点红测。项目有效红证据为 §1 当前基线的合法 src 根手工探针。
此处纠正原先把两项自动失败均解释为断点 pending 的错误归因。
首轮测试调用 Vec 不匹配既有定长参数 helper，编译失败后已修正，不计作行为红测。
命令：`cargo test --locked -p lang-cli --test native_cli final_debug_artifacts -- --nocapture`，
显式 LLVM 21 路径及单一共享 target，原始日志留仓库外。


## 8. 已复核的工程方案

独立只读复核选择 SourceMap 可选不可变文件名元数据，不克隆 SourceMap、不改变逻辑
source_name。旧 add_source 保持；新 add_source_with_file_name 与其共用注册实现，
source_file_name 受检取得显式文件名或逻辑名 fallback。唯一性及 SourceId 仍只按逻辑身份。
LLVM DebugPlan 继续按逻辑名排序/选文件，仅在创建 DIFile 时按 SourceId 取得文件名；
abort_location 保持逻辑名，防止物理路径泄漏到现有 error()/abort 输出。
CLI 保持加载文本来源，为实际用户源提供绝对 presentation path；标准源保持 fallback。
应覆盖 foreign ID、重复逻辑名、DIFile 选择、逻辑排序、确定性和 abort 文案回归。

macOS 调试包目录发布使用宿主 no-clobber 操作；已在本机 SDK sys/stdio.h 核对
renamex_np 与 RENAME_EXCL（0x00000004）。其 FFI 仅在最小宿主边界，附安全说明及
已有路径/目录/悬空 symlink 测试，不以普通 rename 覆盖空目录。先准备调试包再发布
executable；公开包发布后失败不回滚删除，按 ADR-0035 报告遗留位置。

已有两项红测还需补 single 相对输入、Koven frame 及多文件 helper，不能据当前 entry
绝对路径用例声称完整路径合同已覆盖。工程方案复核不代替上述运行验收。


## 9. SourceMap 首片进度

新增可选不可变宿主文件名，共用原注册路径，逻辑名称、SourceId 与 Span 保持。
`source_span` 10 项通过、零忽略；新契约覆盖显式名称、逻辑身份不变、同一 Span 文本、
重复逻辑名拒绝、foreign ID 拒绝及旧入口 fallback。新 API 缺失时的编译红态先于实现记录。
LLVM/CLI 尚未消费此元数据，不能据此宣称最终断点已修复；两项 native 红测仍待修复。

## 10. DIFile 适配进度

DebugPlan 继续以逻辑名称排序/选文件，仅在预检中受检取得宿主文件名，创建 DIFile 时使用。
resolve_position 与 abort_location 保持逻辑名称。新增三文件物理名/逻辑排序/重复 IR 确定性/
完整 abort 路径测试先失败后通过；独立复核未发现阻塞问题。
`llvm::debug_tests` 常规运行 4 passed / 1 ignored；随后显式 `--ignored` 运行实际 LLDB
命中用例，1 passed / 0 ignored。两次结果合计覆盖五项现有调试测试，不将默认忽略隐瞒为通过。
CLI 尚未提供物理文件名，最终 executable/dSYM 仍待修复；对象级命中不证明本片最终产物完成。

## 11. CLI 文件名接入

single 保留原 source_name，project 保留 root/logical 名称及 descriptor；为已加载文本附加
canonical 物理文件名，标准源注册不变。没有重读文本或新增重试；构建期间源码稳定仍是边界，
不以再解析路径声称并发替换时文本与磁盘必然一致。物理路径解析新增 operational failure，
可先于源码诊断返回；原有成功解析路径的诊断文本/JSON 合同由回归验证，不扩大声称错误优先序。
独立复核未发现新增阻塞；已修正项目物理源解析失败误用“输出路径”文案的问题。
首次 check_cli 5、project_basic_ownership_cli 9、single_file_analysis_cli 5 项均通过、零忽略；
文案修正后的最终回归另行记录。CLI 最终 executable/dSYM 发布仍待实现。

## 12. 单文件发布首片与清理边界

single build 已接入共享私有 staging、dSYM 收集、目录 no-clobber 与最后 executable
hard-link 发布；实际最终产物 LLDB 用例由红转绿，复核修正后再次 1 passed / 0 ignored。
project/run 尚未迁移，不将该首片结果称为整片交付。

并发模型：公开 executable/dSYM 可被其他进程并发创建或替换，CLI 不覆盖或删除它们；
本轮私有 staging 的命名空间与内容由本轮独占。对同 UID 主动篡改私有目录不承诺自动恢复；
检测到目录类型或 dev/ino 已变化时停止清理并报告，保留替换目录。该检查不声称消除
检查与删除间的竞态，也不放宽公开产物保护。

独立复核发现重复 cleanup/Drop、失败清理错误丢失及 JSON 被人类错误替换的问题，
已分别加入 disarm、显式清理聚合与保留原 renderer 的处理；构造 bin 失败也显式清理。
新增回归锁定成功清理后名称复用、已观测 staging 替换及 JSON 原诊断保留。
完整 CLI 单元 63 passed / 0 failed / 0 ignored；发布冲突/工具失败和 project/run
联合测试仍须补齐。最终复核与下游回归尚未完成，原始日志留仓库外。


## 13. Project build 迁移与共享发布回归

project build 已迁移共享 BuildArtifacts，删除旧 inline owner；原三项发布测试迁入共享模块，
按 ADR-0035 检查竞态 executable 字节保留、空 dSYM 目录不被替换、已发布后清理失败报告。
单元测试的占位 executable/空包仅验证发布控制流，不充当真实后端证据。
完整 CLI 单元 63 passed / 0 ignored，三项共享发布测试均实际执行。

自动 LLDB fixture 已改为合法 src 根；single/project 实际构建并命中源码断点，
2 passed / 0 failed / 0 ignored / 34 filtered（4.28 秒）。该绿证据不追认原无效项目红测。
独立复核未发现本迁移切片新的必须修复项，JSON 原渲染及显式失败清理保留。
run 现有私有目录生命周期尚待核查；全量 native/project、故障工具路径、Linux 及 CI 未验收。

## 14. 完整 CLI 回归与 run 清理核查

首次 native 默认并发回归 23 passed / 13 failed，包含既有 10 秒执行限时及新 dSYM 引起的
旧产物数量/快照预期差异；不计通过。没有延长超时。串行探针未超时但命中旧目录数量断言。
成功 build 断言现检查对应 Info.plist、同 basename DWARF 文件及预期目录数量；
递归快照保持递归，仅排除精确最终 executable/调试包，保留包外源码字节与额外文件检测。
独立复核指出两处递归检查降为顶层数量的问题，已恢复并重跑。

最终串行完整 native_cli 36 passed（91.72 秒）、project_cli 39 passed（95.32 秒），
均零忽略。这不证明默认并发首次超时的系统根因，也未追认为并发运行通过。
共享 helper 的 native 专用计数函数已移回 native target，避免 project 的未使用警告。

run 保持原私有对象/程序生命周期，不额外增加 dSYM 功能。核查发现早期编译/链接失败仅依赖
Drop 清理，以及 single 清理失败丢失子进程输出；已改显式清理并保留 renderer、程序输出及
原非零退出码。新增输出/退出码回归后 CLI 单元 65 passed / 0 ignored。
修复后的受影响 run 集成回归 single 3 项、project 4 项通过，均零忽略。信号取消/异常终止另见 §15。


## 15. 外部取消探针与剩余缺口

2026-10-29 在隔离 TMPDIR 使用真实 single/project run，待实际程序子进程出现后，只向
CLI 父进程发送 SIGTERM。两者父进程均以 signal 15 退出；观察到各自子程序仍处于 S 状态，
私有运行目录保留。探针随后定向回收自有子进程，不把终止探针程序当作业务修复。
第一次探针误将项目目录前缀按 single 匹配，未取得完整项目结果；修正识别后重新验证上述两模式。
结果与原始脚本保留仓库外，取消处理仍未实现。

可捕获的 SIGINT/SIGTERM 需要由 CLI 进入正常取消收尾：转交/终止本轮子进程，回收后
显式清理，再报告对应退出状态；不能在 signal handler 中执行 Rust 文件系统/分配/日志操作。
不可捕获 SIGKILL、宿主崩溃无法依赖进程内清理，不将其作为已保证行为。
工程方案须处理 PID 复用、等待/管道排空和旧 handler 恢复，独立复核后再实现。


## 16. libc 直接使用审计

取消收尾需要 sigaction 的完整 mask/flags 保存、进程组信号及平台常量。标准库未提供该完整
边界；现有仓库没有信号处理组件。复用已在 Cargo.lock 中的 libc 0.2.189，checksum
3eaf3ede3fee6db1a4c2ee091bf8a8b4dccdc6d17f656fb07896ee72867612f2；仅增加 CLI 的直接依赖边，
根 manifest 精确固定同版本、关闭该依赖的默认 features，不升级或引入新包。

本机已缓存 manifest/source/license 核验：MSRV 1.65 低于仓库 1.96，MIT OR Apache-2.0；
目标为已有 macOS arm64/Linux x86_64 glibc，平台绑定承担 ABI 差异而非在 CLI 猜结构布局。
普通使用无新增运行时库、过程宏或常驻服务；build.rs 进行 rustc/目标配置探测，审查到的
脚本未下载代码。API 含 unsafe，必须限制在带 SAFETY 说明的信号/进程边界，handler 仅原子操作。
所有权、输出和错误策略仍由 CLI 私有模块负责，不让 libc 类型泄漏至 frontend/codegen。

版本/API 升级跟随独立依赖审阅，本片不追逐最新发行版；已有锁定包被复用，不能据此声称
没有安全风险。已查 RustSec 官方入口与上游发行页面，检索不是完整漏洞扫描；交付仍需核验
既有依赖与安全门禁，未执行的扫描不得记通过。成本为既有绑定复用及局部 unsafe 审计，
避免新增信号框架、手写两平台结构布局和额外原生构建工具链。

## 17. 取消控制首片与 macOS 实测

RunControl 使用 libc sigaction 保存原 mask/flags；handler 只记录原子信号意图。子程序
独立进程组、双管道 drain；500ms grace/重复取消升级前保持 leader 未回收，避免 PID/PGID
复用；取消后即使后代关闭管道，也先完成组终止再回收。只处理仍属于该组的进程，不把
主动 setsid/换组逃逸视为已受控。显式 finish 在私有目录清理后恢复信号状态并报告失败。

macOS 实测确认 zombie-only 组的 killpg 返回 EPERM。系统头 C 探针确认
proc_listpgrppids 返回 PID 数量、proc_pidinfo 无法读取该 zombie；该返回单位也核对了
[Apple libproc 实现](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.c)。
因此仅在两次完整枚举都恰有一个成员且为 leader，并由 waitid(WNOWAIT) 正向证明
leader 已退出时，才消解该 EPERM；任何多成员、查询失败或不完整结果均保留原错误。

模块 6 项通过、零忽略，包含启动前取消不执行、原 bytes/status、TERM 默认143、
忽略 TERM 升级137、leader退出后仍持管道及关闭管道的后代。补测曾遇 EPERM 失败及后续
测试锁中毒，未计通过；修正后六项重新通过。

CLI 接线在参数解析后、编译前安装 guard，覆盖子进程回收及目录 cleanup，随后显式恢复。
已启动程序继续透传真实退出状态；只有启动前取消用 128+signal，不伪造 child ExitStatus。
同一真实 SIGTERM 探针复验 single/project 均 CLI143、child 不存在、私有目录为空。
源码取消测试尚未入库，编译期间取消、完整 mask/flags 恢复和 Linux 仍需后续证据。

## 18. 真实取消回归与信号恢复补测

在 `ee06e3ce` 加本片未提交实现上，新增独立 `run_cancellation` integration target，
为 single/project 各实际构建无限循环程序，确认 native 子进程已启动后向 CLI 分别发送
SIGINT、SIGTERM。验证 CLI 退出码 130/143、native 子进程已回收，以及隔离 TMPDIR
中私有产物清理完毕。macOS 最终串行结果 2 passed / 0 failed / 0 ignored（4.94 秒），
每项内部覆盖两种信号；不是 Linux 结果。

独立复核发现 Linux `ps comm` basename 差异，以及状态断言失败会解除 fixture 守卫导致
遗留程序的问题；已接受 basename/完整路径，并在 Drop 按本 fixture 唯一路径回收程序，
覆盖 readiness 尚未记录 PID、CLI 已退出两种失败路径。查询错误不得当作子进程已消失。

`run_control` 新增系统 sigaction 查询测试：安装带自定义 handler、SIGUSR1/SIGUSR2 mask
及 SA_RESTART/SA_NODEFER 的原状态，finish 后逐项比较真实 handler、flags 和有效 mask。
`cargo test --locked -p lang-cli --bin kovenc run_control -- --test-threads=1`
在 macOS 7 passed / 0 failed / 0 ignored / 65 filtered；不以内部 atomic 模拟代替该恢复核验。
编译/链接期间取消、故障工具路径、Linux 及最终双宿主 CI 仍未验收。

本轮独立复核修正了 fixture 先扫描后终止 CLI 的启动竞态：先 kill/wait CLI，再以
`ps -ww` 按唯一路径进行有界重扫；不将有界测试兜底宣称为任意调度状态的进程管理保证。
最终复核未发现新的必须修复项。CLI all-targets 严格 clippy、workspace fmt、
文档结构检查及 `git diff --check` 通过；首次 clippy/fmt 报告的问题已修正后重跑，
未将初次失败记为通过。

## 19. 链接阶段取消与同步编译边界

受控 `/bin/sh` 链接替身在启动后向 CLI 发送 SIGTERM，再等待两秒正常退出。
初始行为红测实际失败：同步链接返回 `Ok(())`（2.01 秒）；测试可编译后才取得该红证据，
首次共享锁可见性编译错误不算行为红测。后续补强同一场景：晚写 executable 不得发生。

run 的链接阶段现透传已有 RunControl：仍以 `[object, "-o", executable]` 参数数组调用
固定宿主 driver，复用进程组取消、升级、管道排空与回收；build 不安装 run 控制。
新增 LinkerError::Cancelled 保留信号，single/project 均在显式目录清理后返回 128+signal，
最后恢复原信号状态；已经启动的实际程序仍保留自身真实退出状态。
独立复核未发现新的必须修复项；指出的受控路径启动/普通失败盲点已补测，None/Some
两条路径均验证 Launch、Failure 及原输入保留。

进程内 frontend/LLVM 仍同步运行，不承诺任意 CPU 阶段即时或有界取消。
补测在真实编译前记录 SIGTERM，确认 object 实际生成而链接 executable 未生成，
并得到结构化 Cancelled；这证明取消状态穿越编译并阻止下一外部进程，
不等于已覆盖所有编译中间时点、阻塞文件系统或宿主崩溃。

当前输入为 `ee06e3ce` 加本片未提交 diff，macOS LLVM 21 / Rust 1.96，串行结果：

| 选择 | 实际结果 |
|---|---|
| CLI `--bin kovenc`，补测后最终完整单元 | 74 passed / 0 failed / 0 ignored，14.53 秒 |
| `--test native_cli` 完整 | 36 passed / 0 failed / 0 ignored，40.56 秒 |
| `--test project_cli` 完整 | 39 passed / 0 failed / 0 ignored，40.22 秒 |
| `--test run_cancellation` 完整 | 2 passed / 0 failed / 0 ignored，3.22 秒 |
| CLI `clippy --all-targets -- -D warnings`、workspace fmt、docs、diff check | 通过 |

完整 native/project/取消集成在补充受控失败与编译前取消测试前执行，生产接线未再变动；
最终单元及 clippy 包含补充测试。文档入口超行首次失败，收敛摘要后重跑通过。
所有 Cargo 使用 `--locked`，测试指定 `--test-threads=1`；本轮无默认并发通过证据。
故障调试工具路径、Linux、最终双宿主 CI 与 PR 闭环仍未验收，Spec 保持 in-progress。

## 20. 调试工具故障与最终停止 frame

成功返回但只产生 Info.plist/目录、没有 DWARF 的受控工具复现发布缺口：
红测 0 passed / 1 failed / 0 ignored / 74 filtered，原 publish 返回成功。
固定 `/usr/bin/dsymutil` 生产调用现通过私有测试接缝复用；没有新增环境或用户配置。
工具成功后，发布前核验必要目录，以及非空普通 Info.plist 和同 executable basename
的 DWARF 文件。结构检查不证明 plist 语法、UUID 匹配或源码行表，不能替代真实产物验收。

两项故障测试内部覆盖工具缺失、退出 9 并遗留私有部分目录，以及 DWARF 缺失、空文件、
目录、符号链接和错误 basename。失败保留已有 executable 字节与已有 dSYM sentinel，
不发布新的最终路径，显式 cleanup 后 staging 不存在。完整 CLI 单元最终
76 passed / 0 failed / 0 ignored（17.71 秒），独立只读复核未发现阻塞问题。

最终 LLDB 三项覆盖 single 相对 source/output、project 原入口，以及两个 source-root
中不同文件的调用，后者包含中文/空格源码根、文件和输出名。均从不同 cwd 启动 LLDB。
LLDB 的实际选中 frame #0 通过独立查询输出 file.fullpath、line、function，断言 canonical
源路径、行号及 `koven.<qualified-name>.d<数字 declaration ID>` 身份来自同一 frame；
保留实际 breakpoint stop 要求，不以命令回显、basename 或断点解析描述代替停止位置。
该函数名格式对齐已有 SSA Unit 命名测试，不提升为语言语义。

新增多根夹具曾因非固定 project.toml 名称被加载器拒绝，修正后才运行调试器；
收紧 frame 断言时裸源码名假设也曾导致 3 项失败，核对实际函数名和既有命名合同后修正。
这些不是生产调试缺陷，也不计通过。最终同一 frame 查询测试
3 passed / 0 failed / 0 ignored / 34 filtered（7.63 秒）。独立复核确认断言问题闭合。

输入仍为 `ee06e3ce` 加本片未提交 diff，以上结果均为 macOS；CLI all-targets 严格 clippy
最终通过，首次 helper 多余借用告警修正后重跑。Linux、最终双宿主 CI、归档及 PR/main
交付闭环仍未验收，不将本地必要结构检查当作跨宿主完成证据。

## 21. 提交前复核与 CI 前置

最终独立整片复核覆盖 tracked/untracked 变更，未发现新增生产阻塞问题；本地 diff 可送
Draft PR，但不等于 Spec 完成。SourceMap/DIFile、staging/no-clobber、取消回收及同一停止
frame 均已核查。Rust 尺寸门禁相对 main 通过：1994 个手写 Rust 文件，39 处历史超限
继续报告，没有新增阈值豁免。workspace all-targets check 通过（32.92 秒）；未执行
frontend 全量测试。文档 checker 自测 41 项通过。

复核指出 LLDB 未显式安装/核验，以及旧 dwarfdump 只看 object 的送 CI 缺口，现已补齐：
Linux 安装官方 Noble LLVM 21.1.8 同一精确版本的 lldb-21、liblldb-21、python3-lldb-21。
已从官方 apt.llvm.org Noble Packages 索引只读核对三个包的版本及绑定依赖；签名源、
原固定版本和既有 liblzma 校验保持。setup-llvm 为 Linux 选择该 prefix 的 bin，macOS
使用系统 /usr/bin/lldb；实际执行 SBFileSpec.fullpath / SBFrame.GetFunctionName 查询前置，
缺工具或 Python API 时立即失败。该配置存在不代表 Linux 已安装或通过。

最终产物测试在 LLDB 前直接 dwarfdump macOS dSYM 或 Linux ELF，后者另核 ELF magic；
要求真实地址行包含预期行号，再由实际停止 frame 核验精确来源。初次把完整路径当作 dump
中的连续文本导致三项失败；实际工具会拆分目录/文件名并以八进制转义 Unicode，修正测试
假设后 macOS 3 passed / 0 failed / 0 ignored / 34 filtered（5.77 秒）。未减少实际 frame
路径核验。新增前置及测试经独立复核，无 must-fix。

LLVM 安装边界自测 4 项、preview setup 2 项、preview dependencies 11 项通过；后者首次
因精确包列表未同步失败，更新为上述固定包后重跑通过。脚本 bash -n、CLI all-targets
严格 clippy 通过。最终输入提交身份及 Linux 证据由随后 PR/CI 记录，不以未提交树的本地
结果代替实际 CI。Spec 仍保持 in-progress。

## 22. 首次 PR CI 与 Linux clippy 修正

Draft [PR #228](https://github.com/Halckon/Koven/pull/228) 输入 head
`fad9d7cf9ad62a934e4136a30d3322fd1a6a3fc5`，base
`0e1c33c58b72d0ec33fcc191a9752ea39dc8973a`；实际 CI merge 输入
`d133a652edb3a8906283c36bb0b79fc467b546eb`。
[首次 run 40883508294](https://github.com/Halckon/Koven/actions/runs/40883508294)
失败，不计通过。依赖、尺寸、fmt、docs 及 macOS workspace check/clippy 通过；
Linux 安装精确 LLDB/Python 绑定并实际得到 SB API 成功 marker，workspace check 通过
（37.17 秒），随后严格 clippy 在 build_artifacts 的条件编译错误传播处报 question_mark。
下游测试/preview 被门禁阻止，不能当作已执行；未重试同一失败输入。

将 hard_link 的结果先保存，仅在 macOS 添加原部分发布上下文，再共同用 `?` 传播。
发布顺序、原 ErrorKind、dSYM 保留位置及成功后 cleanup 均不变；独立只读复核确认等价。
修正后本地共享发布 7 passed / 0 failed / 0 ignored / 69 filtered（7.90 秒），
CLI all-targets 严格 clippy 通过。Linux 修正后实际结果等待新提交 CI，不凭 macOS 推断。

在上述固定 head 上另跑 source_span：10 passed / 0 ignored；codegen debug 常规
4 passed / 1 ignored，随后显式 `--ignored` 的真实 LLDB 项 1 passed / 0 ignored，
分别保留选择范围，不把默认忽略隐瞒为已执行。Spec 仍待双宿主完整门禁及交付闭环。

## 23. Linux 实际断点命中与 frame 名称断言修正

[run 40884417348](https://github.com/Halckon/Koven/actions/runs/40884417348) 的双宿主
workspace check/clippy 均通过。Linux targeted job
[123487885715](https://github.com/Halckon/Koven/actions/runs/40884417348/job/123487885715)
在 CLI native target 失败：34 passed / 3 failed / 0 ignored（3.96 秒）。三项新的调试用例
实际均命中 breakpoint，选中 frame 的 canonical 源路径与行号正确，最终 ELF 行表检查
亦已通过；仅函数名断言错误地只接受 macOS 展示的 DI 名称。
Linux LLDB 返回 `f0.koven.main.d13` / `f0.koven.app.show.d13`，与 LLVM adapter 的
`f<FunctionId>.<function.name>` linkage 名称一致，不是生产来源映射或断点失败。

仅修正测试接受源码 DI 名称及单层 `f<非空数字>.` linkage 前缀；qualified 函数名和
`.d<非空数字>` 仍精确匹配。新增反例拒绝其他函数、相似名、重复前缀和损坏 ID，
未退回 substring 判断。相同实际 frame 的完整路径/行号和真实 breakpoint stop 保持。
独立只读复核确认强身份检查未被弱化，生产代码不变。macOS 定向目标
4 passed / 0 failed / 0 ignored / 34 filtered（7.17 秒）：其中 3 项真实 LLDB、1 项名称
正反例；CLI all-targets 严格 clippy 通过。修正后 Linux 结果等待新提交 CI。

该 Linux 失败输入上，codegen 2477 项、CLI 单元 74 项、project_cli 39 项和真实取消
integration 2 项实际通过，均零忽略；这不表示完整组合通过。主步骤失败导致后续证据
目录未生成，artifact 上传错误为下游缺材料，未将其当作独立业务故障。macOS 仍在运行时
不提前报告其通过；该轮失败不能以局部通过追认为整轮通过。

## 24. 合法夹具的事后基线核验

为补核早期 project 自动红测夹具无效的边界，等待 CI 时在仓库外使用 `git archive`
原基线 `48adcfab59e3d32be305e0f9a6d1127c157bbe71` 建立隔离快照与独立 Cargo target。
复制固定 head `3b1a5bba3e940649141ab464d5091e78d6d187a9` 的最终调试测试主体，
仅用等价 standalone 目录/run helper 替代父模块导入。三个合法夹具均先构建成功，
随后因最终 dSYM 不存在失败：0 passed / 3 failed / 0 ignored（2.29 秒）。

第二个基线探针只移除已知失败的最终行表前置调用，独立执行相同 LLDB 查询与断言，
single/project/多根三项均实际报告 `Breakpoint 1: no locations (pending)`，程序结束后
没有停止 frame；0 passed / 3 failed / 0 ignored（6.62 秒）。没有修改实际分支或基线
编译器代码。此为实施后的缺口佐证，不追认为实施前红测、Linux 证据或当前实现结果。
当前正向命中继续由前述最终产物测试与随后真实 CI 验证。

## 25. 首个 Linux 完整 targeted 成功与 macOS 循环夹具修复

head `b379d43673f8562abdac33d5dc79dd96957069ba` 的
[run 40887166831](https://github.com/Halckon/Koven/actions/runs/40887166831) 的 base 为
`72b0414e1f4cc2d9f7dc251c96947bfc18bd6ce3`，实际 CI merge 为
`3aec9e7addedb1206f90e6dd849c34f2b1426e93`。Linux
[job 123498005904](https://github.com/Halckon/Koven/actions/runs/40887166831/job/123498005904)
完整成功。日志实际命中新名称正反例及三项最终 ELF 行表/LLDB 用例；native_cli
38 passed / 0 failed / 0 ignored（6.02 秒）、project_cli 39 passed / 0 ignored（7.53 秒）、
run_cancellation 2 passed / 0 ignored（1.72 秒），CLI 单元 74 passed / 0 ignored，
codegen 2477 passed / 0 ignored。词频、conformance、Linux sanitizer、生成资源程序
及各自证据上传步骤均成功，不仅是核心步骤结束。

macOS [job 123498005782](https://github.com/Halckon/Koven/actions/runs/40887166831/job/123498005782)
失败在 single_file_analysis_cli 的成功循环夹具：它复用同一输出路径，仅移除 executable，
未移除本轮新增 program.dSYM，下一次 build 正确地按 no-clobber 返回错误。其余 CLI
targets 通过，包括单元 76 项、native 38 项（5.95 秒）、project 39 项（5.39 秒）、取消
integration 2 项。codegen 为 2476 passed / 1 ignored，现存默认 ignore 不隐瞒；对应实际
LLDB 的显式本机证据见 §22。该输入的 macOS 完整组合及 preview 不计通过。

仅在该测试成功构建、执行与原断言之后，复用输出前移除 fixture 独占的 program.dSYM；
Linux 路径、生产保护及 human/JSON/entry/stdout/stderr oracle 均不变。搜索 CLI 所有
remove_file 调用，未发现另一处只删除构建 executable 后复用输出的同类位置；其余删除
源码或整体 fixture。独立只读复核确认无弱化和误清理风险。修正后本机完整
single_file_analysis_cli 5 passed / 0 failed / 0 ignored（16.48 秒），CLI all-targets
严格 clippy 通过。修正后双宿主结果仍待新提交 CI，不能用原 Linux 成功追认新输入。
