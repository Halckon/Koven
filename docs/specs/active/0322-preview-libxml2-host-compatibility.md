# SPEC-0322: Preview 外部 libxml2 有界宿主兼容性

> **性质**：候选工程合同 · **状态**：in-progress · **读取时机**：审阅有界宿主预检策略时 · **唯一真源**：本页；现行行为仍由脚本与交付说明定义

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-322` |
| 所属 Phase | Phase 6 工程交付；不改变语言/native ABI |
| 语言规范 | [Guide v0.43](../../guide/README.md) |
| 批准依据 | 用户 2026-10-15 同意仅 libxml2 有界兼容方向；§4取证合同与§5–6 C1设计独审通过；生产实现须先有CI红证据及diff独审 |
| 前置 Spec | SPEC-0272 |
| 前置 ADR | 无新增 |
| 阻塞项 | C1及tests-only真实CI红已审；最小实现/补充协议测试待独审与新CI验收 |
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

## 5. C1 冻结设计：仅已见两修订的有界预检

本节设计经独审通过，tests-only CI 已建立行为红；现获准最小生产实施，最终验收仍待新CI。仅两侧 `Linux/x86_64`、真实 os-release
`ID=ubuntu / VERSION_ID=24.04 / VERSION_CODENAME=noble`，唯一同 name/resolved 的
`libxml2.so.2` SHA 不同且 §3 全部成立时才进入例外；无其他 fallback。

### 5.1 包与来源边界

- v1 只接受完整 binary/source version 均为 `2.9.14+dfsg-1.3ubuntu3.9` 或
  `2.9.14+dfsg-1.3ubuntu3.10`；不是 hash 白名单，不按前缀、版本大小或“同 upstream”放行。
  新修订需要另一次事实审阅和规则变更；交换 producer/consumer 不免除任一侧校验。
- 真实 resolved 文件必须由唯一 `libxml2:amd64` owner 声明，状态精确为
  `install ok installed`，architecture 为 amd64，source package 为 libxml2。
  `dpkg-query --control-show ... md5sums` 必须有唯一精确相对路径条目，实际文件 MD5 与之相等；
  同时保留原 SHA256。此项仅检查本地 dpkg 清单一致性，不是防篡改或官方供应链证明。
- 本策略选择“受界 Ubuntu 安装环境证据”，不建立具体 installed 文件的来源安全证明。
  要求当前 apt Candidate 为上述两个已审阅版本之一，且 package policy 的该版本块至少有
  noble/noble-updates/noble-security 的 main/amd64 Packages 条目；全局 policy 必须有对应
  suite/component/architecture 且 `o=Ubuntu,l=Ubuntu,n=noble,v=24.04` 的 release 条目。
  不接受只有 status、候选缺失、错误发行/组件/架构或已知第三方来源；但这些标签不能认证URI。
- URI继续按§4脱敏；不能以 `<redacted-source>` 相等来联结两个具体源，也不将多个符合
  suite/component/arch 的记录猜成唯一源。标签仅是可观察环境旁证；URI脱敏时显式记录
  `source_uri_status=unproven`，实际installed来源无论当前索引是否有它均标为 `unproven`。
  不读取配置或记录未知原始URI绕过脱敏，不声称排除伪造本地包数据库/源元数据的宿主。
- 两个精确修订的官方发布存在已由人工审阅的公开页面支持：
  [USN-8787-1](https://ubuntu.com/security/notices/USN-8787-1) 列明 Noble `.9`；
  [Ubuntu Noble libxml2](https://packages.ubuntu.com/noble/libxml2) 列明 amd64 `.10` 与 source版本。
  这些页面是设计依据，不是本机文件来源证书；runtime不访问网页、不根据未来页面自动扩展版本。
- 旧installed版本退出当前apt索引不拒绝，只要上述完整dpkg身份、本地文件一致性、固定已见
  修订边界及当前候选环境旁证满足。取舍是避免把正常升级窗口当作不兼容；代价是明确不证明
  installed来源。预检没有防御恶意宿主的新增承诺，包自身完整性及其它依赖严格规则仍完全保留。
  不下载包、不刷新索引、不安装，不引入历史仓库查询或签名验证体系。

### 5.2 有限 ELF 事实，不实现 loader

- 两侧 libxml 都须 ELF64、小端、EM_X86_64，唯一 SONAME `libxml2.so.2`；
  DT_NEEDED 名称集合两侧严格相同、无重复且全部在原加载闭包中；RPATH/RUNPATH 必须不存在。
  现有闭包路径/hash 规则继续检查其依赖，不能通过新增或移走依赖取得兼容。
- 为避免只检查 direct requester 而漏掉符号引用，读取 kovenc 与现有完整闭包每个文件的
  `readelf --wide -d / --dyn-syms / --version-info`；追加 `-h` 于 libxml 和直接引用者。
  复用 §4 有界 probe，严格 UTF-8/退出/超时/截断检查。对未识别行、表数量/条目数不符、
  重复 dynsym 行索引、同一对象 version_r/version_d 的索引定义冲突、缺少其引用/索引声称存在的必要表或内部矛盾拒绝，
  不能通过正则只挑出认识的行而静默漏项。无版本需求/导出的对象可合法没有version_r/version_d，
  不要求所有闭包对象同时有两表；多个 dynsym 复用同一合法版本索引是正常情况。
- v1 要求真实 DT_NEEDED libxml 的直接引用者集合恰为闭包中的 `libLLVM.so.21.1` 对应路径；
  其路径/hash 本来必须两侧全等。任一其他对象出现归属 libxml 的 version need 或引用则拒绝。
  requester 的动态表、version need 与 UND 集合两侧一致；不硬编码 16 个符号名作兼容白名单。
- 将 requester `.gnu.version_r` 的每个 version index 映射到唯一 File/name/flags，
  dynsym UND 的索引和版本文本必须一致；只选 File 精确 `libxml2.so.2` 的记录。
  同时核验 `.gnu.version` 对 dynsym index 的对应，拒绝索引冲突、孤立版本或隐藏位歧义。
  libxml required 集合非空，所有引用均 GLOBAL/DEFAULT，类型仅 FUNC 或 OBJECT；WEAK、
  NOTYPE、TLS、IFUNC、GNU_UNIQUE 或其它未支持形式一律不走兼容例外。此限制仅针对
  libxml 候选需求及匹配定义，不拒绝闭包其它库自身正常的 IFUNC/WEAK 等符号。
- 需要的每个 `(name,version)` 在各自 provider 中都有唯一 GLOBAL/DEFAULT、已定义、
  `@@` default-version 导出，定义 version index 可核对到 `.gnu.version_d`。类型必须与 UND
  相同；FUNC size 不作为 ABI 合约，OBJECT provider size 必须正数且两侧严格相同；UND size
  为0时不据此推断对象大小，非0时须与两侧 provider 一致。真实 xmlFree 为0→8/8。
- 比较两 provider 所有已定义导出的名字并检查整个闭包：任何无版本 UND 与任一 provider
  导出名字相交，或非上述 requester 对相同 libxml version/name 有引用，均视为归属歧义拒绝。
  不把无版本 UND 全部拒绝（真实 LLVM 含其它库的无版本引用），也不按 `xml` 前缀归属。
  此规则仅排除可见歧义，不宣称模拟 ELF 搜索顺序或证明所有动态调用兼容。
- 符号预检仅是必要条件；函数签名、布局语义及运行时加载仍可能不兼容。最终必须真实执行
  原12命令、逐字节输出断言及 cleanup/sentinel；不能把该有限验收称作通用 ABI 证明。

## 6. 冻结数据形态与调用边界

保留 libraries 的 name/resolved/sha256 三字段、原 schema 与原 compare_hosts 错误列表接口。
新增可选 host `libxml2_policy`，标识 `koven.preview.libxml2-ubuntu-noble.v1`；其事实包含：

- `library`：name/resolved/sha256，与原唯一库记录逐字段绑定；`distribution`：id/version/codename。
- `package`：binary/architecture/status/version/source/source_version，`file_md5`：path/expected/actual；
  `apt_environment`：candidate/version-block的suite/component/architecture与release事实；
  `installed_origin=unproven`、`source_uri_status`明确区分可公开观察与未证，不含推断的源联结。
- `elf`：class/data/machine/soname/needed/rpaths/runpaths；`requesters`：path/sha256 与
  `requirements`（symbol index/name/version index/version/type/binding/visibility/size）。
- `definitions`：required name/version 对应的两侧本地导出 index/type/binding/visibility/size/default；
  `unversioned_imports` 与 `exported_names`：供两侧 union 名字歧义检查的有序去重事实。
  原完整 command/bytes 留在包外 evidence，不把完整 ELF dump 放入 manifest。

集合按字段字典序排序，重复原始定义/索引先拒绝而非靠去重隐藏。所有结构严格校验字段类型、
必需字段、策略标识与绑定；未知策略不能打开例外。采集失败留下结构化 policy error 和原始
probe证据，不抹掉 host 已有事实；同 hash 按原严格路径，不因可选策略未知而取得额外放宽。
不同 hash 必须两侧有效 policy 全部满足。旧包缺 policy 仍按原严格比较拒绝差 hash；
不从消费侧网络或当前包库补造 producer 信息。采集在两侧 inspect_host 中独立完成，
不依赖 CI 诊断开关来启用放宽；实现限现有 preview_host.py，不增 TOOLS 或包成员。

## 7. 验收账本与 CI-only 测试计划

| ID | 精确验收目标 | 当前结果 |
|---|---|---|
| C1 | 独立设计审阅；真实 dpkg/apt/readelf 输出核对，明确来源局限、依赖符号算法 | 设计独审通过；仅推进tests-only，完整闭包实采待CI |
| C2 | CI 协议正例：同 hash 保持；不同 hash 且全部 policy 事实满足；交换生产/消费方向按实际需求判定 | 初片三正例已取得预期行为红；实现后绿及完整矩阵待CI |
| C3 | CI 协议拒绝：非 Ubuntu/架构/SONAME、路径变化、其他库 hash、LLVM/cc/SDK/override、重复/缺字段、未知策略 | 未运行 |
| C4 | CI 解析/故障拒绝：未知/歧义/缺失 symbols/version、不可见或错误类型 export、无实际引用者、工具缺失/非零/超时、不符候选环境/失去ELF版本索引 | 未运行 |
| C5 | 现有 test_preview_host.py、test_preview_install.py、test_preview_package.py 完整 CI；原包篡改/成员/权限/源/清理拒绝不弱化 | 未运行 |
| C6 | 新 head 双宿主完整 CI：各 producer、独立 consumer 12/12 真命令、清理和哨兵；准确记录两侧 libxml2 事实 | 未运行 |
| C7 | 若实际 CI 恰同 hash，只记严格路径验证；差 hash 真实跨修订证据未建立则明确保留，不以 fake 正例冒充 | 待实际结果 |
| C8 | 当前 Architecture/交付说明同步、文档静态门禁、独立代码审阅、按实际证据归档 | 未运行 |

第二阶段具体协议矩阵（全部CI-only，先红后实现）：

- 同hash且policy缺失/unknown仍走原strict；差hash缺policy、错library绑定或其它库变更拒绝。
- dpkg owner/path重复、错误版本/source/arch、MD5不匹配拒绝；候选缺失/错误suite或release拒绝。
  脱敏URI明确未证且不拿占位符联结；旧installed退索引可符合环境策略；未知新修订拒绝。
- xmlFree UND0/provider8/8可满足预检、8/16拒绝；FUNC size改变不单独拒绝。
  required定义非default、hidden、weak、错误类型或版本归属其它SONAME/孤立索引均拒绝。
- 普通libc无版本UND可存在，碰撞任一libxml导出名拒绝；重复合法version索引使用可存在，
  索引定义冲突拒绝；无引用的合法缺省版本表可存在，孤立索引/必要表缺失拒绝；
  其它库正常IFUNC不误拒绝，libxml required IFUNC拒绝。
- 合成预检成功后第N条native命令失败、cleanup失败或sentinel损坏仍使整体失败；
  合成用例不冒充12条真实命令。真实requester/provider摘录与合成闭包fixture分开标注来源。

所有 fake/mutation/fault 校准和 Python 协议测试仅在 CI 运行；云端当前只允许静态检查，
真实只读宿主命令须经审阅。第二阶段尚未运行测试、故障注入或 kovenc 命令。
行为变化先在 CI 建立失败测试证据，再实施最小生产修改。具体 CI 发布由唯一 publisher 执行。

## 8. 实施与提交计划

1. 草案第一片独审，提交合同/必要索引；纯诊断实现与CI-only协议测试独立小提交，真实CI建立 §9 输入。
2. 最小协议测试单独提交，在 CI 证实旧实现拒绝目标正例及既有边界；不在云端运行。
3. 实现单项策略与有限解析，保留原接口/完整性；同步当前说明，小提交复审后交唯一 publisher。
4. 按 C1–C8 回填精确 head/base/tree、实际命令/CI run 来源和未覆盖；全部必要证据满足后才归档。

## 9. 第二阶段停止条件

- C1 独审未通过之前不写 tests/production；通过后先 tests-only CI 红证据，再写生产兼容实现。
- 已回收的 producer .9 只有 status 来源，两侧URI均脱敏；按§5.1明确记来源未证，
  不称具体installed官方来源已验证。该口径已获独审同意，不能把标签改名为来源安全证明。
- §5.2 新增完整闭包 dynsym/version-info 读取尚未在真实 Ubuntu CI 采集，因此无版本歧义
  排除仍待证。fixture 明确区分已采真实 requester/provider 摘录与合成完整闭包场景，
  不称第一片已经覆盖完整解析。任何未知格式或不满足有界规则都停止，不扩张为通用 ELF loader。
- md5sums 条目的实际文件一致性及候选环境解析尚未实现/验收；不声称已经验证。
- 历史失败保留，不重跑历史 attempt 以抹红；新测试/修订使用新 head 留独立证据。

## 10. 已执行的本地只读检查

2026-10-15，经授权读取本云真实 ELF 元数据：环境为 Debian 13 trixie，readelf 2.44、
dpkg-query 1.22.22、apt 3.0.3。本云 libxml2 文件未登记为所查 libxml2:amd64 包；apt-cache
遇到配置文件权限拒绝，未绕过，不继续来源探测。因此不能替代 Ubuntu 来源或兼容验收。
现存 LLVM 21.1 ELF 对 libxml2 有 16 个 versioned UND（15 FUNC 与 xmlFree OBJECT），
涉及 LIBXML2_2.4.30/2.6.0；xmlFree UND size 为0，provider定义size为8。大量无版本
UND 实为其它库用途，不能一概拒绝或只按 xml 前缀归属。该观察只支持设计讨论。
未运行任何协议/行为测试、故障注入或 kovenc 命令；git diff --check 静态检查通过。

2026-10-15：§4 独立设计审阅通过后迁 active，仅允许只读取证阶段；§5 及兼容例外未获实施批准。首次 docs 检查因四份 DAG 过期失败，按既有 generator 更新后 674 项通过；非语义或行为验收。

## 11. 第一片实施时账本（当时待 CI，后续结果见 §12）

- `5ecf25a7`：完整草案/编号/索引；`7ec5e486`：仅§4取证迁active。
- `909a0d6c`：先提交CI-only诊断协议测试，未运行，未取得动态红证据。
  随后实现只读取证与静态独审修订；共15个诊断协议测试定义（不等于已执行）。
- 生产比较函数、原host返回值、原commands集合、包成员/校验和、12命令及清理路径无改动；
  取证记录独立于这些验收结果。仅Ubuntu两step开关，正常宏观CI可能仍因原hash不等失败。
- 云端只做AST语法解析、docs结构检查（674项）和diff空白检查，均通过；未执行helper、
  任何mock/fault测试或kovenc。CI协议执行、真实Ubuntu来源/符号事实与安装结果全部待证。
- 不新增后续compatibility policy字段，不把首片诊断完整性称为libxml2兼容或安装通过。

## 12. 第一片真实 CI 与 C1 输入（2026-10-15）

来源：[PR106 CI 38667368062 attempt 1](https://github.com/Halckon/Koven/actions/runs/38667368062/attempts/1)。
PR head `b388e6dea5571dae95219d170ac22e317c4226a9`，本地等价代码 tree
`991e4e03f2c6aba64e357750eb86ac88e5fd4950`；失败结果不覆盖。
两个宿主代码门禁通过，15个新增诊断协议用例在CI通过（总354）；macOS consumer通过。
Ubuntu producer成功，consumer被原严格hash门禁拒绝，`commands=[]`，不是安装通过。

两侧诊断各29 probes exit0、无超时/截断；17库仅libxml SHA不同；同路径由唯一
libxml2:amd64拥有，完整binary/source版本分别为 `2.9.14+dfsg-1.3ubuntu3.9` / `.10`。
两侧唯一直接引用者是 `/usr/lib/x86_64-linux-gnu/libLLVM.so.21.1`，16个 GLOBAL/DEFAULT
versioned UND：15 FUNC、1 OBJECT xmlFree；后者UND size0、两provider定义size8。
xmlReadMemory需要LIBXML2_2.6.0，其余15需要LIBXML2_2.4.30；两侧对应定义均存在。
这只支持冻结有界预检，不证明运行兼容。URI全部脱敏，producer旧修订不在当前apt源版本块，
故来源仍不完整。完整artifact摘要和原始输出保留在该run artifact，不复制完整dump入库。

## 13. 第二阶段先红与实现切片（2026-10-15）

[tests-only CI 38673070071 attempt 1](https://github.com/Halckon/Koven/actions/runs/38673070071/attempts/1)
在发布head `74abcb99e062f1b5306a3496564b3b49a27f4b18` 对应synthetic merge执行367项协议测试，恰有3个预期
compare_hosts断言失败，无API缺失/导入错误；新增13项为10通过/3失败。这只建立行为先红，
不是兼容成功。CI synthetic与head仅Cargo.toml/lock不同，相关生产/测试/fixture/Spec blob一致，
红归属已独审核对；历史失败保留。

真实main已推进到 `8dae0b49403c40714bb1d99f02379ad7625dafac`（PR104 toml依赖升级）。
本地以零树差normal merge保留local/已发布父链，再normal merge该main，基线merge
`b9b3ae2cb35a981fa2293c62fc37b2d0067d1fda`，tree
`a6b4309e4210943ac1ab50d480953b6998c104da`，无rebase。新base必须重新完成真实验收。

最小生产切片限preview_host.py，保持TOOLS、包成员及consumer命令/清理生产路径不变；
补充CI-only协议矩阵含完整合成版本表、真实已见十六进制size/无0x首offset词法、合法缺省表、
单对象索引冲突、每个probe失败、两侧同步无效policy、第N命令失败与cleanup失败传播。
这些新增测试尚未运行；云端仅AST/docs/diff静态检查，不以合成完整表代替真实闭包读取。

## 14. 首次实现CI的真实header拒绝与最小修复

[CI 38675369744 attempt 1](https://github.com/Halckon/Koven/actions/runs/38675369744/attempts/1)
使用发布head `15ed9483982ac424eb15ff5b12b2e930afc3cac2`，结果failure。Linux两侧各60个
policy probes成功，但正常GNU ELF header有两个语义不同的Version字段（`1 (current)` 与
`0x1`），解析器把第二个当重复键拒绝；两侧policy未生成，consumer命令0条，严格门禁未放宽。
本轮真实包修订为producer `.10`、consumer `.9`；macOS独立consumer12条与cleanup/sentinel通过。

修复仅对Version接受上述精确有序对，其余重复字段和不支持架构继续拒绝。CI-only回归复用
已保留的完整真实header文本（首轮probe24，与本轮两侧probe59字节相同，SHA256
`ae8b8f05034fa46aac15bfb109245147ffa9fe5a1551ef927b963f937526e3f3`），补缺失/错值/反序/
多Version、重复Class/Machine反控。新修复基于实际已发布15ed后继，不再次合并等树映射历史。
尚未运行新回归；新CI仍须验证policy真实生成、完整闭包以及12条命令/清理，不覆盖本次失败。
