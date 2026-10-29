# M1B-b 文本输入与 IO（候选）

> **性质**：非规范候选设计 · **状态**：ECO-V1-M0 建议已选、Guide 未启用 · **读取时机**：选择 argv 之后的文本处理切片时 · **唯一真源**：本页的未选输入与错误合同；现行语义见 Guide

argv首片和Map应用分别由0274、0295承接；本页只保留未选文本输入能力，不重开argv矩阵。
[原材料](../archive/development/text-processing-spec-draft.md)是冻结历史；实施时重核main、
选有界Goal并形成Spec，新API先满足Guide前置。

## 待决定范围

- 首片选stdin或文件之一；封闭流/文件owner、打开、短读、读失败与关闭，不默认两者齐备。
- 固定UTF-8、换行/空白/分词、U+0000、非法字节和大小/词数/计数上限，不静默替换非法字节。
- 明确特殊词转义或拒绝、确定输出顺序与整数转换，不依赖未定义的哈希遍历顺序。
- 区分Result、宿主operational failure与Abort，固定退出码/stderr；`error()`不自动提供精确错误文本。
- 公共逻辑属Koven `.ko`；最小runtime桥接封闭ABI/权限/资源责任，不能假定已有能力可包装libc。
- String长度、索引、slice、分词、格式化逐项选范围，不从concat/clone推导。Result构造/匹配、
  泛型布局和`?`的typed/ownership/native支持分别核验。

## 后继验收条件

原T4/T5及T-C–T-F未选要求继续保留：空/多行/UTF-8/非法字节/短读/失败/超额输入；确定输出
和退出；计数/容量边界及独立参考；字符串、条目、容器、IO资源清理；公共入口、同名声明、
来源身份和错误Span正反例。选择Result才验Ok/Err，选择`?`另验Err提前返回owner/loan/drop，
正常释放不能扩大为Abort展开。

选定合同先红测，再用双宿主真实project build/run及直接阶段测试验收；M1A和argv保持。
不用宿主实现代替Koven程序，不删已选失败用例。normalization、locale/case folding、正则、
异步IO、Str、通用toString和公开C FFI均非默认范围。

## ECO-V1-M0 使用合同建议（2026-10-22，未启用）

用户于 2026-10-22 明确选择“采用建议，授权启用并实施”，批准本节合同，并授权协调已有 v0.46 后启用对应 Guide。
本页仍是非规范候选；实际 Guide 迁移与实施证据另行交付。

本节补齐 [三个使用草案](../development/ecosystem/api-use-cases.md) 的决策输入，不是 public API。
核对基线为 main `ed42a6096034655317690911205d420ce636729f`，Guide v0.45；
`crates/lang-std/koven/` 当前只有 prelude 与 ranges。以下签名是接口记法，不是可编译示例；
包名、标准声明加载与 intrinsic 身份必须由实施 Spec 另行冻结，禁止先加占位声明。

### 与已有设计的关系

- v0.45 只有 Unit process entry，不允许把恢复错误接成 `main: Result`。
- 已有 v0.46 决策输入位于提交 `acfacca31a5f43d9999c018bcb1f29d3bd786c6f` 的
  `docs/proposals/v0.46-language-decisions.md`：Result／Ok／Err 内建、must-use、Display、Result main。
  本候选沿用其方向，但该提交不等于 main 启用或实现验收；不另设异常、退出函数或静默丢弃 Result。
- 原标准库规划 PR #107 保留其集合、stdin、时间、sleep、math 目标；本片只细化文本／IO，
  不因三个示范程序暂不需要其他能力而删除 v1 目标。
- 新文本与 IO 标准库语义须先获用户选择并进入明确启用的 Guide；随后才建立实施 Spec。

### 文本与字节表面建议

默认参数为 Borrow；表中所有 String、Array 返回值都是独立 owner，不借用 receiver 的存储。
采用 UTF-8 **字节偏移**，避免把 Unicode scalar、字节和用户感知字符混为同一种索引。
所有外部字节严格解码；不替换非法序列、不正规化、不做 locale 处理。

