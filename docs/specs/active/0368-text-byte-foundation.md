# SPEC-0368: 可信文本扩展与 UTF-8 字节往返

> **性质**：实现合同 · **状态**：in-progress · **读取时机**：实现或验收文本字节基础时 · **唯一真源**：本 Spec 的交付边界；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0368` |
| 所属 Phase | Phase 2、3、4、5 |
| 主里程碑 | CORE-V1-M1 / ECO-V1-M1，M1-S1 基础实现 |
| 语言规范 | [Guide v0.47 文本合同](../../guide/13-program-runtime-standard-library.md#文本字节与严格解析)、[受限标准扩展](../../guide/05-declarations-callables.md) |
| 前置 Spec | [0367 String 长度不变量](../../archive/specs/0367-string-length-invariant.md)、[0349 Result](../../archive/specs/0349-builtin-result-and-check.md) |
| 批准依据 | [生态持续实施授权](../../development/ecosystem/governance.md#立项与依赖)，现行 Guide 已启用本片表面 |
| 关联 ADR | [0018 String ABI](../../adr/accepted/0018-string-owner-runtime-abi.md) |
| 语言语义变更 | 否 |
| 阻塞项 | 无语义阻塞；实现与动态验收未完成 |

## 1. Goal 与输入身份

从 main `da5d93013125974fa8d736a541aee3ab727407c7` 建立
`feature/spec-0368-text-byte-foundation`。让单文件与 project 公开 CLI 通过显式导入使用
UTF-8 字节长度、独立字节副本、严格解码、按字节切片与可恢复连接，保持调用期借用和独立返回 owner。
不能以 helper、SSA 单测或仅一种入口代替真实公共调用的 build/run 验收。

## 2. 范围与实现边界

- 标准源码资产固定为 `crates/lang-std/koven/text/text.ko`，包为 `koven.text`；宿主按实际
  SourceId 授权，不从包名、文件名、路径或源码文本推断可信身份。
- 实现 `String.byteLength`、`String.toBytes`、`decodeUtf8`、`String.sliceBytes`、
  `String.checkedPlus` 及 Guide 定义的 TextError。签名、错误优先级和偏移口径以 Guide 为准。
- 扩展选择复用 package/import 可见性和 canonical callable 身份；String 扩展只接受可信
  标准源、Borrow receiver 和普通 owned 结果，不能借用 N1a 的 carrier/from 合同冒充。
- 标准源承载公共函数及可表达的范围检查、错误构造和长度策略；受控 runtime 只补存储读取、
  UTF-8 检验和复制所需边界。先核对并复用既有容器生成、String 连接／分配和 argv UTF-8 扫描。
- typed 产物明确发布调用及 receiver 事实；ownership、SSA/verifier、LLVM 消费这些事实，
  不从函数拼写或 AST 重新猜测语义。两前端入口与两 CLI 模式不得出现准入／清理分歧。
- 返回 String／Array 均独立持有数据；输入保持有效。保持 Int 长度不变量、OOM Abort、
  空值／空缓冲行为、UTF-8 与内嵌 NUL，不以 lossy decoding 或截断换取成功。

## 3. 非目标与后继

本片不交付 split／lines 的业务扫描、parseLong／格式化、IO、任意用户扩展或公共 FFI。
TextError 按现行合同保持完整分类；EmptySeparator 的实际产生路径随 split 后继交付，不能
用占位 split 函数伪装能力。本片归档后 M1-S1 仍须落实 split／lines，M1-S2 与 M1-S3 仍开放。
不改变 clone intrinsic、现有 Range Borrow 返回合同、用户同名声明语义或未启用语言能力。

## 4. 验收矩阵

| 主题 | 必须证明的结果 |
|---|---|
| 来源与导入 | 显式导入成功；未导入、伪造同包／同路径／同名源码不能取得标准能力；普通同名 callable 不被劫持 |
| 扩展签名 | 非标准源、错误 receiver／模式／结果、carrier/from 签名被拒绝；N1a 与 clone 回归不变 |
| 字节长度与复制 | 空串、ASCII、中文、多字节 scalar、NUL 长度与全部字节准确；副本不别名输入，销毁顺序独立 |
| 严格解码 | 合法边界及完整往返；孤立 continuation、非法 lead、overlong、surrogate、超 Unicode 范围、截断拒绝 |
| 错误偏移 | 有效前缀后的非法 continuation／截断报告序列起始字节；首错稳定，不按 Unicode 字符计数 |
| 字节切片 | 负数、反向、越界为 InvalidRange；有效范围的非 scalar 边界为 NotScalarBoundary；边界空切片有效 |
| 可恢复连接 | 两输入各求值一次、保持 owner；正常结果独立；超限仅返回 null；OOM 仍 Abort，不先复制超额输入 |
| 所有权与控制流 | 借用命名值／字段／临时值、结果早退、错误分支与容器持有结果均正确清理；输入后续可用 |
| 公共入口 | 单文件/project 的真实 import、check、build/run 与输出／退出码一致，不能只测手工注入标准源 |
| 回归与平台 | 双宿主官方 CI、既有 String／argv／Range／Result 行为、尺寸与文档门禁、独立审查 |

长度上限用合成长度和受控分配器探针覆盖，不真实分配 GB 缓冲；明确区分守卫证据与正常
小输入的真实编译运行。运行失败、被忽略或未运行项单独记录，不作通过计数。

## 5. 实施顺序

1. [ ] 盘点可信来源、扩展选择、ownership 与 native 接入点及直接调用者，确认既有能力和缺口。
2. [ ] 建立失败用例；接通标准源／TextError 和精确 String 扩展事实，验证防冒充及原有扩展回归。
3. [ ] 补充最小字节读取、严格 UTF-8 与复制边界；用 Koven 标准源组合公共接口，验证字节往返与错误偏移。
4. [ ] 验证切片和 checkedPlus 的边界、求值次数、OOM 与 owner 生命周期，补齐两种公开 CLI 入口。
5. [ ] 独立审查共享逻辑与最大剩余风险，修复并复核；同步实现事实、官方 CI 与验收账本。
6. [ ] 验收通过后归档、同步库存和里程碑；最终 head 全部适用检查结束后合并 PR。

## 6. 当前证据

已编写来源授权、byteLength／toBytes／checkedPlus 的两套前端签名绑定与 canonical
扩展候选选择，以及来源防冒充、签名拒绝、owned 结果、导入／alias／遮蔽用例。
unit 已为 sliceBytes 使用内建 Result 的 canonical 声明及授权文本源的 TextError enum 身份，
并补充未授权 Result／外部同名 TextError 的拒绝用例；旧单文件 checker 尚无同等 Result 身份接线。
此部分尚未编译或执行测试；标准源加载、公共函数、完整的双入口结果身份、
ownership 至 native 链路均未完成，不能据此宣称公开 API 可用。
独立静态审查覆盖授权身份、签名限制、候选选择、this 与构造点，未发现确认缺陷；
提出的跨文件覆盖缺口已补测试源码，仍待动态验证；复核发现顶层 inout 扩展先被 Parser 拒绝，
已从要求解析成功的类型测试数组移除该错误夹具。文档结构检查（821 篇）与 diff 检查通过。
前置 SPEC-0367 已由 PR #233 合入上述 main；其证据不能替代本片接口、来源或运行验收。
后续逐项记录精确输入提交、命令、实际结果和稳定 CI 来源，原始日志保留在仓库外。

首片提交 `2a749117cf4e5cf429f51d032baef6bb930eeacf` 的
[PR #235 CI](https://github.com/Halckon/Koven/actions/runs/40926038826) 在文档／CI 规则测试失败：
新增 `type_text_extension` 后精确选择数量应从126变为127，旧清单断言未同步。
464项规则测试中该项失败；Rust 编译与运行作业被跳过，不能视作前端通过。
已同步精确数量并增加新目标只出现一次的断言，等待修复提交的官方验证。

`1c753cdf97ad721f777b18e8d6c9e2f9c186dba8` 的
[Linux Check／Clippy](https://github.com/Halckon/Koven/actions/runs/40926568205/job/123634034551)
在 Clippy 将11项缺失公开 API 文档告警提升为错误后失败；已补齐对应枚举与查询方法的说明，
未放宽 lint。行为测试仍未执行，macOS 尚未形成验收证据。

unit SSA 现已接入文本 receiver 首个共享借用参数，DirectCall 按相同顺序前置该实参，
保留原调用求值与清理帧；String 运算从扩展 this 的真实引用读取 entry Borrow binding。
独立审查发现并闭合 this 原先误入 nominal receiver 路径的问题，继续核查 instance planner
未发现 owner=None 的额外阻断。`text_extension_tests` 新增 SSA／LLVM／native ABI 夹具，
断言非 carrier 的 owned 结果、两次分配的逆序释放及输入后续可用；尚未执行。
夹具的常量 byteLength 仅隔离验证 receiver ABI，不作为真实字节长度算法或公共 API 证据。

已将 argv 的严格 UTF-8 扫描提取为共享 `llvm/utf8.rs`，以 target-usize MAX 表示成功，
否则返回首个非法序列的起始字节；argv 仍比较成功标记后走原 operational failure 路径。
新增 native 夹具以 Rust `valid_up_to()` 为独立 oracle，覆盖30组序列／边界及全部256个
单字节输入。独立静态审查核对 PHI、受长度支配的读取、续字节范围及错误起点，未发现确认缺陷；
LLVM verifier 与 native 测试尚未执行，decodeUtf8 公共函数仍待接线。

两套 checker 已建立五个存储原语的固定类型合同与 host SourceId 门禁，复用普通实参映射和
调用完成检查，仅为本次新增的成功调用发布 `TextPrimitiveKind`。内部原语不作为公共文本 API。
新增来源、参数类型／数量／模式、具名／泛型误用和普通同名函数测试；独立复核修正两处候选
集合类型错误，并核对错误原子性、嵌套调用定位及 trial 回滚，未发现其他确认缺陷。
原语事实尚未接入 SSA／LLVM，测试未执行，不能据此声称字节读写已可运行。


远端输入 `719957bc1e703afa363bd3b7db6ec423735f5c08` 的双宿主 Check／Clippy 通过。
[Linux 首轮行为测试](https://github.com/Halckon/Koven/actions/runs/40927257708/job/123640764286)
中 frontend lib 229 passed、0 failed、0 ignored；codegen 2478 passed、1 failed、0 ignored。
新增 receiver fixture 在类型阶段失败：把 Int 结果直接传入当前只接受 String 的 println。
已改为先断言数值再输出 String 标记；此失败未进入该用例的 SSA/native 断言，后续 stage
integration 未运行，不能把新增 type_text_extension 目标记为通过。

当前继续增加 StringByteLength／StringByteAt 的 IR-local 操作、严格 Int/UByte/shared-loan
verifier、unit typed-call 消费与 LLVM 字节读取；复用现有参数 loan／cleanup 和 String view。
新增 UTF-8／NUL／owner 保留、空串长度、负索引／末端／空串 Abort 与错误结果／已结束
loan 的用例。独立静态审查未发现确认实现缺陷；指出 ended-loan 测试互相掩盖的风险后，
已改为分别检查两条操作的错误位置。上述新增代码尚未经过 Rust 编译或 native 动态执行；
其余 UTF-8／复制原语的 SSA 接入和公开标准源码仍未完成。

同一输入的 [macOS 测试](https://github.com/Halckon/Koven/actions/runs/40927257708/job/123640764153)
也在该 receiver fixture 的 L0084 类型检查处失败；frontend lib 229 passed、0 failed、
0 ignored，codegen 2477 passed、1 failed、1 ignored。忽略项仍为需要 debugserver 权限的
LLDB 断点用例，不能算通过。当前字节读取片的尺寸门禁通过，未增加尺寸例外。

输入 `a49749498294620eec65229de3bec57cf6955c8d` 的
[Linux 编译门禁](https://github.com/Halckon/Koven/actions/runs/40930435335/job/123647622119)
发现 UTF-8 native fixture 使用了 Inkwell FunctionValue 不提供的 set_name；已核对锁定依赖
0.11.0 的真实 API，改用 as_global_value().set_name，独立提交修复。该 head 尚无新增 native
通过证据。

当前新增 Utf8ErrorOffset 的 Array<UByte> shared-loan SSA 接入，unit 调用复用原有参数
清理；LLVM 复用共享 scanner，将合法标记映射为 Int -1。测试补充截断序列起点、空数组、NUL、
输入数组继续可用，以及 List／Int 元素／signed Byte／exclusive／ended／错误返回类型的精确
instruction 拒绝矩阵。独立静态审查确认构造、header、Int 映射、分配计数和错误定位，无确认
缺陷；这些新增用例仍未编译执行，不代表 decodeUtf8 公共 API 已交付。

输入 `c6cf4166ee8d205b2688d08b9ae146f9b7231505` 的
[官方 CI](https://github.com/Halckon/Koven/actions/runs/40931185019) 双宿主 Check／Clippy
已通过；两个 Targeted Tests 作业仍在运行，尚不记为行为验证通过。

新增 StringCopyBytes／StringCopySlice 的 shared-loan SSA 和 unit 调用接线。数组复制先做严格
UTF-8 检验；区间复制先做 Int 范围与顺序检验，再检查两端 scalar 边界，末端不读越界字节。
两者复用既有 String.clone 的独立分配与空串路径，底层非法输入 Abort；公开 Result 分类仍须
由后续 Koven 标准源码完成。测试覆盖独立 owner／逆序释放、NUL／空复制、非法 UTF-8、
负数／反向／越界与非 scalar 边界；CopyBytes 复用精确 Array<UByte> verifier 拒绝矩阵。
独立审阅发现非法 UTF-8 fixture 的零分配计数会提前 Abort，已修为只允许源数组的一次分配并
复核，错误文案断言保留；Abort 用例不作为释放计数证据。文档、diff、尺寸门禁通过，未扩大
尺寸例外；本复制片尚未经过 Rust 编译和 native 执行。

上述 `c6cf4166` 的双平台 Targeted Tests 均在 frontend 内建身份清单断言失败：
新增五个文本存储原语未同步 expected 名称；两宿主均为 228 passed、1 failed、0 ignored，
codegen/native 与 stage integration 尚未运行。已同步精确清单，并逐个断言 TextPrimitiveKind
绑定，保留唯一性检查，独立提交测试修复；不改变授权或生产语义。

已加入真实 `koven/text/text.ko` 并由共同 CLI loader 在 ranges／Result 后加载授权；公共算法
与错误分类留在 Koven。独立审阅确认直接 This capture 会被现有 closure layout 拒绝，已改由
私有 Borrow 参数 helper 承载同步 Array 生成，复用现有 Symbol shared capture。
新增真实单文件/project check、build、run 测试：UTF-8／NUL 往返、切片／非边界错误、
InvalidRange 优先级、非法 continuation／截断起始偏移、checkedPlus、动态 source 后继续
使用字节副本；另覆盖未导入与非授权原语拒绝。普通成功运行不替代 source 实际释放时序的
SSA 或 sanitizer 证据。标准资产存在测试与 loader 身份／完整类型所有权测试已同步。
公共源码与新测试已经过独立静态复核，尚未编译和运行；Result<String, TextError> 的完整
native 使用与所有验收矩阵仍需实际验证，保持 in-progress。

新增 checkedPlus 受控 native 探针直接读取标准源码中的实际函数体：合成合法输入长度
Int.MAX_VALUE + 0 必须到达分配器，+1 必须不分配并返回 null；小合法长度模拟 malloc
失败必须产生 allocation-failed Abort。不可读 bytes 用于暴露提前访问，不证明 GB 成功复制或
析构。公共 CLI 场景增加 left／right 标记与 xy 输出，要求左右各求值一次及真实小输入结果。
独立静态审阅已核对源选择、ABI、守卫／分配顺序和预期输出，无确认缺陷；新增用例尚未运行。
当前远端 `4fad95e6` 的 Linux Check／Clippy 已通过，macOS Check／Clippy 仍排队，继续跟踪
[同一 CI](https://github.com/Halckon/Koven/actions/runs/40932966990)，不将排队记作通过。

本地真实 CLI 首次执行暴露标准源码 scalarBoundary 将 UByte 与默认 Int 字面量比较的
L0085；仅补注解仍产生 L0084，最终使用明确 UByte 变量和 `128u`／`192u`。公共夹具的
NUL 改为 Guide 定义的 `\0`，Array<UByte> 的返回字面量使用无符号后缀。修正后真实 CLI
前端检查与未导入／非授权原语拒绝测试通过；完整正向 build 仍报 lowering MissingFact。
逐项真实 CLI 隔离已确认 toBytes、sliceBytes 成功与 NotScalarBoundary 错误路径可运行，
失败定位为 checkedPlus 返回的非 null String 读取；只判断其非 null 并输出固定 marker 的
真实 CLI 已成功。实际标准函数体的 overflow／分配边界／OOM 探针 1 passed、0 ignored，
不能将返回值的 smart-cast 读取缺口归因为长度策略。这些结果不构成整个公开 API 的完成验收。

MoveOnly enum 消费补片先以真实泛型 String payload 用例取得失败：0 passed、1 failed、
0 ignored，失败发生于首个 when lowering。现仅为 Name／透明 Group 的既有 owned binding、
且主体链无该 owner 的 AfterExpression 析构事实，开放普通分支传递；每次判别读取当前
重绑定 owner。共享 enum payload place 校验用于借用，保留具体泛型类型／case／field 身份，
不以 MoveOnly Read 复制 payload。独立审阅发现字段借用漏登 shared_field_roots，已修复并复核。
完整 enum 定向组 14 passed、0 failed、0 ignored，包含局部变量、own 参数、Group、连续
when、相反分支顺序、一个分支提前返回与另一分支正常借用，以及真实 3 次分配／3 次释放。
原主体 ASAP-drop 拒绝保留，补充 Group 与 temporary subject 拒绝。所有分支都直接 return
的 statement when 仍在 callable lowering 报 MissingFact；未将该形态纳入通过范围。
代码生成 crate 的 all-targets strict Clippy、diff 与尺寸门禁通过，未扩大尺寸例外。

另以真实 Result／text 标准资产和宿主 SourceId 授权验证 decodeUtf8：1 passed、0 ignored，
精确输出 é／NUL，动态核对两份独立分配按 decoded→bytes 释放（分配顺序
0=array、1=String，释放顺序 1、0）。源码后续仍读取 bytes.size，但此计数不证明 decoded
在 size 求值之前释放。该证据补充 Result payload 借用，不代替双平台完整矩阵。

远端 `4fad95e6` 的上述 CI 后续结束：macOS Check／Clippy 作业为 cancelled（未执行步骤），
Targeted Tests 与 preview 均跳过，CI 汇总失败。Linux Check／Clippy 成功不能替代缺失宿主及
行为验收；本地补片结果也不覆盖该次远端输入。取消原因未在作业结果中说明。

### nullable String 读取补片（2026-10-30）

新增真实源码回归先重现非空 String 读取的 MissingFact（0 passed、1 failed、0 ignored），
随后在 unit per-family 路径复用 NullableBranch／If proof／既有清理协议，不新增 LLVM 表示。
仅接受函数体直接 statement-if、named owned String?、精确 niche 布局及真实非空借用事实；
普通源函数与绑定的 String println 共享 owner／loan 身份校验。再次判断重新取当前 owner，
不沿合流复活旧 proof。修正文本夹具的 UByte 字面量，以及 project build 与单文件 build
复用同一输出路径导致的测试失败，未放宽前端类型或输出覆盖规则。

本地使用 LLVM 21、独立 target 和 `--offline` 串行运行下列检查：

| 命令（省略环境与 target 参数） | 实际结果 |
|---|---|
| `cargo test -p lang-codegen --lib nullable_string` | 5 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib text_` | 16 passed、0 failed、0 ignored |
| `cargo test -p lang-cli --test native_cli text_bytes` | 2 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib unit_lower::control::nullable_if` | 6 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib unit_lower::nullable_proof` | 1 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib utf8_error_offset_native` | 1 passed、0 failed、0 ignored |
| `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 |

