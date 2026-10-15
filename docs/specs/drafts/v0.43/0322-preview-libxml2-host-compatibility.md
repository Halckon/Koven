# SPEC-0322: Preview 外部 libxml2 有界宿主兼容性

> **性质**：候选工程合同 · **状态**：draft · **读取时机**：审阅有界宿主预检策略时 · **唯一真源**：本页；现行行为仍由脚本与交付说明定义

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P6-322` |
| 所属 Phase | Phase 6 工程交付；不改变语言/native ABI |
| 语言规范 | [Guide v0.43](../../../guide/README.md) |
| 批准依据 | 用户 2026-10-15 同意仅 libxml2 有界兼容方向；本详细合同须先独立审阅 |
| 前置 Spec | SPEC-0272 |
| 前置 ADR | 无新增 |
| 阻塞项 | §9 的真实包来源和符号提取可行性待证；不得先改生产行为 |
| 影响范围 | `scripts/preview_host.py`、对应 Python 协议测试、preview 文档/CI 证据 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

允许 Ubuntu 24.04 x86_64 的外部 `libxml2.so.2` 在满足下述完整前置时存在不同 SHA256，
同时保留包完整性与其他外部依赖原合同，并以无 checkout/Cargo 的独立 consumer 实际执行
完整 12 命令及清理/哨兵作为成功必要条件。预检通过不是安装验收通过，也不是通用 ABI 保证。

基线为官方 main `7c348e704ecd3595fa17ce4b08a2734d7d966e80`，独立分支 `fix/spec-0322`。
2026-10-15 官方 branch 全量分页最大号为 0321；main 全目录 Spec 最大号 0320，0321 在独立 PR105。
本片不引入或修改 0321 的生产代码、测试、验收和状态。

## 2. 已知事实与不得推断的事实

main CI 38656839732 attempts 1/2 与 PR105 CI 38659703874 attempt 1 的 Ubuntu consumer
在宿主预检拒绝，安装命令 `commands=[]`。已比对的 17 个库中仅同路径 libxml2 SHA 不同；
新旧 runner 方向可反转。这证明测得文件字节不同，不证明实际 Debian 修订或 ABI 兼容。
不把已见两个 hash 列为兼容白名单，不从文件名中的版本推断包版本。

历史 SPEC-0272 规定包完整性、外部加载闭包声明和 12 条真实命令；当前 `compare_hosts`
实现对所有外部库 `(name, resolved, sha256)` 全等，是更强的现行规则。本片显式改变唯一
libxml2 项的宿主策略，并同步当前文档；历史合同和失败记录不改写。

## 3. 不变合同

- tar SHA256、逐包内文件 SHA256、manifest/schema/source/tree/lock/target/profile、bootstrap 同源、
  tar 成员集合/类型/权限、许可与源码身份全部保留；不删除字段或忽略校验。
- 外部库名称与解析后路径集合仍严格相同。除唯一 `libxml2.so.2` 外，每项 SHA256 仍全等；
  不按固定 17 项硬编码集合，现有 16 个其他项仅是历史实测数量。
- LLVM version/prefix、C driver、glibc、SDK/path 与平台规则、loader 环境覆盖拒绝规则不放宽。
- macOS 不使用本策略。没有 libxml2、重复记录、字段不完整或不能识别的证据不得打开兼容例外。
- 独立 consumer 不 checkout、不 Cargo、不恢复编译缓存；四组 argv 各 build/产物/run，逐条
  exit/stdout/stderr 字节校验与拥有目录清理/同级哨兵仍必需，任一失败不得报 success。

## 4. 第一片：仅 CI 真实只读取证（先独审、后实现）

本片第一阶段不启用兼容例外，不修改 `compare_hosts`、host 返回值、原 success/status、
包成员/校验、12 命令或清理路径。只在 CI 两个 Ubuntu preview 执行 step 设置
`KOVEN_PREVIEW_LIBXML2_EVIDENCE=1`，macOS 和普通本地调用不启用。

- 在现有 Linux `inspect_host` 成功取得真实 libraries/glibc 后、返回 host 前采证；
  生产端原 `finally` 写 `host-inspection.json`，消费端原 `finally` 写 results.json，
  两个 job 原有 always artifact 步骤保留。后续 compare 拒绝仍先落独立诊断证据。
  若原本 inspect_host 更早失败，本小片不改变原失败，不宣称额外证据已收集。
- 精确输入仅为实际 libraries 中唯一 `libxml2.so.2` 的 resolved path，以及实际 binary
  和闭包路径。对所有已解析文件只读 `readelf --wide -d`；只对实际 DT_NEEDED libxml2
  的直接引用者及 libxml2 本身追加 `-h`、`--dyn-syms`、`--version-info`。
- 包管理器命令固定为 `dpkg-query -S <实际libxml2 resolved>`，
  `dpkg-query -W -f=<binary:Package/Status/Architecture/Version/source:Package/source:Version> libxml2:amd64`，
  `dpkg-query --control-show libxml2:amd64 md5sums`，
  `apt-cache policy libxml2:amd64` 与 `apt-cache policy`。不安装、不更新索引、不调配置或权限。
  md5sums 此时仅采证，未执行发行包文件一致性判断；将来 MD5 不作为安全证明。
- 每命令固定 `LC_ALL=C`、stdin DEVNULL、60 秒超时、每路 stdout/stderr 32 MiB 上限；
  达限杀子进程并标记 output_limited，超时标记 timed_out，留已有有界输出。
  原始 argv/exit、输出字节（base64）、elapsed 与错误留 evidence，不写完整环境。
- apt 输出在保存前逐 URI 审查：仅保留无 userinfo/query/fragment、路径严格为 `/ubuntu`
  或 `/ubuntu/` 的官方 archive.ubuntu.com / azure.archive.ubuntu.com / security.ubuntu.com
  URI；其余 URI 整项替换为明确 redacted 标记。原始未脱敏 apt 输出不落盘；记录 redacted
  状态。被脱敏的来源不可充当来源已验证事实。其他命令输出不涉及配置源凭据。
- 诊断独立 `schema=koven.preview.libxml2-diagnostic.v1`，记录目标、直接引用者和 commands/
  errors。任一采集失败只把该诊断标为 incomplete；不覆盖原有 host 结果，不修复现有失败。
  不新增包内文件或第三方依赖，helper 留 `preview_host.py`，不扩张 TOOLS。

第一片允许提交的最小范围：本合同/索引、preview_host 的纯诊断 helper、两个 Ubuntu step
的显式开关、CI-only 协议测试。先由独审确认合同，再实现并送 diff 独审，再由唯一 publisher
发 Draft PR 触发现有完整 CI。此次 CI 红可以是原严格 hash 门禁，不能把它记为安装通过。
云端仅语法/静态检查；所有 mock/fault 测试、真实 Ubuntu 探针都只在 CI 执行。

## 5. 拟定后续最小宿主策略（未批准实施）

仅对两侧 `system=Linux`、`arch=x86_64`、Ubuntu `ID=ubuntu / VERSION_ID=24.04` 且同一
name/resolved 的 `libxml2.so.2` 考虑不同 hash。两侧记录均须由真实工具采集：

1. ELF64、小端、EM_X86_64，唯一真实 DT_SONAME 为 `libxml2.so.2`；读取该文件的
   DT_NEEDED、RPATH/RUNPATH，不允许该例外新增外部路径或闭包。
2. `dpkg-query` 对实际 resolved 文件给出唯一安装包 owner，安装状态完整，包名 libxml2、
   架构 amd64、source package libxml2，保存完整 binary/source version；不能仅核名称。
3. 精确 installed version 必须在当前 apt 索引有 Ubuntu Noble 官方来源证据；允许
   noble/noble-updates/noble-security，禁止 PPA、其他发行、proposed/backports、未知来源。
   该证据是宿主包管理器来源边界，不冒充独立的加密供应链证明。是否追加本地包文件完整性
   检查、精确来源解析命令及其局限，必须在 §9 冻结后才能实施。
4. 遍历已测得加载闭包与 kovenc 的真实 ELF dynamic/version/dynsym 元数据，找出真实
   libxml2 引用者及所需 symbol/version；consumer 必须提供所需可见定义及匹配的符号类型，
   版本名必须与归属 libxml2 的 version need 对应。无版本引用的归属不明确、未知格式、
   空/矛盾事实、工具缺失/非零/超时均拒绝，不凭 `xml` 前缀或 SONAME 推断满足。
5. producer/consumer 均保留 path/hash/deb version/source、架构/SONAME、引用者和需要/导出
   符号的可核对事实；原始命令 argv/exit/stdout/stderr 继续留在 host_inspection evidence。
   不把 ABI 预检扩大为符号解析器/通用 ELF loader；若无法有限提取则保守拒绝并报告。

相同 hash 的原严格路径不因未知的兼容扩展元数据获得放宽；不同 hash 必须有完整匹配策略
证据才可考虑通过。旧候选缺少元数据仍按旧严格规则，不临时从网络补写 producer 事实。

## 6. 拟定数据合同与改动边界

新增仅 Ubuntu libxml2 有界 policy 子记录，保留现有 libraries 每项三字段。候选字段：
`policy`（固定版本标识）、`package`（name/arch/version/source/source_version/origin）、
`elf`（class/data/machine/soname/needed）、`required_symbols`（requester/name/version/type）、
`exports`（name/version/type/visibility，若涉及数据符号另明确 size 规则）。

字段精确形态、排序/去重、版本索引归属和无版本歧义拒绝规则须在真实输出审阅后冻结，
不能先实现再把实现倒写为合同。兼容判定只返回原有 compare_hosts 错误列表；结构化两侧
事实留 manifest/result，最终 success 仍由原 12 命令和清理决定。优先仅修改现有模块；
如新增 helper，则包内 TOOLS 精确成员与 bootstrap/hash 测试同步，不豁免包校验。

## 7. 验收账本与 CI-only 测试计划

| ID | 精确验收目标 | 当前结果 |
|---|---|---|
| C1 | 独立设计审阅；真实 dpkg/apt/readelf 输出核对，明确来源局限、依赖符号算法 | 待完成 |
| C2 | CI 协议正例：同 hash 保持；不同 hash 且全部 policy 事实满足；交换生产/消费方向按实际需求判定 | 未运行 |
| C3 | CI 协议拒绝：非 Ubuntu/架构/SONAME、路径变化、其他库 hash、LLVM/cc/SDK/override、重复/缺字段、未知策略 | 未运行 |
| C4 | CI 解析/故障拒绝：未知/歧义/缺失 symbols/version、不可见或错误类型 export、无实际引用者、工具缺失/非零/超时、非官方/失去版本索引 | 未运行 |
| C5 | 现有 test_preview_host.py、test_preview_install.py、test_preview_package.py 完整 CI；原包篡改/成员/权限/源/清理拒绝不弱化 | 未运行 |
| C6 | 新 head 双宿主完整 CI：各 producer、独立 consumer 12/12 真命令、清理和哨兵；准确记录两侧 libxml2 事实 | 未运行 |
| C7 | 若实际 CI 恰同 hash，只记严格路径验证；差 hash 真实跨修订证据未建立则明确保留，不以 fake 正例冒充 | 待实际结果 |
| C8 | 当前 Architecture/交付说明同步、文档静态门禁、独立代码审阅、按实际证据归档 | 未运行 |

所有 fake/mutation/fault 校准和 Python 协议测试仅在 CI 运行；云端当前只允许静态检查，
真实只读宿主命令须经审阅。尚未运行任何测试、故障注入或 kovenc 命令。
行为变化先在 CI 建立失败测试证据，再实施最小生产修改。具体 CI 发布由唯一 publisher 执行。

## 8. 实施与提交计划

1. 草案第一片独审，提交合同/必要索引；纯诊断实现与CI-only协议测试独立小提交，真实CI建立 §9 输入。
2. 最小协议测试单独提交，在 CI 证实旧实现拒绝目标正例及既有边界；不在云端运行。
3. 实现单项策略与有限解析，保留原接口/完整性；同步当前说明，小提交复审后交唯一 publisher。
4. 按 C1–C8 回填精确 head/base/tree、实际命令/CI run 来源和未覆盖；全部必要证据满足后才归档。

## 9. 未决问题与停止条件

- 真实 runner deb 修订尚未知；本地只读输出只能证明解析可行，不替代历史 runner 证据。
- apt 索引通常会淘汰旧修订；未知/缺失版本保守拒绝，不能因影响通过率而改为信任包名。
- 来源证据和文件所有权之间是否需追加 `dpkg --verify` 或包内摘要核验，待独审。
- 实际 libxml2 引用者的 version needs/undefined symbols 格式及数据符号是否出现待真实读取。
- 可安全限定的无版本符号归属规则、对象 size 与 weak/global 的处理待审；未知不默认兼容。
- 不保证精确已见两 runner 的兼容路径一定通过；如果真实事实不满足，保留失败并报告具体差异。

第一片取证合同已独立审阅通过；后续兼容判定仍待真实 Ubuntu 证据与独立审阅。不得将本稿当作兼容验收通过。

## 10. 已执行的本地只读检查

2026-10-15，经授权读取本云真实 ELF 元数据：环境为 Debian 13 trixie，readelf 2.44、
dpkg-query 1.22.22、apt 3.0.3。本云 libxml2 文件未登记为所查 libxml2:amd64 包；apt-cache
遇到配置文件权限拒绝，未绕过，不继续来源探测。因此不能替代 Ubuntu 来源或兼容验收。
现存 LLVM 21.1 ELF 对 libxml2 有 16 个 versioned UND（15 FUNC 与 xmlFree OBJECT），
涉及 LIBXML2_2.4.30/2.6.0；xmlFree UND size 为0，provider定义size为8。大量无版本
UND 实为其它库用途，不能一概拒绝或只按 xml 前缀归属。该观察只支持设计讨论。
未运行任何协议/行为测试、故障注入或 kovenc 命令；git diff --check 静态检查通过。