| 候选接口记法 | 合同 |
|---|---|
| `text.byteLength(): Int` | UTF-8 字节数；String 最大字节数受 Int 上限约束，构造超额返回错误或按既有分配边界 Abort |
| `text.toBytes(): Array<UByte>` | 精确复制全部字节，包括 U+0000；源仍有效 |
| `decodeUtf8(bytes: Array<UByte>): Result<String, TextError>` | 验完整 UTF-8；首个非法字节偏移随错误返回；成功复制为新 String |
| `text.sliceBytes(start: Int, end: Int): Result<String, TextError>` | 半开区间；0≤start≤end≤byteLength 且两端均在 scalar 边界；空区间有效 |
| `text.split(separator: String): Result<Array<String>, TextError>` | 精确字节分隔、保留空字段；空分隔符报错，不提供隐式逐字符模式 |
| `text.lines(): Array<String>` | LF 分行，紧邻 LF 的 CR 一并移除；其他 CR 保留；空输入无行，末尾换行不额外产空行 |
| `parseLong(text: String): Result<Long, ParseError>` | 十进制 ASCII，可选一个正负号，至少一位；不 trim、不接收分隔符／Unicode 数字；越界返回错误 |

`TextError` 候选分类为 InvalidUtf8(offset)、InvalidRange、NotScalarBoundary、EmptySeparator；
`ParseError` 为 InvalidDigit(offset)、MissingDigits、OutOfRange。偏移均从 0 开始计字节。
Long 的十进制格式化复用既有 Display 设计，不新增竞争的格式化协议。
`lines`／`split`／解析业务循环优先写在 Koven；只有所需存储读取及 UTF-8 检验边界进入受控 runtime。

分配失败仍遵循现行 Abort 合同，不声称 Result 能恢复 OOM。外部长度先验 Int 上限；
已经存在的 String owner 不得因本建议被悄悄截断。实施前必须核现有长度 ABI 与这个上限的兼容性。

### 同步 IO 与资源表面建议

首片优先完成整个文件读取／写入，后续仍须补 stdin、流 owner 和显式 close；不是用便利函数取消资源合同。
同步阻塞，不隐式创建线程。路径首片为 UTF-8 String，拒绝内嵌 NUL；不展开 `~`、环境变量或通配符。
相对路径以进程工作目录解释；不规范化大小写，不承诺沙箱隔离或消除检查与使用之间的竞争。

| 候选接口记法 | 合同 |
|---|---|
| `File.readBytes(path: String, maxBytes: Int): Result<Array<UByte>, IoError>` | 非负显式上限；循环短读到 EOF，超过上限报 LimitExceeded，不返回截断成功 |
| `File.readText(path: String, maxBytes: Int): Result<String, IoError>` | 复用 readBytes 和严格 UTF-8；非法输入为 InvalidEncoding，不静默替换 |
| `File.writeBytes(path: String, bytes: Array<UByte>): Result<Unit, IoError>` | 创建或截断普通文件，循环短写；失败可能留下前缀，不承诺原子替换／持久化 |
| `File.writeText(path: String, text: String): Result<Unit, IoError>` | 精确 UTF-8 写出，不自动加换行；资源行为同 writeBytes |
| `Directory.list(path: String, maxEntries: Int): Result<Array<DirectoryEntry>, IoError>` | 非递归；排除 . 与 ..；超过显式非负上限报错；结果本身无顺序保证 |
| `Path.join(parent: String, child: String): Result<String, IoError>` | child 必须单个非空文件名，不含路径分隔符且不是 . 或 ..；不提供安全目录遍历保证 |
| `stdout.write(text: String)`／`stderr.write(text: String)` | 返回 Result<Unit, IoError>；write-all，不自动加换行；不改变现有 println 行为 |
| `stdout.flush()`／`stderr.flush()` | 返回 Result<Unit, IoError>；把本库缓冲交给宿主，不等于磁盘 fsync |
| `stdin.readLine(maxBytes: Int): Result<String?, IoError>` | EOF 前无字节为 null，空行为空 String；LF／CRLF 去终止符；EOF 的剩余字节仍返回一行 |

IoError 建议携带稳定分类、操作和 owned 路径／描述：InvalidInput、NotFound、PermissionDenied、
InvalidEncoding、LimitExceeded、Interrupted、Other；不把宿主 errno 数值或系统文案作为跨平台 oracle。
readLine 上限按终止符之前的原始字节计；超额／编码错误后 stdin 状态未恢复，不保证下一次调用续行。
宿主 EINTR 在无进展时重试，已读写部分保留进度；无法重试的中断才返回 Interrupted。

