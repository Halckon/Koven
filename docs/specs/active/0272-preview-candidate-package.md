# SPEC-0272: 双宿主候选包与正常安装验收

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施或验收 M5b 候选包首片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-272` |
| 所属 Phase | Phase 6 工程交付；消费现有 native/runtime |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[运行时](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户持续实施里程碑、满足前置可并行及按实际调整草稿的授权 |
| 前置 Spec | SPEC-0266、SPEC-0268 |
| 前置 ADR | ADR-0007、ADR-0010 |
| 影响范围 | Python 打包/安装验收工具、教程复用、CI、交付说明 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

在 macOS arm64 和 Linux x86_64 glibc 上，从同一提交生成可追溯的 release tar 候选包；
独立 runner 仅消费包和其依赖说明，正常安装后实际编译运行包内的三文件参数报告。
基线为 main `80b1c3ba5ae3dbce9e6bae7000c0a235e2d2f24d`，分支 `feature/spec-0272-current`。

本片承接 [M5 起草材料](../../development/measurement-distribution-spec-draft.md) D03–D05、
候选阶段 D07 及已有 M4a 前置。M4b、M1B、性能测量不是本片前置。
D06 缺失依赖负向安装、浏览器下载隔离与签名体验保持开放；本片不关闭整个 M5b。

## 2. 候选内容与使用边界

- `bin/kovenc`：`cargo build --locked --release -p lang-cli --bin kovenc` 的真实产物；
  包身份包含源码 SHA、实际目标及 profile，不仅使用 workspace `0.1.0`。
- `examples/parameter-report/`：由教程唯一真源提取的三份源码、`project.toml`、四组 argv/oracle；
  不另写一套预期输出，不需要 checkout 内的标准库源码。
- 安装、依赖和限制说明；实际消费端使用的依赖准备及正常 smoke 工具；本项目和实际包内
  组件的许可文本/NOTICE、来源与版本清单；manifest 与逐文件 SHA256。
- tar 外另附 SHA256；包中不包含 Cargo target、开发 checkout、构建缓存或未声明动态库。

LLVM 21 选择外部安装，保留本机 C driver/SDK 依赖：Linux `/usr/bin/cc`，macOS `/usr/bin/clang`。
生产 job 可复用现有 setup-llvm action；消费 job 无 checkout，必须按包内说明/工具准备依赖。
不得偷偷 checkout 来取得本地 action，不得改变生产链接方式以绕过未确认的依赖。

`llvm21-1-prefer-dynamic` 不保证实际动态链接。必须检查两宿主 release 产物的加载依赖，
确认符合外部 LLVM 边界；静态包含或加载闭包不明时停止候选验收并记录，不从 debug 推断。
保存 macOS `otool -L` 与加载命令、Linux ELF NEEDED/RPATH/解释器及解析结果；
拒绝未声明的构建目录依赖。记录实测 OS、arch、glibc/SDK 和工具版本，不外推最低兼容版本。

许可清单基于实际 CLI 构建依赖而非整个 workspace 的 Cargo.lock；Rust 标准库和 native
组成另行核实。根许可证不能代替第三方材料，外部安装 LLVM 与随包组件分开列出。
来源/材料缺失时不生成可验收候选；不引入新的运行库或许可生成依赖。

## 3. 独立安装验收

消费 job 为两个独立 GitHub runner，不 checkout、不 Cargo 构建、不恢复编译缓存。
记录预装工具清单；这是有预装软件的新 runner，不宣称空白操作系统。

1. 仅下载当前运行对应宿主候选 artifact，核对归档校验和和 manifest 文件清单。
2. 按包内说明准备明确的外部依赖，在含中文和空格的独立目录解压与创建项目目录。
   保存消费端实际动态库解析路径及版本、C driver/SDK 信息，并核对 manifest 的声明约束；
   不得依靠未声明的 runner 预装依赖。无法确认实际加载闭包时 P3 不通过。
3. 四组 argv 为 `[]`、`["alpha"]`、`["alpha", "你好", "tail"]`、`[""]`。
   每组实际 CLI build、直接运行产物、CLI run，共 12 条命令，逐项比较 exit/stdout/stderr bytes。
4. 保存实际 argv、cwd、必要环境、输出、退出码、耗时及失败原因；缺项/零运行/工具缺失均失败。
5. 移除仅本次拥有的安装与项目目录，确认同级哨兵文件内容未变，保留独立证据目录。

执行过程有明确超时和独立临时目录；失败保留原始记录，不自动改输出或下载别的候选替代。
移除只针对自行创建并记录的路径，不能从输入字符串拼出通用递归清理目标。

## 4. CI 与复用

打包阶段复用 `scripts/check_tutorial.py::load_examples`。正常运行逻辑优先从现有
`check` 提取“已加载合同与项目目录”边界，保留旧教程调用行为，避免复制语义和 oracle。
标准库与 runtime 交付继续遵守现有 CLI/codegen 合同，不修改语言规则、关键字或 ABI。

现有 CI 增加双宿主生产和双宿主独立消费 job；消费 job 只依赖对应候选产物与已通过的前置。
代码、lockfile、工具链、打包/安装脚本、教程、许可材料和工作流变更均须触发所需 job。
`scripts/check_ci_results.py` 汇总新必需结果，失败、取消、缺失、意外 skip 不能成为绿灯。
矩阵政策逐个 job 核验，不能把原固定矩阵数简单放宽。成功和失败都上传可用证据。

M4a 原有用例和现有双宿主门禁保持必需；以当前交付提交的真实 CI 通过为前置证据。
不新增故障校准，不将正常安装成功升级为额外内存安全保证。

## 5. 非目标

公开 Release/tag、包管理器上架、自动更新、LLVM 捆绑/重定位、CLI 版本 API、跨编译、
Windows/musl、性能排名、全量 M1B/M4b、最低系统兼容性、签名/公证与浏览器隔离属性均不在本片。
CI artifact 是可审阅候选交付，不代称面向所有用户的正式发行版。

## 6. 验收与实施顺序

| ID | 必须完成的证据 | 状态 |
|---|---|---|
| P1 | 合同与实际入口独立审阅；依赖/许可策略符合真实 release 产物 | 待实施 |
| P2 | 两宿主 release tar、源码/tree/lock/toolchain/命令、依赖清单、材料及全部哈希 | 待实施 |
| P3 | 两个独立消费 runner 各 12 命令通过，含中文/空格路径、空字符串参数 | 待实施 |
| P4 | 精确移除与哨兵保留，成功/失败记录与缺项拒绝测试 | 待实施 |
| P5 | 原教程行为及四组 oracle 保持，新增 CI 门禁拒绝缺失/取消/错误 skip | 待实施 |
| P6 | 全部现有必需 CI、已选 M4a 通过，独立审查、文档/Architecture 与归档 PR 闭环 | 待实施 |

先完成许可/加载闭包与包清单，再最小打包/正常消费工具，再 CI 接线。行为改动有失败测试，
Python 使用现有标准库和 unittest；不为纯文案新增测试。本地 Cargo 只占一个串行窗口。
任何未知产物属性都保留为待验证，不把本机 debug 预检记为 release 或新 runner 验收。

一个提交一个逻辑边界，信息含 SPEC-0272；只有全部所选验收完成才归档，
最终归档提交 CI 通过后合并。M5 草稿只保留链接与未承接范围，不维护第二份实施账本。

## 7. 起草与验证记录

2026-10-04 独立预审确认 M1A/M4a 已满足启动前置，指出实际 release 加载闭包和第三方
材料为首要待证事实，D06 安装负向与宿主信任体验不能由本片关闭。该意见已写入本合同。
本机先前 debug CLI 在中文/空格目录完成教程 12 命令，仅作为可行性预检。
正式合同独立复审补充了消费端实际加载闭包与 manifest 的核对，已纳入 §3。
P1 的合同部分已审阅，release 依赖与材料仍待证；P2–P6 尚未执行。尚未生成候选 tar，未发布 Release/tag。

2026-10-04 合同在 PR49/PR50 合并后从最新 main 迁入独立 worktree。原合同提交 `f87f031` 保留。
本机 release 只读调查确认外部 LLVM/zstd 与 CLI 自身的最低 macOS 标记不同，
不得据此宣称 macOS 14 兼容；须由目标 CI 宿主构建并验证真实加载闭包。
许可材料调查识别 15 个目标 Rust 第三方库及构建工具、Rust 标准库材料；尚未生成正式候选。
P1 合同已独立审阅，双宿主产物与材料仍待完成。

## 8. 实施检查点

2026-10-05 已实现最小包生产、实际宿主检查和独立消费工具，CI 增加明确的四个必需 job。
教程共享 `run_case` 保留默认行为，本机现有 debug CLI 实际执行23项合同、1项线程planned；
这不是 release 候选或独立 runner 验收。

独立完整审查分别覆盖宿主、消费端、生产端和 CI。已修复实际 dyld 核验、canonical 搜索路径、
尾零版本比较、宿主失败原始证据、目标与宿主一致性、哨兵异常导致证据丢失、部分重跑产物身份、
生产失败原因上传及发布前消费合同预检。生产二进制的私人路径通过受控 Rust remap 和原样扫描拒绝，
不对生成二进制直接改写；真实标准库元数据的 remap 效果仍须 release 构建证明。

原始 release 调查与只读材料审查能证明当前安装中有实际许可文件，不能代替新候选来源和双宿主验收。
P1 尚待真实最终 release 的闭包/材料核验；P2–P6 尚未取得本片双宿主证据，Spec 保持 in-progress。