公开 CLI 正向测试现已实际通过单文件和 project 的 import／check／build／直接运行及 CLI run，
包括 checkedPlus 返回值读取、左右输入各求值一次、字节往返与错误分类；另一项覆盖导入／
宿主授权拒绝。共享 scanner 的动态矩阵覆盖30组序列和全部256个单字节输入，不代替公共
decodeUtf8 全矩阵。nullable native 回归覆盖重复读取、反向条件、合流后再次判断及空串非 null。
内部 fault tests 核对错误 binding／owner／inner／niche／scope 与 pending operand／frame；
独立审查发现 pending 拒绝可能被已结束基本块掩盖，已改为精确 UnsupportedNode 和状态不变
断言，并复核通过。per-family 的未实现控制转移拒绝不作为所有入口均不支持的结论。

本补片尚未取得最终 head 双宿主 CI；字段／临时借用、容器持有结果、公共完整 scalar／错误
矩阵和所有分支直接 return 等剩余范围仍需逐项验收。保持 in-progress，不归档或宣称完整完成。
Rustfmt、文档结构（821 篇）、diff 及尺寸门禁通过；2012 个手写 Rust 文件中仍有39个历史
超限文件，本补片未增加尺寸例外。旧输入 `8db23df7` 的双宿主 Check／Clippy 已通过，
[Linux 行为作业](https://github.com/Halckon/Koven/actions/runs/40936020064/job/123672278781)
为2486 passed、5 failed、0 ignored：四项 UByte 夹具类型错误和一项 nullable String 读取
MissingFact，均对应本补片本地已通过的用例。后续集成门禁被跳过，不能据此声称远端通过。

### 公开矩阵与 String 字段 receiver（2026-10-30）

公开 CLI 复用真实标准源，在两种模式各执行 check、build、直接运行及 CLI run：新增5组合法
输入逐字节核对（空串、ASCII、中文、多字节／NUL、编码宽度边界及 Unicode 上界），17组
非法序列以 Rust `valid_up_to()` 独立计算首错起点；空串与11字节混合串的205对切片端点，
以 Rust scalar 边界及字节区间核对成功输出、InvalidRange 优先级和 NotScalarBoundary。
该矩阵不是全部 Unicode scalar／全部字节序列的穷举。

另一条公开用例先取得真实 check 成功、build UnsupportedNode 的失败；隔离后确认
`holder.text.toBytes()` 是缺口，临时输入、Result 的函数提前返回和容器持有已经可运行。
修复复用已有 owned 单层字段 projection／place 校验，对 String shared receiver 生成 loan，
避免 MoveOnly 字段 Read；receiver 帧在参数求值前登记父 root，保留 sibling replacement
拒绝。字段借用 helper 抽取后，原实参路径仍登记同一 root。未增加字段布局或语言语义。

| 本地检查（LLVM 21、独立 target、offline、串行） | 实际结果 |
|---|---|
| `cargo test -p lang-cli --test native_cli text_bytes` | 4 passed、0 failed、0 ignored；两模式公开矩阵及所有权可达性 |
| `cargo test -p lang-frontend --test type_text_extension` | 首次9 passed、1 failed；修正旧 UByte 夹具 `0`→`0u` 后10 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib public_text_field_receiver` | 3 passed、0 failed、0 ignored；计数、精确冲突与既有参数 return 拒绝 |
| `cargo test -p lang-codegen --lib unit_field` | 25 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib unit_lower_enum_tests` | 14 passed、0 failed、0 ignored |
| `cargo test -p lang-codegen --lib text_` | 19 passed、0 failed、0 ignored |
| `cargo clippy -p lang-codegen -p lang-cli --all-targets -- -D warnings` | 通过 |

计数用例覆盖两次 Group 字段 receiver 与后续普通 if 实参、两条 Result 返回路径、Array
持有两个成功 String payload：11次分配、11次逐活跃指针释放、无残留；未指定精确释放顺序。
旧 decode 的独立副本 `[1,0]` 释放顺序仍保留。独立复审核对实现、205对端点 oracle、
11次分配构成与 helper 抽取，未发现确认的生产缺陷。

计数试验也暴露原 receiver control-prefix 门禁：后续实参直接 return 仍在整个 call 处拒绝，
未进入字段借用逻辑；已保留精确拒绝回归，不能把普通 if 实参或 Result 函数返回混称为这项
能力通过。泛型／嵌套字段、Borrow 参数字段及所有分支直接 return 等剩余边界未由本片解决。
双宿主最终 head CI、完整生命周期矩阵与其余 Spec 验收仍未完成，不归档。

针对独立复审指出的 guard 假阳性风险，差分试验仅临时清空新 receiver frame 的父 root
登记：原精确拒绝测试变为失败，实际错误由 UnsupportedNode 变为 InvalidSsa，说明没有被
其他同 span 的准入门槛掩盖。试验后逐字节恢复生产文件；突变版本不提交、不作为通过证据。
恢复后同一 guard 复跑1 passed、0 failed、0 ignored，独立复审核对恢复文件摘要及文档边界。
Rustfmt、diff、文档结构（821篇）和尺寸门禁通过；2014个手写 Rust 文件中39个历史超限，
本片未增加尺寸例外。
