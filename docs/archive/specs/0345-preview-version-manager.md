# SPEC-0345: 本地 preview 版本管理

> **性质**：交付合同 · **状态**：done · **读取时机**：实现或验收本地版本管理时 · **唯一真源**：本次交付范围与验收

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-0345` |
| 所属 Phase | Phase 6 |
| 语言规范 | [当前 Guide](../../guide/README.md)；不依赖 v0.46 |
| 批准依据 | 用户授权持续实施生态路线图，并要求跳过由另一 agent 实施的 v0.46 |
| 前置 Spec | SPEC-0272、SPEC-0322 |
| 影响范围 | preview 管理脚本、包、协议测试与安装 CI |
| 语言语义变更 | 否 |

## 1. Goal

交付 M1-S8 的本地包管理首片：用户指定独占安装根目录，安装已下载的 preview 包，
查询精确源码提交与 target，显式切换版本、停用及卸载；校验或安装失败不改变当前版本。

## 2. 范围

复用现有 preview checksum／manifest／宿主核验和十二条真实示例命令；成功后才发布版本。
版本 ID 为完整源码提交加 target，同 ID 不覆盖；切换以原子 current 符号链接替换完成。
安装不自动切换；用户将 `<root>/current/bin` 加入 PATH。操作采用独占锁，冲突立即失败。
只管理新建或带本工具标记的专用目录，拒绝路径逃逸、符号链接版本和未知目录。
卸载当前版本前必须显式停用或切换；失败不删除旧版本和无关同级文件。
包和 checksum 须来自用户信任的同一发布输入；校验不构成签名或发布者认证。

## 3. 非目标

不自动下载、不修改 shell 配置、不安装系统依赖、不提供包管理器上架或远程版本解析。
不修改 Guide、编译器或其他 agent 的 v0.46 分支；不声称三个未来示范程序已完成安装验收。
本片不承诺对同一 OS 用户恶意并发修改目录的安全隔离，也不承诺断电持久化。

## 4. 验收

- [x] 新目录安装、精确身份查询、两个版本切换、停用、卸载正常路径。
- [x] 损坏包、宿主／真实示例失败、同 ID、未知根、符号链接、非法 ID、锁冲突拒绝；当前版本和同级哨兵保全。
- [x] 发布或切换失败保持旧 current；已安装文件损坏时拒绝切换。
- [x] Python 合同测试、文档门禁与 diff 检查；独立上下文审查具体边界。
- [x] 双宿主真实 preview 包管理流程与现有消费验收；PR CI 无未决状态后归档。

## 5. 验证记录

基线 `3d23b45cb03a0c052f036692a9c51970ce243cb0`，未改 Rust／Guide。首次管理器测试9项因入口缺失失败；
实现后定向 `python3 -m unittest scripts.tests.test_preview_manager scripts.tests.test_preview_install scripts.tests.test_preview_package -q`
52项通过，无跳过（管理器14项）。完整 scripts/tests 门禁共409项，408通过、1项Linux `/proc`进程组检查在macOS按条件跳过；
期间追加的管理器回归由上述最终定向运行覆盖。文档742页及 diff whitespace通过。
独立审查发现并修复未校验pyc执行、切换后清理误报、安装后清理误报；具体负例已入测试并复审闭合。
macOS arm64 本机真实验收（2026-10-22）：输入 `f1846790fb7e7f9e0b30a7835638a7e27384bd99`，
Rust 1.96.0、LLVM 21.1.8；`scripts/package_preview.py` 在干净提交上成功生成真实 release 包。
包内管理器 install 复用消费合同，十二条命令全部通过、消费临时目录清理成功；随后 use／list、
损坏 checksum 拒绝及 current 不变检查通过，经 `current/bin/kovenc` 复用该包 `run_case` 的
四组 argv 再执行十二条 build／直接运行／run，均逐字节匹配包内合同；deactivate／uninstall 后
current 消失且版本列表为空。原始产物与结果保留在仓库外，本条不替代双宿主 CI。
[实现 CI 39795369166](https://github.com/Halckon/Koven/actions/runs/39795369166) 的双宿主测试、
双宿主候选生产已通过；Linux独立消费在管理步骤之前失败，真实命令0条，不能记为通过。
[消费证据](https://github.com/Halckon/Koven/actions/runs/39795369166/artifacts/12434734543)
显示加载闭包仅 liblzma.so.5 的SHA256不同，包／依赖准备成功、路径相同；未推断两个发行包版本。
共享Ubuntu准备脚本现显式选择已存在的LLVM传递运行库liblzma5，避免仅满足传递依赖而沿用runner预装构建；
不新增二进制/Cargo依赖、不变更发行源或安装信任规则，继续原严格实际哈希比较，不建立新ABI例外。
准备策略失败测试已复现未选择该运行库；修复后63项相关测试、shell语法与文档检查通过，独立审查无阻断项。
该运行macOS独立消费被取消，不能记为通过；修复后的验收见下。
M1-S8三个示范程序最终联合验收仍在后继切片。

## 6. 双宿主交付验收

源码输入 `de1babd12279fa47f554b99be0edebd2c8865614`，
[PR CI 39804644182](https://github.com/Halckon/Koven/actions/runs/39804644182) 于 2026-10-23（Asia/Singapore）全部通过。
macOS arm64 与 Ubuntu x86_64 的 Check/Clippy、定向测试、preview 生产及独立消费均实际执行成功。
独立消费与管理器 install 各自运行十二条真实命令；use/list 身份匹配，损坏 checksum 拒绝且
current 保持不变，通过 current/bin/kovenc 构建并运行参数示例并核对输出，最后停用及卸载成功。
Linux 严格加载闭包核验通过，未引入 liblzma 哈希或 ABI 豁免。
原始消费证据：[Linux](https://github.com/Halckon/Koven/actions/runs/39804644182/artifacts/12440200658)、
[macOS](https://github.com/Halckon/Koven/actions/runs/39804644182/artifacts/12439386839)。
归档提交仅修改文档和生成索引；合并仍须该提交的最终 PR 门禁无未决状态。

## 7. 主干集成复验

归档后主干合入 `5b844c374257d41dde94592ea60aadd34ae77045`，共享 LLVM 准备脚本冲突。
集成采用主干 `85f0dfc9` 的固定 liblzma 版本、包及加载文件 SHA256 核验，保留拒绝降级与来源检查；
本分支的准备策略回归同步断言精确包参数，不退回不固定版本的安装。实现验收仍为上节原输入，
合并后的双宿主 CI 另行复验，尚不能由原输入成功推断新组合通过。
