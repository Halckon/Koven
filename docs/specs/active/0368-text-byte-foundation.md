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