便利文件 API 在所有正常返回路径关闭自己打开的句柄；先操作失败再关闭失败时保留操作错误，
操作成功但 close 失败时返回 close 错误。关闭失败不得无条件重试，以免误关已复用句柄。
标准流不归调用者所有，不关闭宿主标准句柄。显式可关闭流 owner 留给独立后继合同，
须定义 close 幂等性、drop 的尽力清理及不可观测错误；本候选不从 RAII 推导“close 必成功”。

DirectoryEntry 建议有 owned name、kind（File／Directory／Symlink／Other）、`size: Long?`；
size 只对普通文件存在，无法表示的宿主大小返回错误。读取元信息不跟随链接；条目消失、
不可读或不可表示的文件名使整个调用失败，已创建 entry owner 正常释放，不输出部分成功。
列表不是目录原子快照；入口目录及文件打开仍遵循宿主符号链接行为，不能宣称禁止跟随所有链接。

### 三个程序的确定性 oracle 建议

每个示范程序先在自己的 Koven 源文件实现逻辑；不先拆 CSV／日志库或加入通用排序 API。
所有报告 write 与最后一次 stdout.flush 都成功后才返回 Ok(Unit)；失败沿既有 Result main 设计输出到 stderr 并返回 1。
这一路径当前仍有规范／实现前置，最终 flush 失败必须返回 Err，不能因较早 write 成功而退出 0。
业务错误显示文本固定；宿主错误只对稳定分类和操作断言，不比较本地化系统错误全文。

| 程序 | 冻结建议与最小 oracle |
|---|---|
| 日志统计器 | 单路径参数；每个非空行是 INFO／WARN／ERROR + 一个 ASCII 空格 + 消息；空行也报错；计数用 Long，溢出报错。输入 `INFO a\nWARN 中文\nINFO b` 输出 `INFO 2\nWARN 1\nERROR 0\n`；空文件输出三个 0；未知级别报告 1-based 行号 |
| 目录清单 | 单目录参数，非递归、不跟随条目链接；按 name 的 UTF-8 无符号字节序排序；每行 name、kind、size 用 TAB 分隔，非普通文件 size 为 `-`。name 的反斜线／TAB／LF／CR 分别编码为 `\\`／`\t`／`\n`／`\r`；无表头，空目录无输出 |
| CSV 校验器 | 固定三列 `name,count,note`，首行为精确表头；逗号分隔，双引号字段允许逗号和 `""` 转义；本片拒绝字段内换行，LF／CRLF 都可；name 非空、count 是非负 Long、note 可空。首个错误即停止，报告 1-based 行／列；空文件为缺表头；合法无数据输出 `rows 0\n`，一行合法数据输出 `rows 1\n` |

CSV 需要独立状态机，不能用 split 冒充带引号字段解析；`count` 拒绝负数、空格及正号，
即使 parseLong 接受正号。目录排序可先用程序内比较／插入逻辑，但比较必须基于字节而非宿主 locale。
三个程序暂建议统一输入上限 16 MiB、目录上限 100000 条；这是本次批准的示范程序产品策略，
不是语言容量上限。输出写／flush 失败必须返回错误；验收包含 write 成功但最后 flush 失败的负例，
不得把已输出前缀描述为完整成功报告。

### 实施顺序与批准点

1. 本节建议已获批准；正式启用 Guide 时协调已批准 v0.46 的相关设计，避免两套错误合同。
2. 核验 Result／Display／process entry 的 single、project 真实能力；缺口绑定生产方 Spec，不以 fake runtime 通过。
3. 文本字节与解析 → 文件便利函数／标准流／目录路径分片 → 三个程序与用户测试；各片 Goal 单一、编号实施时分配。
4. 每个公共 API 的正常／失败 owner、借用、复制、清理及双宿主行为均有反例测试；先失败后最小实现。

本节批准不等于 Guide 已启用或生产能力已实现；启用前不新增生产 API。M0-S5 仍须补实现前置矩阵，不能提前记为完成。

## 迁移准备

2026-10-29 的 [v0.47 迁移对账](v0.47-text-io-enablement.md)记录完整落点与三项可观察错误留白；
不重复批准本节合同，也不提前启用或实现公共 API。
