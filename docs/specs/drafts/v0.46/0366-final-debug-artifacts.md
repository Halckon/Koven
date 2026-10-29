# SPEC-0366: CLI 最终调试产物与源码断点

> **性质**：CLI 产物合同草案 · **状态**：draft · **读取时机**：交付最终 executable 的源码调试能力时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P6-0366` |
| 所属 Phase | Phase 6 CLI；复用 Phase 4 调试行表 |
| 语言规范 | [Guide v0.46](../../../guide/README.md) |
| 授权范围 | 持续推进编辑器里程碑，补齐消费方 SPEC-0011 的 debug build 前置；草案不授权未审结方案实施 |
| 前置合同 | [ADR-0010](../../../adr/accepted/0010-first-native-object-and-linker-contract.md)、[ADR-0011](../../../adr/accepted/0011-first-dwarf-line-mapping.md)、[ADR-0026](../../../adr/accepted/0026-linux-x86-64-native-host.md) |
| 阻塞项 | macOS 多产物发布/清理及项目源码路径合同待审结；当前 main 的失败复现及双宿主验证待执行 |
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

### 项目源码位置待决

独立只读复核确认 `prepare_project_unit` 目前以 `{root_identity}/{logical_path}`
作为 SourceMap source name，ADR-0011 又要求 codegen 原样写入 DIFile。
生成 dSYM 本身不保证 LLDB 能定位项目实际磁盘文件。
实施合同必须区分稳定诊断/项目逻辑身份与调试器磁盘来源，验证多 source-root、
Unicode/空格路径及从不同 cwd 启动 LLDB；不得为方便调试悄悄改写诊断协议或名称解析。
需要改变 ADR 的来源模型时先形成相应决策，不在 codegen 中猜测磁盘路径。

普通 build 新增调试伴随物与显式 debug 入口两种方案尚待审结；Guide 已固定 CLI 参数形状，
不能仅以实现配置为由增加参数。现有 native_cli 的首个输出残留断言只计 `.o`，
不能把它误读为禁止所有伴随产物的证据。

### 独立设计复核结论

2026-10-29 独立只读复核确认三项 P1 前置：项目调试源码的物理来源、single/project
统一的本轮产物所有权，以及两路径发布的失败边界。single 当前构建路径的清理不能
不加区分地沿用到新方案；需验证悬空符号链接、已有路径和并发创建，避免清理非本轮文件。

不能用 inode 检查后 remove_dir_all 来宣称安全回滚公开 dSYM：检查与删除之间仍有
外部替换窗口。候选方案为先准备 dSYM、最后 no-clobber 发布 executable；第二步失败时
若无法证明公开路径独占，则返回具体 operational error 并报告遗留位置，而非递归删除。
该例外尚未批准，现行失败原子性仍以 Guide 为准。若采用该方案，须新增窄范围 ADR
并明确启用对应 Guide 调整后实施，不在本草案中自行豁免。

已向用户呈现默认伴随产物、单目录包及显式 debug 入口的取舍，等待架构决策；
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

当前只形成草案与已有预检结论；本片生产代码、当前基线红测、Linux 和 PR CI 均未运行。
