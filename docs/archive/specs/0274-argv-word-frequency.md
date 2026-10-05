# SPEC-0274: 现行语义下的多文件 argv 词频程序

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施或验收 M1B-a 参数词频首片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-274` |
| 所属 Phase | Phase 2/3 的现有事实交接、Phase 4 native、Phase 6 程序验收 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[集合](../../guide/12-collections-destructuring.md)、[String/entry](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行、按实际情况调整草稿的站立授权 |
| 前置 Spec | SPEC-0268 |
| 前置 ADR | ADR-0008、ADR-0016、ADR-0018、ADR-0019 |
| 影响范围 | frontend 容器长度事实/所有权读取、codegen unit String/长度 lowering、教程应用及验收 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

多文件 Koven 程序将每个 argv 参数作为一个完整词，按首次出现次序输出词与十进制计数，
在 macOS arm64 与 Ubuntu x86_64 上通过真实 project build、artifact、run 及资源验收。
起草基线是PR50 main `80b1c3ba5ae3dbce9e6bae7000c0a235e2d2f24d`，原合同提交`f28bf09`保留。
2026-10-05 PR51最终归档CI全部通过后合入main `7262232c5ec2b98e6ecdea6256fe41e985dd67aa`；
本片在该最新main新建worktree，分支`feature/spec-0274-current`接收合同与在途实现。
M1A 已验收，PR51未改Rust生产语义；M5b候选包不是本片启动前置，精确main push CI 37243777369已15/15成功，两候选消费者各12命令通过。

本片承接 [M1B 起草材料](../../development/text-processing-spec-draft.md) 的 a，
只修复程序实际触发的既有语义交接；文本输入 b、新公共标准库 API 仍待各自前置。

## 2. 基线事实与需求调整

2026-10-05 使用同基线生产代码的0272实现候选 CLI，在 checkout 外分离 project 用例预检：
`println(args[0])` 的实际 build/artifact 成功；只有 `.size` 的用例拒绝；
默认 Borrow String 参数的 `==`、`+` helper 各自被 unit lowering 拒绝；
采用 subjectless Boolean 条件的普通十进制函数实际输出 `12\n`。
完整第一版样例含 `.size` 与 Int subject when，不能把它的单文件 BlockingDeferred归因于后者。
这些是缺口定位，不是本片已验收。临时账本尚非长期交付证据，正式测试须重建完整命令身份。

| 起草假设 | 当前证据 | 本片处理 |
|---|---|---|
| 词频必需新增整数转文本 API | 整数算术、条件与 String concat 可组成普通应用函数 | decimal 保留在应用 `.ko`，不新增 intrinsic/插值/通用格式化 |
| 必需动态计数数组/MutableList | 重扫只读 argv 可直接计数并检测此前同词 | 首版 O(n²) 扫描，无额外词容器，顺序稳定；不承诺性能提升 |
| 原语 size 和 Borrow String binary 已可用 | 源码入口与 unit backend 有明确交接缺口 | 补现有类型/读取事实与 lowering，保留 frontend 权威 |
| 任意参数可默认为已转义文本 | String 最小表面未提供字符遍历/转义 | 精确采用下述原始字节展示合同，不声称 JSON/可逆行格式 |

## 3. 应用完整合同

- 输入继续由既有 `main(args: Array<String>): Unit` 接收。参数不分词、不折叠大小写、不做
  Unicode normalization；UTF-8 字节相同即同词。每个空字符串参数也是一个词。
- 输出每个不同词一次，按首次出现次序；每行的字节为原始词 + ASCII TAB + 无前导零的
  非负十进制计数 + ASCII LF。无参数时 stdout 为空，正常 exit 0、stderr 空。
- 含 TAB/LF/CR、空格、引号、反斜杠或非 ASCII 的词原样输出；该展示格式允许词内换行，
  不宣称可以按行无歧义解析。主机 argv 不能携带 NUL，不扩张这一平台入口。
- 非法 UTF-8 仍在 entry 调用前形成既有 operational failure；不静默替换字节。
  stdout 短写、分配失败等沿用既有 Abort，不能声称在 Abort 路径展开清理。
- 计数与索引为 Int；count不大于 args.size，size处于非负Int域。索引每次最多递增至size，
  不依靠整数环绕。decimal 应用 helper 支持0到Int.MAX_VALUE，负值调用既有 `error()`/Abort，必须在任何正常输出与后继marker之前终止；
  不保证stderr文本，也不承诺Abort展开清理。
  递归至多10层，既有 `%`、`/`、比较与 concat 即可完成，不改整数语义。
- 三个应用文件分别承载 main、统计与 decimal；教程 fence 是唯一源码，metadata 保存
  独立输出 oracle，不从被测编译器或新实现生成预期。

## 4. 既有编译器合同的补齐

### 4.1 容器长度

Phase 2 发布稳定、source-qualified 的 `.size` intrinsic descriptor：receiver、类型/容器
identity、结果Int、表达式/Span；不能让 codegen 按成员拼写重新识别语言能力。
Phase 3 将其作为同步只读 header 访问，不消费/复制 owner，不引入逃逸 loan；
保留 receiver 左到右一次求值、temporary owner 的既有 ASAP cleanup 与 borrow有效期。

本片至少交付三种已支持容器的具名local、Value/Borrow参数及grouped receiver长度读取，
以及零长度、非零长度、重复读取、读取后继续使用/移动owner。temporary receiver及其正常
清理纳入本片，含资源元素的容器需要检查先读取长度、后精确析构。
字段/嵌套元素 source 如仍被现有容器边界拒绝，保持原子且明确拒绝，不由此宣称一般投影已支持。
同名用户成员、读取已移动owner、给size赋值/传Inout均须沿用正确的身份与前端诊断。

backend 复用已有 `ContainerLength`、verifier与LLVM；不得增加第二套header或猜测大小，
Value/active shared Loan身份须与frontend事实一致；不绕过既有失效/exclusive loan拒绝。

### 4.2 unit Borrow String binary

unit `==`/`!=`/`+` 的 String view消费既有 owned binding 或 active Borrow binding；
group、跨文件具名callee、参数与临时operand的求值/清理身份保持一致。
不能以clone、retain或搬走参数owner替代同步共享读取。修复的不是新的String API。
覆盖动态/静态/空/非ASCII/NUL字符串、重复调用后源仍可用、concat结果独立清理、CFG之后loan重绑定。
无效/类型不匹配的SSA事实仍由现有verifier拒绝，不放宽验证器。

## 5. 验收矩阵

| ID | 必需验收 | 当前状态 |
|---|---|---|
| W1 | size descriptor身份/Span、输入排列确定性；读取分类与owner/loan/drop；负例精确诊断 | 本机已红转绿；见§8 |
| W2 | 三容器×空/非空、Value/Borrow/local/group/temp SSA/native，求值一次与资源精确清理 | 本机已红转绿；见§8 |
| W3 | unit String三种binary、Borrow/owned混合、临时/CFG、独立结果与源继续可用；malformed事实拒绝 | 本机已红转绿；见§8 |
| W4 | 三文件应用双宿主build/artifact/run；空、独词、交错重复、中文、空词、TAB/LF/CR、空格/引号/反斜杠等字节oracle | 双宿主真实CI通过；见§9 |
| W5 | 独立参考计数，0/1/9/10/99/100/Int.MAX_VALUE decimal及负值Abort；非法UTF-8在实际project artifact及CLI run中于entry前拒绝，记录各自退出/完整bytes；索引/计数上界说明；正常资源与M1A回归 | 双宿主真实CI通过；见§9 |
| W6 | 直接frontend/codegen/CLI消费者、原双宿主必需CI及既选M4a；独立完整审阅、Architecture/Spec归档及最终head PR闭环 | 实现/审阅及首轮15项CI通过；归档最终head及合并门禁仍必需 |

W4必须记录argv/cwd/编译器身份、build/产物/run每项退出与stdout/stderr字节；无命中和skip不算通过。
真实argv上限不可能构造Int.MAX_VALUE个参数；W5用应用decimal的独立边界调用及计数不超size的
代码/测试不变量说明，不伪造超大argv实测。未选支持/未运行门禁须明确列出。

## 6. 非目标

stdin/文件IO、String字符访问/转义/新整数API、Map/MutableList增长、runtime-length
initializer、内含类型参数的容器native实例替换、Int subject when、性能比较、公开Release/tag及M4b故障校准不在本片。
这些合法语言缺口保持独立后继；应用不用某操作不等于它已实现或不再需要。

## 7. 实施与提交

1. 合同与基线预检独立审阅 → docs/依赖图/作用域检查；尚不记W1–W6完成。
2. size facts/ownership红测→最小生产事实→最近frontend tests；codegen消费明确产物。
3. Borrow String/size SSA与native红测→最小实现→共享verifier和直接native消费者。
4. 应用及独立oracle→完整CLI命令与M1A→独立审阅→分支PR及首轮真实CI。
5. 真实验收账本、Architecture与归档同批→最终归档head CI全部必需结果成功→合并及主干CI。

不同生产文件在合同明确后可由子agent并行实现；所有本地Cargo门禁由root串行调度，
不得争用target。一个提交一个逻辑切片，提交信息含SPEC-0274；旧索引/PR51合入冲突先对照
真实状态处理，不重写原历史证据，不混入其它worktree改动。

## 8. 当前记录

2026-10-05 本机验收 checkpoint（尚未完成远端CI/归档/合并）：

- W1：size定向14项通过；扩大到`type_containers`、`ownership_containers`、
  `multifile_type_checking`、`multifile_ownership_checking`、`string_clone`共258项通过、0 ignored。
  assignment RHS红测确认CallReturn(size)提前析构旧owner；unit replacement栈保护后同例通过，
  return分支也核对ControlTransfer清理。独立frontend全审未发现剩余逻辑缺陷；
  随后新增break/continue（loop-local与inner-loop）及outer pending Borrow三组6场景；
  unit size filter共8函数通过、97 filtered，独立复核这些精确drop/loan断言。
- W2：7项SSA/native通过、0 ignored、836 filtered；三容器空/非空、Value/Borrow/group/temp、
  当前loan经CFG重绑定、读取后再用/移动owner与receiver一次求值。
  三个资源temporary合计9次allocation/free，逐指针与逆序deinit核对；
  assignment RHS native精确`rhs/old/after/finish/new`输出防提前释放旧owner。
- W3：原4项UnsupportedNode红测后最小current Borrow binding消费修复；
  6项SSA/native通过，含两种比较的RHS CFG与等长NUL后字节差异；相关String合同77项通过。
- W4/W5：教程8组实际build/artifact/run及decimal、负值Abort、非法UTF-8完整37命令通过。
  普通应用与entry marker wrapper均在非法argv调用前拒绝；负值oracle严格要求产物SIGABRT，不能把其他崩溃算作Abort；本机negative产物为SIGABRT，
  CLI run为exit 1，stdout空，未把stderr或unwind写作承诺。
  `scripts/check_word_frequency.py`保存argv/base64、cwd、compiler SHA256、源码SHA256及完整输出bytes；
  双宿主test job新增必需step与always evidence上传，配置存在不计作实际通过。
  timeout/spawn修复后的新harness本机37命令再次全部通过。
- 原tutorial文本模式会把CR换为LF；独立实际子进程红测后改为无newline转换的UTF-8 decode。
  失败命令包括timeout/spawn也保存账本；独立审阅指出的漏记问题已修复并注入复核。
- 已有尺寸欠账不提高baseline；本次8处增长使用精确有限exception，主体测试放在独立小模块。
  独立审阅核对实际行数与职责；完整Python192项通过、0 skip，fmt/docs518/size门禁通过；
  frontend/codegen/CLI严格clippy均通过，包括新增最后3frontend tests；
  codegen container相关55项通过、0 ignored、788 filtered；CLI native/project两套共21项通过、0 ignored；
  当前tour完整31项实际合同通过、1 planned不计通过。workspace all-targets check通过；W6仍待远端双宿主CI、验收归档与最终head合并闭环。

两项隔离定位失败保持未覆盖边界：容器owner在短路`||`不同路径最后使用的合流MissingFact；
以及`Array<T>`等内含类型参数的native实例替换在既有resolve_concrete_type入口UnsupportedNode。
本片长度矩阵用独立if和具体Int容器核对，不把这两项称作已修复。泛型size的frontend类型事实已验证。
字段/嵌套投影及单文件native size仍不由unit范围证明。

M1B-b、M2/M3新增语义和完整M5保留原草稿前置，本片不关闭它们。

## 9. 双宿主实际验收与归档（2026-10-05）

实现提交 `315169073700077bcd97f60d88af93a2bf7e7996` 的
[PR52 CI 37246195048](https://github.com/Halckon/Koven/actions/runs/37246195048)
已终态成功，15项全部success，没有以pending、skip或旧提交的取消代替验收。
两个Targeted Tests实际执行原有bounded composition、31组tutorial合同及新增词频step；
Linux选定ASan/LSan与双宿主Check/Clippy、两候选producer/独立consumer也成功。

长期证据保存在[CI身份与账本哈希](../../development/evidence/word-frequency-0274/ci.json)、
[Linux完整命令记录](../../development/evidence/word-frequency-0274/linux-results.json)及
[macOS完整命令记录](../../development/evidence/word-frequency-0274/macos-results.json)。
测试checkout为合成merge `62459600da9e2cba6f39dea087ab62e2ca62ecfa`，parents是main
`7262232`与实现`3151690`；其tree `049acd9ed9a4806149b713d334d0ba11819489ba`
与实现tree完全相同。artifact名称中的合成SHA不能误写成实现head。

每宿主9项目、37实际命令、全部success、无timeout/spawn错误。独立复核从教程唯一fence
取得三个源码与manifest哈希，并验证边界probe完整源码哈希及精确文件集合；另外按独立
首次顺序计数与decimal参考逐条核对argv原始bytes/cwd/exit/stdout/stderr，未依赖账本success标记。
两个宿主negative产物均为SIGABRT（-6），CLI均exit1且stdout空；正常与非法UTF-8四命令
均核对完整输出bytes；无效参数在entry marker前拒绝。两个Abort的stderr保留原始bytes，
继续不作为稳定文本或unwind承诺。源/编译器SHA、artifact ID及原JSON SHA在证据中可追溯。

W1–W5及W6实现/独立完整审阅已满足，本次迁入archive；最终归档head仍必须通过全部必需CI，
无未决状态后才允许合并。此记录不提前宣称归档head、merge或main push CI成功。
原§8本机checkpoint与隔离失败保持历史证据；短路owner合流、内含T容器native实例替换、
一般投影和single-file size等后继边界没有因本次CI成功而关闭。
