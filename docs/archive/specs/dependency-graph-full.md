# Spec 全量依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[Specs 索引](../specs/README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph-full.svg](dependency-graph-full.svg)。

```mermaid
flowchart TD
subgraph Garchive["已完成（archive，255 份）"]
  S0001["S0001<br/>建立可检查的 Cargo workspace"]
  S0002["S0002<br/>建立统一 source 与 Span 基础设施"]
  S0003["S0003<br/>建立结构化诊断核心"]
  S0004["S0004<br/>建立索引式 AST 基础"]
  S0005["S0005<br/>建立语言 fixture harness"]
  S0006["S0006<br/>建立确定性 Lexer"]
  S0007["S0007<br/>建立 Pratt 表达式 Parser"]
  S0008["S0008<br/>建立独立声明 Parser"]
  S0009["S0009<br/>建立 block 与 statement 序列 Parser"]
  S0010["S0010<br/>解析 lambda literal"]
  S0011["S0011<br/>支持具名函数隐式 Unit 返回"]
  S0012["S0012<br/>解析 callable 参数契约与 typed call argument"]
  S0013["S0013<br/>解析局部 val 解构"]
  S0014["S0014<br/>组合完整文件并跨声明恢复"]
  S0015["S0015<br/>解析 package 与 Kotlin 风格 import 文件头"]
  S0016["S0016<br/>解析 control-flow、jump 与 super"]
  S0017["S0017<br/>解析 class-family"]
  S0018["S0018<br/>建立单文件作用域与名称解析"]
  S0019["S0019<br/>建立基础类型检查与局部推导"]
  S0020["S0020<br/>建立名义/泛型类型与静态 interface 实现"]
  S0021["S0021<br/>检查 when 穷尽性与 smart cast"]
  S0022["S0022<br/>推导条件 Copyable 并检查结构化解构类型"]
  S0023["S0023<br/>检查顺序容器类型、构造与 element place"]
  S0025["S0025<br/>多文件 package/import 名称解析"]
  S0026["S0026<br/>单文件关联常量选择与编译期求值"]
  S0027["S0027<br/>建立变量所有权状态并检测 use-after-move"]
  S0028["S0028<br/>检查条件复制、消费式解构与结构分量移动"]
  S0029["S0029<br/>调用期借用与 ASAP 析构点"]
  S0030["S0030<br/>顺序容器 element place 所有权"]
  S0032["S0032<br/>move closure 与 Transferable 检查"]
  S0033["S0033<br/>最小 typed SSA IR 与 verifier"]
  S0034["S0034<br/>标量与控制流经 typed SSA lower 到 LLVM IR"]
  S0035["S0035<br/>聚合、class 分配与显式 drop/free 后端基元"]
  S0036["S0036<br/>顺序容器连续缓冲区与运行时基元"]
  S0038["S0038<br/>具体闭包环境与间接调用后端"]
  S0039["S0039<br/>本机目标文件、显式入口与首个链接链路"]
  S0040["S0040<br/>DWARF 源码行表与首个 LLDB 验收"]
  S0042["S0042<br/>标准库目标语言 bootstrap 闭环"]
  S0043["S0043<br/>标准 error() identity 与 abort 接线"]
  S0044["S0044<br/>标准 Pair 与 Result"]
  S0045["S0045<br/>单线程共享 Rc&lt;T&gt; owner"]
  S0052["S0052<br/>最小 project manifest 与本地 source-set provider"]
  S0054["S0054<br/>无依赖本地 project build/run"]
  S0055["S0055<br/>发布单文档 LSP 诊断"]
  S0056["S0056<br/>单文档语义跳转定义"]
  S0057["S0057<br/>保守、稳定且幂等的源码格式化器"]
  S0058["S0058<br/>提供 TextMate grammar 与回归 fixture"]
  S0059["S0059<br/>提供 Tree-sitter grammar 与 corpus"]
  S0060["S0060<br/>发布版本化机器可读诊断"]
  S0062["S0062<br/>修正顶层声明换行与分号分隔"]
  S0063["S0063<br/>解析 postfix 错误传播运算符 ?"]
  S0064["S0064<br/>解析窄化接口委托"]
  S0065["S0065<br/>按职责拆分 Parser 模块"]
  S0066["S0066<br/>保留数值字面量后缀身份"]
  S0067["S0067<br/>检查 callable 调用与实参契约"]
  S0068["S0068<br/>增加 Lexer / Parser 对抗组合矩阵"]
  S0069["S0069<br/>覆盖独立 Parser 入口对抗矩阵"]
  S0070["S0070<br/>锁定 Tree-sitter 与 Lexer 词表契约"]
  S0071["S0071<br/>执行 TextMate symbol 与 literal 契约"]
  S0072["S0072<br/>锁定 Pratt 运算符矩阵契约"]
  S0073["S0073<br/>锁定 Lexer 固定词与符号边界矩阵"]
  S0074["S0074<br/>覆盖 Parser 完整词法片段库存"]
  S0075["S0075<br/>覆盖 Parser lexical owner 语法位置矩阵"]
  S0076["S0076<br/>建立 Parser 已发布诊断 witness 矩阵"]
  S0077["S0077<br/>建立 Parser 非换行 trivia 等价矩阵"]
  S0078["S0078<br/>建立 Parser 结构性换行边界矩阵"]
  S0079["S0079<br/>建立 Parser 前缀截断恢复矩阵"]
  S0080["S0080<br/>建立 Parser 单 token 缺失恢复矩阵"]
  S0081["S0081<br/>建立 Parser 词法 poison 替换矩阵"]
  S0082["S0082<br/>建立 Parser 单 token 重复恢复矩阵"]
  S0083["S0083<br/>建立 Parser 词法 poison 插入矩阵"]
  S0084["S0084<br/>建立 Parser 相邻 token 交换矩阵"]
  S0085["S0085<br/>建立独立 Parser 入口前缀截断矩阵"]
  S0086["S0086<br/>建立独立 Parser 入口单 token 缺失矩阵"]
  S0087["S0087<br/>建立独立 Parser 入口单 token 重复矩阵"]
  S0088["S0088<br/>建立独立 Parser 入口词法 poison 替换矩阵"]
  S0089["S0089<br/>建立独立 Parser 入口词法 poison 插入矩阵"]
  S0090["S0090<br/>建立独立 Parser 入口相邻 token 交换矩阵"]
  S0091["S0091<br/>建立独立 Parser 入口 trivia 等价矩阵"]
  S0092["S0092<br/>建立独立 Parser 入口换行边界矩阵"]
  S0093["S0093<br/>强化独立 Parser 入口对抗产物不变量"]
  S0094["S0094<br/>强化 Parser token inventory 产物不变量"]
  S0095["S0095<br/>强化 Parser lexical-owner 矩阵产物不变量"]
  S0096["S0096<br/>强化 Parser diagnostic witness 产物不变量"]
  S0097["S0097<br/>强化 Parser trivia 等价矩阵产物不变量"]
  S0098["S0098<br/>强化 Parser line-break 边界产物不变量"]
  S0099["S0099<br/>强化完整文件恢复矩阵共享产物不变量"]
  S0100["S0100<br/>强化独立 Parser 入口 line-break 产物不变量"]
  S0101["S0101<br/>强化独立 Parser 入口 trivia 产物不变量"]
  S0102["S0102<br/>强化完整文件对抗矩阵 Lexer / Parser 产物不变量"]
  S0103["S0103<br/>强化 Lexer 固定词与符号边界矩阵产物不变量"]
  S0104["S0104<br/>强化 Pratt 运算符矩阵前端产物不变量"]
  S0105["S0105<br/>强化独立 Parser 入口对抗矩阵 Lexer 确定性"]
  S0106["S0106<br/>强化 token inventory Lexer 确定性"]
  S0107["S0107<br/>强化 Parser lexical-owner Lexer 确定性"]
  S0108["S0108<br/>强化 Parser diagnostic-witness Lexer 确定性"]
  S0109["S0109<br/>强化 Parser trivia 等价矩阵 Lexer 确定性"]
  S0110["S0110<br/>强化完整文件 line-break 边界 Lexer 确定性"]
  S0111["S0111<br/>强化完整文件 mutation 矩阵 Lexer 确定性"]
  S0112["S0112<br/>强化独立 Parser 入口 line-break Lexer 确定性"]
  S0113["S0113<br/>强化独立 Parser 入口 trivia Lexer 确定性"]
  S0114["S0114<br/>强化独立 Parser 入口 mutation Lexer 确定性"]
  S0115["S0115<br/>强化 fixture frontend 重复产物不变量"]
  S0116["S0116<br/>强化语法工具链 frontend 重复产物不变量"]
  S0117["S0117<br/>强化表达式 Parser 核心 suite 重复产物不变量"]
  S0118["S0118<br/>强化声明 Parser 核心 suite 重复产物不变量"]
  S0119["S0119<br/>强化 Block Parser 核心 suite 重复产物不变量"]
  S0120["S0120<br/>强化 Lambda Parser 核心 suite 重复产物不变量"]
  S0121["S0121<br/>强化 Call Argument Parser 核心 suite 重复产物不变量"]
  S0122["S0122<br/>强化局部解构 Parser 核心 suite 重复产物不变量"]
  S0123["S0123<br/>强化 Control-flow Parser 核心 suite 重复产物不变量"]
  S0124["S0124<br/>强化错误传播 Parser 核心 suite 重复产物不变量"]
  S0125["S0125<br/>强化 Class-family Parser 核心 suite 重复产物不变量"]
  S0126["S0126<br/>强化接口委托 Parser 核心 suite 重复产物不变量"]
  S0127["S0127<br/>强化隐式 Unit Parser 核心 suite 重复产物不变量"]
  S0128["S0128<br/>强化完整文件 Parser 核心 suite 重复产物不变量"]
  S0129["S0129<br/>强化 Lexer 核心 suite 重复产物不变量"]
  S0130["S0130<br/>强化名称解析 suite 的前端输入不变量"]
  S0131["S0131<br/>强化类型检查核心 suite 的前端输入不变量"]
  S0132["S0132<br/>强化 callable 类型 suite 的前端输入不变量"]
  S0133["S0133<br/>强化顺序容器类型 suite 的前端输入不变量"]
  S0134["S0134<br/>强化 copyability 类型 suite 的前端输入不变量"]
  S0135["S0135<br/>强化 Parser 私有算法测试的 Lexer 输入不变量"]
  S0136["S0136<br/>强化 Lexer / Parser 内部边界错误确定性"]
  S0137["S0137<br/>建立 Parser 非法 Lexeme 流拒绝矩阵"]
  S0138["S0138<br/>建立 Parser 非法 lexical-owner 流拒绝矩阵"]
  S0139["S0139<br/>建立 Parser 非法 recovery diagnostic 关联矩阵"]
  S0140["S0140<br/>锁定 Parser 的 Lexer diagnostic anchor 契约"]
  S0141["S0141<br/>锁定 Parser 的 Lexer diagnostic 流身份"]
  S0142["S0142<br/>锁定 Parser 的 lexical poison 诊断覆盖"]
  S0143["S0143<br/>锁定 Parser 的 Lexer diagnostic anchor 唯一性"]
  S0144["S0144<br/>建立 Parser UTF-8 后缀截断矩阵"]
  S0145["S0145<br/>建立 Parser UTF-8 内部区间删除矩阵"]
  S0146["S0146<br/>建立 Parser UTF-8 scalar 重复矩阵"]
  S0147["S0147<br/>建立 Parser UTF-8 scalar 相邻交换矩阵"]
  S0148["S0148<br/>建立 Parser UTF-8 scalar 替换矩阵"]
  S0149["S0149<br/>建立 Parser UTF-8 scalar 插入矩阵"]
  S0150["S0150<br/>建立 Lexer 大输入与深模式压力矩阵"]
  S0151["S0151<br/>建立 Parser 大平坦列表与恢复压力矩阵"]
  S0152["S0152<br/>锁定 Parser 递归预算的精确公开边界"]
  S0153["S0153<br/>锁定 Parser 四入口的调用者栈隔离"]
  S0154["S0154<br/>锁定 Lexer 深模式的小调用栈行为"]
  S0155["S0155<br/>建立 Parser 大规模 lexical-owner 压力矩阵"]
  S0156["S0156<br/>压力验证 Lexer 字符串错误向 Parser 的唯一传播"]
  S0157["S0157<br/>扩展 Parser 独立 lexical poison 变换矩阵"]
  S0158["S0158<br/>建立 Parser standalone lexical poison 压力矩阵"]
  S0159["S0159<br/>锁定 Parser lexical-owner 递归预算边界"]
  S0160["S0160<br/>建立 Lexer 超长非法 lexeme 压力矩阵"]
  S0161["S0161<br/>建立 Parser 超长词法错误桥接矩阵"]
  S0162["S0162<br/>建立 Parser 混合超长词法错误流矩阵"]
  S0163["S0163<br/>建立 Parser 混合超长可恢复词法错误流矩阵"]
  S0164["S0164<br/>建立 Parser 超长 UTF-8 换行恢复矩阵"]
  S0165["S0165<br/>建立 Parser 超长 UTF-8 嵌套换行恢复矩阵"]
  S0166["S0166<br/>建立 Parser 超长 UTF-8 Char 换行恢复矩阵"]
  S0167["S0167<br/>建立 Parser 超长非法数字边界矩阵"]
  S0168["S0168<br/>建立 Parser 超长块注释换行矩阵"]
  S0169["S0169<br/>建立 Parser 超长行注释边界矩阵"]
  S0170["S0170<br/>建立 Parser 大文件头压力矩阵"]
  S0171["S0171<br/>建立 Parser 超长文件头限定路径矩阵"]
  S0172["S0172<br/>建立 Parser 大文件头分隔与恢复矩阵"]
  S0173["S0173<br/>Lambda 参数契约 typed facts"]
  S0174["S0174<br/>overload lambda 候选隔离检查"]
  S0175["S0175<br/>修复调用实参 lambda 的 block 边界误判"]
  S0176["S0176<br/>迁移 borrow-default 参数契约"]
  S0177["S0177<br/>泛型 callable 实例化与实例 identity"]
  S0178["S0178<br/>检查 break / continue 词法目标"]
  S0179["S0179<br/>顺序容器借用迭代 typed plan"]
  S0180["S0180<br/>instance receiver typed facts"]
  S0181["S0181<br/>instance receiver ownership"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0183["S0183<br/>构造目标、实例化与 typed facts"]
  S0184["S0184<br/>名义构造与所有权 facts 到 SSA/LLVM lowering"]
  S0185["S0185<br/>声明型 type roots 的源码模块接纳边界"]
  S0186["S0186<br/>目标布局预检"]
  S0187["S0187<br/>跨文件 LSP 诊断与跳转定义"]
  S0188["S0188<br/>构造 Value delivery 与所有权效果"]
  S0189["S0189<br/>标准 println(String) 与最小 stdout 输出"]
  S0190["S0190<br/>公开单文件 kovenc build/run"]
  S0191["S0191<br/>instance receiver 与静态委托 lowering"]
  S0192["S0192<br/>一般 UTF-8 String runtime"]
  S0193["S0193<br/>单文件零参数 conventional main"]
  S0194["S0194<br/>参数化 main 与 argv owner"]
  S0195["S0195<br/>跨 callable Borrow 的 SSA/LLVM lowering"]
  S0196["S0196<br/>pointer-like nullable handle lowering"]
  S0197["S0197<br/>跨文件类型检查"]
  S0198["S0198<br/>跨文件所有权检查"]
  S0199["S0199<br/>多文件 compilation-unit native lowering"]
  S0201["S0201<br/>instance receiver mode Parser"]
  S0202["S0202<br/>nullable when 剩余域 typed facts"]
  S0203["S0203<br/>nullable when view 与 extraction 所有权"]
  S0204["S0204<br/>pointer-like nullable when lowering"]
  S0205["S0205<br/>非空断言 extraction typed facts"]
  S0206["S0206<br/>非空断言 Copy/Consume 所有权"]
  S0207["S0207<br/>pointer-like 非空断言 lowering"]
  S0208["S0208<br/>常量重新物化与所有权事实"]
  S0209["S0209<br/>关联常量 SSA/LLVM 重新物化"]
  S0210["S0210<br/>跨文件关联常量集成"]
  S0211["S0211<br/>顺序迭代 source/element loan 与退出清理"]
  S0212["S0212<br/>借用式顺序迭代 SSA/LLVM primitives"]
  S0213["S0213<br/>尾 lambda 调用 Parser"]
  S0214["S0214<br/>隐式 it lambda 参数"]
  S0215["S0215<br/>lambda body 隐式结果析构事实"]
  S0216["S0216<br/>MoveOnly control result 析构事实"]
  S0217["S0217<br/>lambda Value 参数入口析构事实"]
  S0218["S0218<br/>compilation-unit 普通替换赋值类型事实"]
  S0219["S0219<br/>compilation-unit 实例限定 runtime 字段布局事实"]
  S0220["S0220<br/>compilation-unit pointer-like nullable storage lowering"]
  S0221["S0221<br/>MoveOnly enum 空 case owner lowering"]
  S0222["S0222<br/>StaticSelf Value receiver 条件交付事实"]
  S0223["S0223<br/>StaticSelf Value receiver 条件交付 lowering"]
  S0224["S0224<br/>dependent inherited owner recipe lowering"]
  S0225["S0225<br/>参数增长型 runtime recipe 策略与 lowering"]
  S0226["S0226<br/>跨文件常量重新物化与所有权"]
  S0227["S0227<br/>跨文件常量 SSA 与 native 交付"]
  S0228["S0228<br/>Linux x86_64 本机目标与基线验收"]
  S0229["S0229<br/>扩展数值字面量值的端到端闭合"]
  S0230["S0230<br/>递归 Box enum 的 native 构造与析构"]
  S0231["S0231<br/>上下文 TypeRef 与严格调用试探一致性"]
  S0232["S0232<br/>原子置换原语的可信类型事实"]
  S0233["S0233<br/>Parser 工程合同的保全文档迁移"]
  S0234["S0234<br/>普通 block 的换行表达式边界"]
  S0235["S0235<br/>三项批准规则在真实 clone-first 基线启用"]
  S0236["S0236<br/>String.clone 显式深拷贝端到端"]
  S0237["S0237<br/>八阶段本地整合与交叉契约验证"]
  S0238["S0238<br/>Guide 勘误与可执行 Litmus 前端门禁"]
  S0239["S0239<br/>Linux CI 与双宿主定向回归门禁"]
  S0240["S0240<br/>整数具名位运算与取反端到端执行"]
  S0241["S0241<br/>Return 控制表达式操作数与单文件 enum 条件"]
  S0242["S0242<br/>调用点自动借用迁移"]
  S0243["S0243<br/>Instance receiver 两阶段借用"]
  S0244["S0244<br/>owned mutable root 原子 replace / swap"]
  S0245["S0245<br/>具体普通 class 的资源析构闭环"]
  S0246["S0246<br/>owned local 普通 class 一级字段 replace"]
  S0247["S0247<br/>跨文件类型基线与恢复事实闭合"]
  S0248["S0248<br/>列表式 Unit 容器的零大小存储"]
  S0249["S0249<br/>普通 owned-unit 封闭借用交接"]
  S0250["S0250<br/>封闭 unit 名称前缀 owner"]
  S0251["S0251<br/>LSP unit 消费共享名称快照"]
  S0252["S0252<br/>unit 基础所有权共享推进"]
  S0253["S0253<br/>共享单文件阶段门面"]
  S0254["S0254<br/>const owned-unit 封闭借用交接"]
  S0255["S0255<br/>中立 lowering error 与 String helper 边界"]
  S0256["S0256<br/>当前可执行教程与CLI完整输出合同"]
  S0257["S0257<br/>有界组合门禁去重与失败传播"]
  S0258["S0258<br/>五成员四条内部直接声明依赖门禁"]
  S0259["S0259<br/>真实LLVM可恢复发射失败与TLS恢复"]
  S0260["S0260<br/>unit planner/lower共享借用source查询"]
  S0261["S0261<br/>有限只读 iteration fact validator"]
  S0262["S0262<br/>原治理计划的当前教程覆盖补齐"]
  S0263["S0263<br/>跨文件字段可变性查询"]
  S0264["S0264<br/>Unit owned class 一级字段直接 Borrow"]
  S0265["S0265<br/>unit 顺序迭代前端事实"]
  S0266["S0266<br/>Koven native 地址与泄漏检测接线"]
  S0267["S0267<br/>Tree-sitter corpus 解析与必需 CI 门禁"]
  S0268["S0268<br/>unit 顺序迭代 native 与 M1A 程序贯通"]
end
S0001 --> S0002
S0001 --> S0004
S0001 --> S0005
S0002 --> S0003
S0002 --> S0004
S0002 --> S0005
S0002 --> S0006
S0002 --> S0055
S0002 --> S0141
S0002 --> S0164
S0002 --> S0165
S0002 --> S0166
S0002 --> S0167
S0002 --> S0168
S0002 --> S0169
S0002 --> S0170
S0002 --> S0171
S0002 --> S0172
S0003 --> S0005
S0003 --> S0006
S0003 --> S0055
S0003 --> S0060
S0003 --> S0076
S0003 --> S0096
S0003 --> S0108
S0003 --> S0139
S0003 --> S0140
S0003 --> S0142
S0003 --> S0143
S0004 --> S0005
S0004 --> S0007
S0005 --> S0006
S0005 --> S0115
S0006 --> S0007
S0006 --> S0057
S0006 --> S0066
S0006 --> S0068
S0006 --> S0070
S0006 --> S0071
S0006 --> S0073
S0006 --> S0074
S0006 --> S0075
S0006 --> S0077
S0006 --> S0079
S0006 --> S0080
S0006 --> S0081
S0006 --> S0082
S0006 --> S0083
S0006 --> S0084
S0006 --> S0085
S0006 --> S0086
S0006 --> S0087
S0006 --> S0088
S0006 --> S0089
S0006 --> S0090
S0006 --> S0091
S0006 --> S0092
S0006 --> S0093
S0006 --> S0094
S0006 --> S0095
S0006 --> S0097
S0006 --> S0098
S0006 --> S0099
S0006 --> S0100
S0006 --> S0101
S0006 --> S0102
S0006 --> S0103
S0006 --> S0104
S0006 --> S0105
S0006 --> S0106
S0006 --> S0107
S0006 --> S0108
S0006 --> S0109
S0006 --> S0110
S0006 --> S0111
S0006 --> S0112
S0006 --> S0113
S0006 --> S0114
S0006 --> S0116
S0006 --> S0117
S0006 --> S0118
S0006 --> S0119
S0006 --> S0120
S0006 --> S0121
S0006 --> S0122
S0006 --> S0123
S0006 --> S0124
S0006 --> S0125
S0006 --> S0126
S0006 --> S0127
S0006 --> S0128
S0006 --> S0129
S0006 --> S0135
S0006 --> S0136
S0006 --> S0137
S0006 --> S0138
S0006 --> S0139
S0006 --> S0140
S0006 --> S0141
S0006 --> S0142
S0006 --> S0143
S0006 --> S0144
S0006 --> S0145
S0006 --> S0146
S0006 --> S0147
S0006 --> S0148
S0006 --> S0149
S0006 --> S0150
S0006 --> S0151
S0006 --> S0154
S0006 --> S0155
S0006 --> S0156
S0006 --> S0157
S0006 --> S0158
S0006 --> S0159
S0006 --> S0160
S0006 --> S0161
S0006 --> S0162
S0006 --> S0163
S0006 --> S0164
S0006 --> S0165
S0006 --> S0166
S0006 --> S0167
S0006 --> S0168
S0006 --> S0169
S0006 --> S0170
S0006 --> S0171
S0006 --> S0172
S0007 --> S0008
S0007 --> S0066
S0007 --> S0069
S0007 --> S0072
S0007 --> S0085
S0007 --> S0104
S0007 --> S0152
S0007 --> S0153
S0008 --> S0009
S0008 --> S0069
S0008 --> S0085
S0008 --> S0118
S0009 --> S0010
S0009 --> S0011
S0009 --> S0016
S0009 --> S0069
S0009 --> S0085
S0009 --> S0119
S0010 --> S0012
S0010 --> S0120
S0010 --> S0136
S0010 --> S0175
S0010 --> S0213
S0010 --> S0214
S0011 --> S0012
S0011 --> S0014
S0011 --> S0127
S0011 --> S0136
S0012 --> S0013
S0012 --> S0121
S0012 --> S0175
S0012 --> S0176
S0012 --> S0213
S0013 --> S0014
S0013 --> S0122
S0014 --> S0015
S0014 --> S0016
S0014 --> S0017
S0014 --> S0018
S0014 --> S0057
S0014 --> S0058
S0014 --> S0059
S0014 --> S0062
S0014 --> S0068
S0014 --> S0074
S0014 --> S0075
S0014 --> S0076
S0014 --> S0077
S0014 --> S0078
S0014 --> S0079
S0014 --> S0080
S0014 --> S0081
S0014 --> S0082
S0014 --> S0083
S0014 --> S0084
S0014 --> S0094
S0014 --> S0095
S0014 --> S0096
S0014 --> S0097
S0014 --> S0098
S0014 --> S0099
S0014 --> S0102
S0014 --> S0106
S0014 --> S0107
S0014 --> S0108
S0014 --> S0109
S0014 --> S0110
S0014 --> S0111
S0014 --> S0116
S0014 --> S0128
S0014 --> S0144
S0014 --> S0145
S0014 --> S0146
S0014 --> S0147
S0014 --> S0148
S0014 --> S0149
S0014 --> S0151
S0014 --> S0152
S0014 --> S0153
S0014 --> S0155
S0014 --> S0156
S0014 --> S0157
S0014 --> S0158
S0014 --> S0159
S0014 --> S0161
S0014 --> S0162
S0014 --> S0163
S0014 --> S0164
S0014 --> S0165
S0014 --> S0166
S0014 --> S0167
S0014 --> S0168
S0014 --> S0169
S0014 --> S0170
S0014 --> S0171
S0014 --> S0172
S0014 --> S0213
S0015 --> S0025
S0015 --> S0058
S0015 --> S0059
S0016 --> S0017
S0016 --> S0021
S0016 --> S0063
S0016 --> S0078
S0016 --> S0123
S0016 --> S0124
S0016 --> S0178
S0016 --> S0179
S0017 --> S0020
S0017 --> S0026
S0017 --> S0064
S0017 --> S0065
S0017 --> S0078
S0017 --> S0125
S0017 --> S0126
S0017 --> S0201
S0018 --> S0019
S0018 --> S0025
S0018 --> S0026
S0018 --> S0055
S0018 --> S0056
S0018 --> S0130
S0018 --> S0179
S0018 --> S0214
S0019 --> S0020
S0019 --> S0022
S0019 --> S0026
S0019 --> S0027
S0019 --> S0034
S0019 --> S0043
S0019 --> S0067
S0019 --> S0131
S0019 --> S0173
S0019 --> S0178
S0019 --> S0179
S0019 --> S0202
S0019 --> S0205
S0019 --> S0214
S0019 --> S0218
S0020 --> S0021
S0020 --> S0022
S0020 --> S0023
S0020 --> S0026
S0020 --> S0027
S0020 --> S0032
S0020 --> S0067
S0020 --> S0177
S0020 --> S0179
S0020 --> S0180
S0020 --> S0183
S0020 --> S0185
S0020 --> S0197
S0020 --> S0218
S0020 --> S0219
S0021 --> S0033
S0021 --> S0034
S0021 --> S0197
S0021 --> S0202
S0022 --> S0023
S0022 --> S0027
S0022 --> S0028
S0022 --> S0067
S0022 --> S0134
S0022 --> S0177
S0022 --> S0179
S0022 --> S0183
S0022 --> S0205
S0023 --> S0030
S0023 --> S0036
S0023 --> S0133
S0023 --> S0179
S0025 --> S0052
S0025 --> S0187
S0025 --> S0197
S0025 --> S0210
S0025 --> S0250
S0026 --> S0208
S0026 --> S0209
S0026 --> S0210
S0027 --> S0028
S0028 --> S0044
S0028 --> S0045
S0028 --> S0176
S0028 --> S0203
S0028 --> S0206
S0028 --> S0208
S0029 --> S0030
S0029 --> S0032
S0029 --> S0033
S0029 --> S0034
S0029 --> S0035
S0029 --> S0043
S0029 --> S0181
S0029 --> S0188
S0029 --> S0195
S0029 --> S0198
S0029 --> S0203
S0029 --> S0206
S0029 --> S0208
S0029 --> S0211
S0029 --> S0215
S0029 --> S0216
S0029 --> S0217
S0030 --> S0036
S0030 --> S0198
S0030 --> S0211
S0032 --> S0038
S0032 --> S0177
S0032 --> S0181
S0032 --> S0198
S0032 --> S0211
S0032 --> S0214
S0032 --> S0215
S0032 --> S0217
S0033 --> S0034
S0033 --> S0035
S0033 --> S0039
S0033 --> S0040
S0033 --> S0042
S0033 --> S0186
S0034 --> S0035
S0034 --> S0038
S0034 --> S0039
S0034 --> S0040
S0034 --> S0042
S0034 --> S0182
S0034 --> S0185
S0034 --> S0191
S0034 --> S0195
S0034 --> S0199
S0034 --> S0204
S0034 --> S0207
S0034 --> S0209
S0034 --> S0212
S0034 --> S0228
S0035 --> S0036
S0035 --> S0038
S0035 --> S0039
S0035 --> S0044
S0035 --> S0045
S0035 --> S0184
S0035 --> S0186
S0035 --> S0191
S0035 --> S0195
S0035 --> S0199
S0035 --> S0220
S0035 --> S0225
S0035 --> S0248
S0036 --> S0182
S0036 --> S0186
S0036 --> S0199
S0036 --> S0212
S0036 --> S0248
S0038 --> S0039
S0038 --> S0186
S0038 --> S0191
S0038 --> S0199
S0039 --> S0040
S0039 --> S0042
S0039 --> S0043
S0039 --> S0184
S0039 --> S0189
S0039 --> S0190
S0039 --> S0191
S0039 --> S0199
S0039 --> S0207
S0039 --> S0209
S0039 --> S0228
S0040 --> S0228
S0042 --> S0043
S0042 --> S0044
S0042 --> S0045
S0042 --> S0185
S0042 --> S0189
S0042 --> S0190
S0042 --> S0192
S0043 --> S0189
S0043 --> S0190
S0043 --> S0192
S0045 --> S0195
S0045 --> S0196
S0052 --> S0054
S0055 --> S0056
S0055 --> S0060
S0055 --> S0187
S0056 --> S0187
S0058 --> S0071
S0058 --> S0116
S0059 --> S0070
S0060 --> S0054
S0062 --> S0015
S0062 --> S0065
S0062 --> S0078
S0062 --> S0115
S0062 --> S0172
S0063 --> S0017
S0063 --> S0065
S0063 --> S0124
S0064 --> S0020
S0064 --> S0065
S0064 --> S0126
S0064 --> S0201
S0065 --> S0137
S0065 --> S0138
S0065 --> S0139
S0065 --> S0140
S0065 --> S0141
S0065 --> S0142
S0065 --> S0143
S0066 --> S0019
S0067 --> S0023
S0067 --> S0027
S0067 --> S0043
S0067 --> S0056
S0067 --> S0131
S0067 --> S0132
S0067 --> S0173
S0067 --> S0174
S0067 --> S0176
S0067 --> S0177
S0067 --> S0180
S0067 --> S0183
S0067 --> S0205
S0067 --> S0214
S0068 --> S0069
S0068 --> S0073
S0068 --> S0079
S0068 --> S0093
S0068 --> S0102
S0068 --> S0144
S0068 --> S0145
S0068 --> S0146
S0068 --> S0147
S0068 --> S0148
S0068 --> S0149
S0069 --> S0072
S0069 --> S0074
S0069 --> S0075
S0069 --> S0076
S0069 --> S0080
S0069 --> S0085
S0069 --> S0086
S0069 --> S0087
S0069 --> S0088
S0069 --> S0089
S0069 --> S0090
S0069 --> S0091
S0069 --> S0092
S0069 --> S0093
S0069 --> S0105
S0070 --> S0116
S0072 --> S0104
S0073 --> S0074
S0073 --> S0077
S0073 --> S0082
S0073 --> S0084
S0073 --> S0094
S0073 --> S0103
S0073 --> S0129
S0073 --> S0150
S0073 --> S0160
S0073 --> S0166
S0073 --> S0167
S0073 --> S0168
S0073 --> S0169
S0074 --> S0075
S0074 --> S0079
S0074 --> S0081
S0074 --> S0094
S0074 --> S0106
S0075 --> S0076
S0075 --> S0080
S0075 --> S0081
S0075 --> S0095
S0075 --> S0107
S0075 --> S0155
S0075 --> S0159
S0075 --> S0163
S0076 --> S0077
S0076 --> S0096
S0076 --> S0108
S0077 --> S0078
S0077 --> S0083
S0077 --> S0091
S0077 --> S0097
S0077 --> S0101
S0077 --> S0109
S0077 --> S0113
S0078 --> S0079
S0078 --> S0092
S0078 --> S0098
S0078 --> S0100
S0078 --> S0110
S0078 --> S0112
S0078 --> S0164
S0078 --> S0165
S0078 --> S0172
S0079 --> S0080
S0079 --> S0085
S0079 --> S0099
S0079 --> S0111
S0079 --> S0144
S0079 --> S0145
S0080 --> S0081
S0080 --> S0082
S0080 --> S0084
S0080 --> S0086
S0081 --> S0082
S0081 --> S0083
S0081 --> S0088
S0081 --> S0148
S0081 --> S0157
S0082 --> S0083
S0082 --> S0084
S0082 --> S0087
S0082 --> S0146
S0082 --> S0149
S0083 --> S0084
S0083 --> S0089
S0083 --> S0157
S0084 --> S0090
S0084 --> S0147
S0085 --> S0086
S0085 --> S0087
S0085 --> S0088
S0085 --> S0089
S0085 --> S0090
S0085 --> S0091
S0085 --> S0092
S0085 --> S0093
S0085 --> S0114
S0085 --> S0144
S0085 --> S0145
S0086 --> S0087
S0087 --> S0146
S0087 --> S0149
S0088 --> S0148
S0088 --> S0157
S0089 --> S0149
S0090 --> S0147
S0091 --> S0101
S0091 --> S0113
S0092 --> S0100
S0092 --> S0112
S0093 --> S0094
S0093 --> S0095
S0093 --> S0096
S0093 --> S0097
S0093 --> S0098
S0093 --> S0099
S0093 --> S0100
S0093 --> S0101
S0093 --> S0102
S0093 --> S0103
S0093 --> S0104
S0093 --> S0105
S0093 --> S0114
S0093 --> S0117
S0093 --> S0118
S0093 --> S0119
S0093 --> S0120
S0093 --> S0121
S0093 --> S0122
S0093 --> S0123
S0093 --> S0124
S0093 --> S0125
S0093 --> S0126
S0093 --> S0127
S0093 --> S0128
S0093 --> S0129
S0093 --> S0130
S0093 --> S0135
S0093 --> S0168
S0093 --> S0169
S0093 --> S0170
S0093 --> S0171
S0093 --> S0172
S0094 --> S0095
S0094 --> S0106
S0095 --> S0107
S0095 --> S0155
S0095 --> S0156
S0095 --> S0159
S0095 --> S0161
S0095 --> S0162
S0095 --> S0163
S0095 --> S0166
S0095 --> S0167
S0096 --> S0108
S0097 --> S0109
S0098 --> S0110
S0098 --> S0164
S0098 --> S0165
S0099 --> S0111
S0099 --> S0144
S0099 --> S0145
S0099 --> S0146
S0099 --> S0147
S0099 --> S0148
S0099 --> S0149
S0100 --> S0110
S0100 --> S0112
S0100 --> S0164
S0100 --> S0165
S0101 --> S0113
S0103 --> S0105
S0103 --> S0106
S0103 --> S0107
S0103 --> S0108
S0103 --> S0109
S0103 --> S0110
S0103 --> S0111
S0103 --> S0112
S0103 --> S0113
S0103 --> S0114
S0103 --> S0115
S0103 --> S0116
S0103 --> S0117
S0103 --> S0118
S0103 --> S0119
S0103 --> S0120
S0103 --> S0121
S0103 --> S0122
S0103 --> S0123
S0103 --> S0124
S0103 --> S0125
S0103 --> S0126
S0103 --> S0127
S0103 --> S0128
S0103 --> S0129
S0103 --> S0150
S0103 --> S0160
S0103 --> S0166
S0103 --> S0167
S0103 --> S0168
S0103 --> S0169
S0103 --> S0170
S0103 --> S0171
S0103 --> S0172
S0106 --> S0129
S0110 --> S0164
S0110 --> S0165
S0111 --> S0114
S0111 --> S0144
S0111 --> S0145
S0111 --> S0146
S0111 --> S0147
S0111 --> S0148
S0111 --> S0149
S0111 --> S0157
S0112 --> S0164
S0112 --> S0165
S0114 --> S0144
S0114 --> S0145
S0114 --> S0146
S0114 --> S0147
S0114 --> S0148
S0114 --> S0149
S0114 --> S0157
S0115 --> S0117
S0115 --> S0118
S0115 --> S0119
S0115 --> S0120
S0115 --> S0121
S0115 --> S0122
S0115 --> S0123
S0115 --> S0124
S0115 --> S0125
S0115 --> S0126
S0115 --> S0127
S0115 --> S0128
S0115 --> S0129
S0115 --> S0130
S0117 --> S0136
S0117 --> S0151
S0117 --> S0152
S0127 --> S0136
S0128 --> S0129
S0128 --> S0130
S0128 --> S0151
S0128 --> S0170
S0128 --> S0171
S0128 --> S0172
S0129 --> S0130
S0129 --> S0135
S0129 --> S0136
S0129 --> S0140
S0129 --> S0141
S0129 --> S0142
S0129 --> S0143
S0129 --> S0150
S0129 --> S0154
S0129 --> S0158
S0129 --> S0160
S0129 --> S0168
S0129 --> S0169
S0130 --> S0131
S0130 --> S0132
S0130 --> S0133
S0130 --> S0134
S0130 --> S0135
S0131 --> S0132
S0135 --> S0136
S0135 --> S0137
S0135 --> S0138
S0135 --> S0139
S0135 --> S0140
S0135 --> S0141
S0135 --> S0142
S0135 --> S0143
S0135 --> S0151
S0135 --> S0152
S0135 --> S0155
S0136 --> S0137
S0136 --> S0153
S0140 --> S0156
S0140 --> S0158
S0140 --> S0161
S0140 --> S0162
S0140 --> S0163
S0142 --> S0157
S0143 --> S0144
S0144 --> S0145
S0145 --> S0146
S0146 --> S0147
S0147 --> S0148
S0148 --> S0149
S0149 --> S0150
S0150 --> S0151
S0150 --> S0154
S0150 --> S0158
S0150 --> S0159
S0150 --> S0160
S0150 --> S0164
S0150 --> S0165
S0150 --> S0166
S0150 --> S0167
S0150 --> S0168
S0150 --> S0169
S0150 --> S0170
S0150 --> S0171
S0150 --> S0172
S0151 --> S0152
S0151 --> S0155
S0152 --> S0153
S0152 --> S0159
S0153 --> S0154
S0154 --> S0155
S0154 --> S0160
S0155 --> S0156
S0155 --> S0162
S0155 --> S0163
S0155 --> S0165
S0155 --> S0166
S0155 --> S0167
S0156 --> S0157
S0157 --> S0158
S0157 --> S0160
S0157 --> S0166
S0157 --> S0167
S0158 --> S0159
S0158 --> S0161
S0160 --> S0164
S0160 --> S0165
S0160 --> S0168
S0160 --> S0169
S0163 --> S0166
S0165 --> S0167
S0168 --> S0170
S0170 --> S0171
S0170 --> S0172
S0173 --> S0174
S0173 --> S0176
S0173 --> S0214
S0174 --> S0033
S0174 --> S0034
S0174 --> S0197
S0175 --> S0213
S0176 --> S0029
S0176 --> S0180
S0176 --> S0201
S0177 --> S0033
S0177 --> S0034
S0177 --> S0174
S0177 --> S0180
S0177 --> S0183
S0177 --> S0191
S0177 --> S0197
S0177 --> S0219
S0178 --> S0179
S0179 --> S0182
S0179 --> S0211
S0180 --> S0181
S0180 --> S0191
S0180 --> S0222
S0180 --> S0224
S0181 --> S0191
S0181 --> S0222
S0181 --> S0224
S0182 --> S0265
S0182 --> S0268
S0183 --> S0044
S0183 --> S0045
S0183 --> S0184
S0183 --> S0188
S0184 --> S0044
S0184 --> S0045
S0184 --> S0182
S0184 --> S0189
S0184 --> S0190
S0184 --> S0191
S0184 --> S0192
S0184 --> S0199
S0184 --> S0204
S0184 --> S0207
S0184 --> S0220
S0184 --> S0221
S0185 --> S0044
S0185 --> S0045
S0185 --> S0184
S0185 --> S0209
S0186 --> S0184
S0186 --> S0212
S0186 --> S0225
S0187 --> S0251
S0188 --> S0044
S0188 --> S0045
S0188 --> S0184
S0188 --> S0221
S0189 --> S0190
S0189 --> S0192
S0189 --> S0209
S0190 --> S0054
S0190 --> S0193
S0191 --> S0223
S0191 --> S0224
S0191 --> S0225
S0192 --> S0182
S0192 --> S0194
S0192 --> S0199
S0192 --> S0209
S0192 --> S0236
S0193 --> S0054
S0193 --> S0194
S0194 --> S0054
S0195 --> S0182
S0195 --> S0191
S0195 --> S0192
S0195 --> S0196
S0195 --> S0199
S0195 --> S0212
S0196 --> S0199
S0196 --> S0204
S0196 --> S0207
S0196 --> S0220
S0197 --> S0187
S0197 --> S0198
S0197 --> S0210
S0197 --> S0214
S0197 --> S0215
S0197 --> S0216
S0197 --> S0217
S0197 --> S0218
S0197 --> S0219
S0197 --> S0249
S0197 --> S0265
S0198 --> S0187
S0198 --> S0199
S0198 --> S0214
S0198 --> S0215
S0198 --> S0216
S0198 --> S0217
S0198 --> S0221
S0198 --> S0226
S0198 --> S0227
S0198 --> S0249
S0198 --> S0265
S0199 --> S0054
S0199 --> S0220
S0199 --> S0221
S0199 --> S0226
S0199 --> S0227
S0199 --> S0249
S0201 --> S0180
S0202 --> S0203
S0202 --> S0204
S0203 --> S0204
S0205 --> S0206
S0205 --> S0207
S0206 --> S0207
S0208 --> S0209
S0208 --> S0226
S0208 --> S0227
S0209 --> S0226
S0209 --> S0227
S0210 --> S0226
S0210 --> S0227
S0211 --> S0182
S0212 --> S0182
S0212 --> S0248
S0213 --> S0214
S0215 --> S0216
S0215 --> S0217
S0216 --> S0217
S0218 --> S0220
S0219 --> S0191
S0219 --> S0220
S0219 --> S0224
S0219 --> S0225
S0222 --> S0223
S0224 --> S0225
S0226 --> S0227
S0228 --> S0266
S0244 --> S0245
S0244 --> S0246
S0246 --> S0263
S0249 --> S0250
S0249 --> S0252
S0249 --> S0254
S0250 --> S0251
S0250 --> S0252
S0250 --> S0253
S0251 --> S0252
S0251 --> S0253
S0252 --> S0253
S0252 --> S0254
S0253 --> S0254
S0254 --> S0255
S0263 --> S0264
S0263 --> S0265
S0263 --> S0268
S0264 --> S0268
S0265 --> S0268
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0001 | archive | [0001-bootstrap-cargo-workspace.md](0001-bootstrap-cargo-workspace.md) |
| SPEC-0002 | archive | [0002-source-span-foundation.md](0002-source-span-foundation.md) |
| SPEC-0003 | archive | [0003-structured-diagnostics.md](0003-structured-diagnostics.md) |
| SPEC-0004 | archive | [0004-indexed-ast-foundation.md](0004-indexed-ast-foundation.md) |
| SPEC-0005 | archive | [0005-language-fixture-harness.md](0005-language-fixture-harness.md) |
| SPEC-0006 | archive | [0006-deterministic-lexer.md](0006-deterministic-lexer.md) |
| SPEC-0007 | archive | [0007-pratt-expression-parser.md](0007-pratt-expression-parser.md) |
| SPEC-0008 | archive | [0008-declaration-parser.md](0008-declaration-parser.md) |
| SPEC-0009 | archive | [0009-block-statement-parser.md](0009-block-statement-parser.md) |
| SPEC-0010 | archive | [0010-lambda-literal-parser.md](0010-lambda-literal-parser.md) |
| SPEC-0011 | archive | [0011-implicit-unit-return.md](0011-implicit-unit-return.md) |
| SPEC-0012 | archive | [0012-callable-parameter-and-call-argument-parser.md](0012-callable-parameter-and-call-argument-parser.md) |
| SPEC-0013 | archive | [0013-local-val-destructuring-parser.md](0013-local-val-destructuring-parser.md) |
| SPEC-0014 | archive | [0014-complete-file-parser.md](0014-complete-file-parser.md) |
| SPEC-0015 | archive | [0015-package-import-parser.md](0015-package-import-parser.md) |
| SPEC-0016 | archive | [0016-control-flow-parser.md](0016-control-flow-parser.md) |
| SPEC-0017 | archive | [0017-class-family-parser.md](0017-class-family-parser.md) |
| SPEC-0018 | archive | [0018-single-file-name-resolution.md](0018-single-file-name-resolution.md) |
| SPEC-0019 | archive | [0019-basic-type-checking.md](0019-basic-type-checking.md) |
| SPEC-0020 | archive | [0020-nominal-generic-interface-types.md](0020-nominal-generic-interface-types.md) |
| SPEC-0021 | archive | [0021-when-exhaustiveness-smart-cast.md](0021-when-exhaustiveness-smart-cast.md) |
| SPEC-0022 | archive | [0022-copyable-structural-destructuring.md](0022-copyable-structural-destructuring.md) |
| SPEC-0023 | archive | [0023-sequential-container-types.md](0023-sequential-container-types.md) |
| SPEC-0025 | archive | [0025-multifile-package-import-name-resolution.md](0025-multifile-package-import-name-resolution.md) |
| SPEC-0026 | archive | [0026-associated-constant-evaluation.md](0026-associated-constant-evaluation.md) |
| SPEC-0027 | archive | [0027-variable-ownership-use-after-move.md](0027-variable-ownership-use-after-move.md) |
| SPEC-0028 | archive | [0028-conditional-copy-structural-move.md](0028-conditional-copy-structural-move.md) |
| SPEC-0029 | archive | [0029-call-loans-drop-points.md](0029-call-loans-drop-points.md) |
| SPEC-0030 | archive | [0030-sequential-container-element-ownership.md](0030-sequential-container-element-ownership.md) |
| SPEC-0032 | archive | [0032-move-closure-transferable.md](0032-move-closure-transferable.md) |
| SPEC-0033 | archive | [0033-typed-ssa-ir-verifier.md](0033-typed-ssa-ir-verifier.md) |
| SPEC-0034 | archive | [0034-scalar-control-flow-llvm-lowering.md](0034-scalar-control-flow-llvm-lowering.md) |
| SPEC-0035 | archive | [0035-aggregate-class-allocation-drop.md](0035-aggregate-class-allocation-drop.md) |
| SPEC-0036 | archive | [0036-sequential-container-runtime.md](0036-sequential-container-runtime.md) |
| SPEC-0038 | archive | [0038-closure-environment-codegen.md](0038-closure-environment-codegen.md) |
| SPEC-0039 | archive | [0039-native-object-entry-link.md](0039-native-object-entry-link.md) |
| SPEC-0040 | archive | [0040-dwarf-line-tables-lldb.md](0040-dwarf-line-tables-lldb.md) |
| SPEC-0042 | archive | [0042-standard-library-bootstrap.md](0042-standard-library-bootstrap.md) |
| SPEC-0043 | archive | [0043-standard-error-abort.md](0043-standard-error-abort.md) |
| SPEC-0044 | archive | [0044-standard-pair-result.md](0044-standard-pair-result.md) |
| SPEC-0045 | archive | [0045-shared-rc-owner.md](0045-shared-rc-owner.md) |
| SPEC-0052 | archive | [0052-minimal-project-manifest-source-set.md](0052-minimal-project-manifest-source-set.md) |
| SPEC-0054 | archive | [0054-local-project-build-run.md](0054-local-project-build-run.md) |
| SPEC-0055 | archive | [0055-single-document-lsp-diagnostics.md](0055-single-document-lsp-diagnostics.md) |
| SPEC-0056 | archive | [0056-single-document-definition.md](0056-single-document-definition.md) |
| SPEC-0057 | archive | [0057-conservative-source-formatter.md](0057-conservative-source-formatter.md) |
| SPEC-0058 | archive | [0058-textmate-grammar.md](0058-textmate-grammar.md) |
| SPEC-0059 | archive | [0059-tree-sitter-grammar.md](0059-tree-sitter-grammar.md) |
| SPEC-0060 | archive | [0060-machine-readable-diagnostics.md](0060-machine-readable-diagnostics.md) |
| SPEC-0062 | archive | [0062-top-level-declaration-separators.md](0062-top-level-declaration-separators.md) |
| SPEC-0063 | archive | [0063-postfix-error-propagation-parser.md](0063-postfix-error-propagation-parser.md) |
| SPEC-0064 | archive | [0064-interface-delegation-parser.md](0064-interface-delegation-parser.md) |
| SPEC-0065 | archive | [0065-parser-module-decomposition.md](0065-parser-module-decomposition.md) |
| SPEC-0066 | archive | [0066-numeric-literal-suffixes.md](0066-numeric-literal-suffixes.md) |
| SPEC-0067 | archive | [0067-callable-type-checking.md](0067-callable-type-checking.md) |
| SPEC-0068 | archive | [0068-frontend-adversarial-matrix.md](0068-frontend-adversarial-matrix.md) |
| SPEC-0069 | archive | [0069-parser-entry-adversarial-matrix.md](0069-parser-entry-adversarial-matrix.md) |
| SPEC-0070 | archive | [0070-tree-sitter-word-contract.md](0070-tree-sitter-word-contract.md) |
| SPEC-0071 | archive | [0071-textmate-lexical-contract.md](0071-textmate-lexical-contract.md) |
| SPEC-0072 | archive | [0072-pratt-operator-matrix.md](0072-pratt-operator-matrix.md) |
| SPEC-0073 | archive | [0073-lexer-boundary-matrix.md](0073-lexer-boundary-matrix.md) |
| SPEC-0074 | archive | [0074-parser-token-inventory-matrix.md](0074-parser-token-inventory-matrix.md) |
| SPEC-0075 | archive | [0075-parser-lexical-owner-placement-matrix.md](0075-parser-lexical-owner-placement-matrix.md) |
| SPEC-0076 | archive | [0076-parser-diagnostic-witness-matrix.md](0076-parser-diagnostic-witness-matrix.md) |
| SPEC-0077 | archive | [0077-parser-trivia-invariance-matrix.md](0077-parser-trivia-invariance-matrix.md) |
| SPEC-0078 | archive | [0078-parser-line-break-boundary-matrix.md](0078-parser-line-break-boundary-matrix.md) |
| SPEC-0079 | archive | [0079-parser-prefix-truncation-matrix.md](0079-parser-prefix-truncation-matrix.md) |
| SPEC-0080 | archive | [0080-parser-token-omission-matrix.md](0080-parser-token-omission-matrix.md) |
| SPEC-0081 | archive | [0081-parser-lexical-poison-replacement-matrix.md](0081-parser-lexical-poison-replacement-matrix.md) |
| SPEC-0082 | archive | [0082-parser-token-duplication-matrix.md](0082-parser-token-duplication-matrix.md) |
| SPEC-0083 | archive | [0083-parser-lexical-poison-insertion-matrix.md](0083-parser-lexical-poison-insertion-matrix.md) |
| SPEC-0084 | archive | [0084-parser-adjacent-token-transposition-matrix.md](0084-parser-adjacent-token-transposition-matrix.md) |
| SPEC-0085 | archive | [0085-parser-entry-prefix-truncation-matrix.md](0085-parser-entry-prefix-truncation-matrix.md) |
| SPEC-0086 | archive | [0086-parser-entry-token-omission-matrix.md](0086-parser-entry-token-omission-matrix.md) |
| SPEC-0087 | archive | [0087-parser-entry-token-duplication-matrix.md](0087-parser-entry-token-duplication-matrix.md) |
| SPEC-0088 | archive | [0088-parser-entry-lexical-poison-replacement-matrix.md](0088-parser-entry-lexical-poison-replacement-matrix.md) |
| SPEC-0089 | archive | [0089-parser-entry-lexical-poison-insertion-matrix.md](0089-parser-entry-lexical-poison-insertion-matrix.md) |
| SPEC-0090 | archive | [0090-parser-entry-adjacent-token-transposition-matrix.md](0090-parser-entry-adjacent-token-transposition-matrix.md) |
| SPEC-0091 | archive | [0091-parser-entry-trivia-invariance-matrix.md](0091-parser-entry-trivia-invariance-matrix.md) |
| SPEC-0092 | archive | [0092-parser-entry-line-break-boundary-matrix.md](0092-parser-entry-line-break-boundary-matrix.md) |
| SPEC-0093 | archive | [0093-parser-entry-adversarial-output-invariants.md](0093-parser-entry-adversarial-output-invariants.md) |
| SPEC-0094 | archive | [0094-parser-token-inventory-output-invariants.md](0094-parser-token-inventory-output-invariants.md) |
| SPEC-0095 | archive | [0095-parser-lexical-owner-output-invariants.md](0095-parser-lexical-owner-output-invariants.md) |
| SPEC-0096 | archive | [0096-parser-diagnostic-witness-output-invariants.md](0096-parser-diagnostic-witness-output-invariants.md) |
| SPEC-0097 | archive | [0097-parser-trivia-output-invariants.md](0097-parser-trivia-output-invariants.md) |
| SPEC-0098 | archive | [0098-parser-line-break-output-invariants.md](0098-parser-line-break-output-invariants.md) |
| SPEC-0099 | archive | [0099-parser-file-mutation-output-invariants.md](0099-parser-file-mutation-output-invariants.md) |
| SPEC-0100 | archive | [0100-parser-entry-line-break-output-invariants.md](0100-parser-entry-line-break-output-invariants.md) |
| SPEC-0101 | archive | [0101-parser-entry-trivia-output-invariants.md](0101-parser-entry-trivia-output-invariants.md) |
| SPEC-0102 | archive | [0102-frontend-adversarial-output-invariants.md](0102-frontend-adversarial-output-invariants.md) |
| SPEC-0103 | archive | [0103-lexer-boundary-output-invariants.md](0103-lexer-boundary-output-invariants.md) |
| SPEC-0104 | archive | [0104-pratt-operator-output-invariants.md](0104-pratt-operator-output-invariants.md) |
| SPEC-0105 | archive | [0105-parser-entry-adversarial-lexer-invariants.md](0105-parser-entry-adversarial-lexer-invariants.md) |
| SPEC-0106 | archive | [0106-parser-token-inventory-lexer-invariants.md](0106-parser-token-inventory-lexer-invariants.md) |
| SPEC-0107 | archive | [0107-parser-lexical-owner-lexer-invariants.md](0107-parser-lexical-owner-lexer-invariants.md) |
| SPEC-0108 | archive | [0108-parser-diagnostic-witness-lexer-invariants.md](0108-parser-diagnostic-witness-lexer-invariants.md) |
| SPEC-0109 | archive | [0109-parser-trivia-lexer-invariants.md](0109-parser-trivia-lexer-invariants.md) |
| SPEC-0110 | archive | [0110-parser-line-break-lexer-invariants.md](0110-parser-line-break-lexer-invariants.md) |
| SPEC-0111 | archive | [0111-parser-file-mutation-lexer-invariants.md](0111-parser-file-mutation-lexer-invariants.md) |
| SPEC-0112 | archive | [0112-parser-entry-line-break-lexer-invariants.md](0112-parser-entry-line-break-lexer-invariants.md) |
| SPEC-0113 | archive | [0113-parser-entry-trivia-lexer-invariants.md](0113-parser-entry-trivia-lexer-invariants.md) |
| SPEC-0114 | archive | [0114-parser-entry-mutation-lexer-invariants.md](0114-parser-entry-mutation-lexer-invariants.md) |
| SPEC-0115 | archive | [0115-fixture-frontend-output-invariants.md](0115-fixture-frontend-output-invariants.md) |
| SPEC-0116 | archive | [0116-grammar-bridge-frontend-invariants.md](0116-grammar-bridge-frontend-invariants.md) |
| SPEC-0117 | archive | [0117-parser-expression-suite-output-invariants.md](0117-parser-expression-suite-output-invariants.md) |
| SPEC-0118 | archive | [0118-parser-declaration-suite-output-invariants.md](0118-parser-declaration-suite-output-invariants.md) |
| SPEC-0119 | archive | [0119-parser-block-suite-output-invariants.md](0119-parser-block-suite-output-invariants.md) |
| SPEC-0120 | archive | [0120-parser-lambda-suite-output-invariants.md](0120-parser-lambda-suite-output-invariants.md) |
| SPEC-0121 | archive | [0121-parser-call-argument-suite-output-invariants.md](0121-parser-call-argument-suite-output-invariants.md) |
| SPEC-0122 | archive | [0122-parser-local-destructuring-suite-output-invariants.md](0122-parser-local-destructuring-suite-output-invariants.md) |
| SPEC-0123 | archive | [0123-parser-control-flow-suite-output-invariants.md](0123-parser-control-flow-suite-output-invariants.md) |
| SPEC-0124 | archive | [0124-parser-error-propagation-suite-output-invariants.md](0124-parser-error-propagation-suite-output-invariants.md) |
| SPEC-0125 | archive | [0125-parser-class-family-suite-output-invariants.md](0125-parser-class-family-suite-output-invariants.md) |
| SPEC-0126 | archive | [0126-parser-interface-delegation-suite-output-invariants.md](0126-parser-interface-delegation-suite-output-invariants.md) |
| SPEC-0127 | archive | [0127-parser-implicit-unit-suite-output-invariants.md](0127-parser-implicit-unit-suite-output-invariants.md) |
| SPEC-0128 | archive | [0128-parser-file-suite-output-invariants.md](0128-parser-file-suite-output-invariants.md) |
| SPEC-0129 | archive | [0129-lexer-core-suite-output-invariants.md](0129-lexer-core-suite-output-invariants.md) |
| SPEC-0130 | archive | [0130-name-resolution-frontend-input-invariants.md](0130-name-resolution-frontend-input-invariants.md) |
| SPEC-0131 | archive | [0131-type-checking-frontend-input-invariants.md](0131-type-checking-frontend-input-invariants.md) |
| SPEC-0132 | archive | [0132-callable-type-frontend-input-invariants.md](0132-callable-type-frontend-input-invariants.md) |
| SPEC-0133 | archive | [0133-container-type-frontend-input-invariants.md](0133-container-type-frontend-input-invariants.md) |
| SPEC-0134 | archive | [0134-copyability-type-frontend-input-invariants.md](0134-copyability-type-frontend-input-invariants.md) |
| SPEC-0135 | archive | [0135-parser-internal-lexer-input-invariants.md](0135-parser-internal-lexer-input-invariants.md) |
| SPEC-0136 | archive | [0136-frontend-internal-error-determinism.md](0136-frontend-internal-error-determinism.md) |
| SPEC-0137 | archive | [0137-parser-invalid-lexeme-stream-matrix.md](0137-parser-invalid-lexeme-stream-matrix.md) |
| SPEC-0138 | archive | [0138-parser-invalid-lexical-owner-matrix.md](0138-parser-invalid-lexical-owner-matrix.md) |
| SPEC-0139 | archive | [0139-parser-invalid-recovery-diagnostic-matrix.md](0139-parser-invalid-recovery-diagnostic-matrix.md) |
| SPEC-0140 | archive | [0140-parser-lexer-diagnostic-anchor-contract.md](0140-parser-lexer-diagnostic-anchor-contract.md) |
| SPEC-0141 | archive | [0141-parser-lexer-diagnostic-stream-identity.md](0141-parser-lexer-diagnostic-stream-identity.md) |
| SPEC-0142 | archive | [0142-parser-lexer-poison-diagnostic-coverage.md](0142-parser-lexer-poison-diagnostic-coverage.md) |
| SPEC-0143 | archive | [0143-parser-lexer-diagnostic-anchor-uniqueness.md](0143-parser-lexer-diagnostic-anchor-uniqueness.md) |
| SPEC-0144 | archive | [0144-parser-suffix-truncation-matrices.md](0144-parser-suffix-truncation-matrices.md) |
| SPEC-0145 | archive | [0145-parser-interior-deletion-matrices.md](0145-parser-interior-deletion-matrices.md) |
| SPEC-0146 | archive | [0146-parser-scalar-duplication-matrices.md](0146-parser-scalar-duplication-matrices.md) |
| SPEC-0147 | archive | [0147-parser-scalar-transposition-matrices.md](0147-parser-scalar-transposition-matrices.md) |
| SPEC-0148 | archive | [0148-parser-scalar-replacement-matrices.md](0148-parser-scalar-replacement-matrices.md) |
| SPEC-0149 | archive | [0149-parser-scalar-insertion-matrices.md](0149-parser-scalar-insertion-matrices.md) |
| SPEC-0150 | archive | [0150-lexer-large-input-mode-depth-stress.md](0150-lexer-large-input-mode-depth-stress.md) |
| SPEC-0151 | archive | [0151-parser-large-flat-recovery-stress.md](0151-parser-large-flat-recovery-stress.md) |
| SPEC-0152 | archive | [0152-parser-recursion-budget-boundaries.md](0152-parser-recursion-budget-boundaries.md) |
| SPEC-0153 | archive | [0153-parser-caller-stack-isolation.md](0153-parser-caller-stack-isolation.md) |
| SPEC-0154 | archive | [0154-lexer-small-stack-stress.md](0154-lexer-small-stack-stress.md) |
| SPEC-0155 | archive | [0155-parser-owner-rich-stress.md](0155-parser-owner-rich-stress.md) |
| SPEC-0156 | archive | [0156-parser-string-poison-stress.md](0156-parser-string-poison-stress.md) |
| SPEC-0157 | archive | [0157-parser-standalone-poison-matrices.md](0157-parser-standalone-poison-matrices.md) |
| SPEC-0158 | archive | [0158-parser-standalone-poison-stress.md](0158-parser-standalone-poison-stress.md) |
| SPEC-0159 | archive | [0159-parser-lexical-owner-recursion-boundaries.md](0159-parser-lexical-owner-recursion-boundaries.md) |
| SPEC-0160 | archive | [0160-lexer-long-invalid-lexeme-stress.md](0160-lexer-long-invalid-lexeme-stress.md) |
| SPEC-0161 | archive | [0161-parser-long-lexical-error-bridge.md](0161-parser-long-lexical-error-bridge.md) |
| SPEC-0162 | archive | [0162-parser-mixed-long-lexical-error-stream.md](0162-parser-mixed-long-lexical-error-stream.md) |
| SPEC-0163 | archive | [0163-parser-mixed-long-recoverable-error-stream.md](0163-parser-mixed-long-recoverable-error-stream.md) |
| SPEC-0164 | archive | [0164-parser-long-utf8-line-recovery.md](0164-parser-long-utf8-line-recovery.md) |
| SPEC-0165 | archive | [0165-parser-long-utf8-nested-line-recovery.md](0165-parser-long-utf8-nested-line-recovery.md) |
| SPEC-0166 | archive | [0166-parser-long-utf8-char-line-recovery.md](0166-parser-long-utf8-char-line-recovery.md) |
| SPEC-0167 | archive | [0167-parser-long-invalid-number-boundaries.md](0167-parser-long-invalid-number-boundaries.md) |
| SPEC-0168 | archive | [0168-parser-long-block-comment-line-breaks.md](0168-parser-long-block-comment-line-breaks.md) |
| SPEC-0169 | archive | [0169-parser-long-line-comment-boundaries.md](0169-parser-long-line-comment-boundaries.md) |
| SPEC-0170 | archive | [0170-parser-large-file-header-stress.md](0170-parser-large-file-header-stress.md) |
| SPEC-0171 | archive | [0171-parser-large-qualified-header-paths.md](0171-parser-large-qualified-header-paths.md) |
| SPEC-0172 | archive | [0172-parser-large-file-header-separators.md](0172-parser-large-file-header-separators.md) |
| SPEC-0173 | archive | [0173-lambda-parameter-contract-facts.md](0173-lambda-parameter-contract-facts.md) |
| SPEC-0174 | archive | [0174-overload-lambda-candidate-isolation.md](0174-overload-lambda-candidate-isolation.md) |
| SPEC-0175 | archive | [0175-call-argument-lambda-boundary.md](0175-call-argument-lambda-boundary.md) |
| SPEC-0176 | archive | [0176-borrow-default-parameter-contracts.md](0176-borrow-default-parameter-contracts.md) |
| SPEC-0177 | archive | [0177-generic-callable-instantiation.md](0177-generic-callable-instantiation.md) |
| SPEC-0178 | archive | [0178-jump-target-checking.md](0178-jump-target-checking.md) |
| SPEC-0179 | archive | [0179-sequential-iteration-typed-plan.md](0179-sequential-iteration-typed-plan.md) |
| SPEC-0180 | archive | [0180-instance-receiver-typed-facts.md](0180-instance-receiver-typed-facts.md) |
| SPEC-0181 | archive | [0181-instance-receiver-ownership.md](0181-instance-receiver-ownership.md) |
| SPEC-0182 | archive | [0182-sequential-for-lowering.md](0182-sequential-for-lowering.md) |
| SPEC-0183 | archive | [0183-constructor-typed-facts.md](0183-constructor-typed-facts.md) |
| SPEC-0184 | archive | [0184-nominal-construction-lowering.md](0184-nominal-construction-lowering.md) |
| SPEC-0185 | archive | [0185-declarative-type-roots-codegen.md](0185-declarative-type-roots-codegen.md) |
| SPEC-0186 | archive | [0186-target-layout-preflight.md](0186-target-layout-preflight.md) |
| SPEC-0187 | archive | [0187-multifile-lsp-diagnostics-definition.md](0187-multifile-lsp-diagnostics-definition.md) |
| SPEC-0188 | archive | [0188-constructor-ownership-effects.md](0188-constructor-ownership-effects.md) |
| SPEC-0189 | archive | [0189-standard-println-output.md](0189-standard-println-output.md) |
| SPEC-0190 | archive | [0190-public-single-file-build-run.md](0190-public-single-file-build-run.md) |
| SPEC-0191 | archive | [0191-instance-receiver-lowering.md](0191-instance-receiver-lowering.md) |
| SPEC-0192 | archive | [0192-general-string-runtime.md](0192-general-string-runtime.md) |
| SPEC-0193 | archive | [0193-conventional-zero-argument-main.md](0193-conventional-zero-argument-main.md) |
| SPEC-0194 | archive | [0194-parameterized-main-argv.md](0194-parameterized-main-argv.md) |
| SPEC-0195 | archive | [0195-interprocedural-borrow-lowering.md](0195-interprocedural-borrow-lowering.md) |
| SPEC-0196 | archive | [0196-nullable-handle-lowering.md](0196-nullable-handle-lowering.md) |
| SPEC-0197 | archive | [0197-multifile-type-checking.md](0197-multifile-type-checking.md) |
| SPEC-0198 | archive | [0198-multifile-ownership-checking.md](0198-multifile-ownership-checking.md) |
| SPEC-0199 | archive | [0199-multifile-native-lowering.md](0199-multifile-native-lowering.md) |
| SPEC-0201 | archive | [0201-instance-receiver-mode-parser.md](0201-instance-receiver-mode-parser.md) |
| SPEC-0202 | archive | [0202-nullable-when-flow-facts.md](0202-nullable-when-flow-facts.md) |
| SPEC-0203 | archive | [0203-nullable-when-ownership.md](0203-nullable-when-ownership.md) |
| SPEC-0204 | archive | [0204-pointer-nullable-when-lowering.md](0204-pointer-nullable-when-lowering.md) |
| SPEC-0205 | archive | [0205-non-null-assertion-facts.md](0205-non-null-assertion-facts.md) |
| SPEC-0206 | archive | [0206-non-null-assertion-ownership.md](0206-non-null-assertion-ownership.md) |
| SPEC-0207 | archive | [0207-pointer-non-null-assertion-lowering.md](0207-pointer-non-null-assertion-lowering.md) |
| SPEC-0208 | archive | [0208-constant-materialization-ownership.md](0208-constant-materialization-ownership.md) |
| SPEC-0209 | archive | [0209-associated-constant-lowering.md](0209-associated-constant-lowering.md) |
| SPEC-0210 | archive | [0210-multifile-associated-constants.md](0210-multifile-associated-constants.md) |
| SPEC-0211 | archive | [0211-sequential-iteration-ownership.md](0211-sequential-iteration-ownership.md) |
| SPEC-0212 | archive | [0212-borrowed-sequential-iteration-ssa.md](0212-borrowed-sequential-iteration-ssa.md) |
| SPEC-0213 | archive | [0213-trailing-lambda-call-parser.md](0213-trailing-lambda-call-parser.md) |
| SPEC-0214 | archive | [0214-implicit-it-lambda-parameter.md](0214-implicit-it-lambda-parameter.md) |
| SPEC-0215 | archive | [0215-lambda-body-result-drop-facts.md](0215-lambda-body-result-drop-facts.md) |
| SPEC-0216 | archive | [0216-control-result-drop-facts.md](0216-control-result-drop-facts.md) |
| SPEC-0217 | archive | [0217-lambda-value-parameter-drop-facts.md](0217-lambda-value-parameter-drop-facts.md) |
| SPEC-0218 | archive | [0218-compilation-unit-assignment-facts.md](0218-compilation-unit-assignment-facts.md) |
| SPEC-0219 | archive | [0219-compilation-unit-runtime-field-layout-facts.md](0219-compilation-unit-runtime-field-layout-facts.md) |
| SPEC-0220 | archive | [0220-compilation-unit-pointer-nullable-storage-lowering.md](0220-compilation-unit-pointer-nullable-storage-lowering.md) |
| SPEC-0221 | archive | [0221-move-only-empty-enum-case-lowering.md](0221-move-only-empty-enum-case-lowering.md) |
| SPEC-0222 | archive | [0222-static-self-value-delivery-facts.md](0222-static-self-value-delivery-facts.md) |
| SPEC-0223 | archive | [0223-static-self-value-delivery-lowering.md](0223-static-self-value-delivery-lowering.md) |
| SPEC-0224 | archive | [0224-dependent-inherited-owner-recipes.md](0224-dependent-inherited-owner-recipes.md) |
| SPEC-0225 | archive | [0225-parameter-growing-runtime-type-cycles.md](0225-parameter-growing-runtime-type-cycles.md) |
| SPEC-0226 | archive | [0226-unit-constant-materialization-ownership.md](0226-unit-constant-materialization-ownership.md) |
| SPEC-0227 | archive | [0227-unit-constant-native-lowering.md](0227-unit-constant-native-lowering.md) |
| SPEC-0228 | archive | [0228-linux-x86-64-native-host.md](0228-linux-x86-64-native-host.md) |
| SPEC-0229 | archive | [0229-extended-numeric-literal-values.md](0229-extended-numeric-literal-values.md) |
| SPEC-0230 | archive | [0230-recursive-boxed-enum-native.md](0230-recursive-boxed-enum-native.md) |
| SPEC-0231 | archive | [0231-contextual-type-ref-trials.md](0231-contextual-type-ref-trials.md) |
| SPEC-0232 | archive | [0232-ownership-primitive-type-facts.md](0232-ownership-primitive-type-facts.md) |
| SPEC-0233 | archive | [0233-parser-compiler-contracts.md](0233-parser-compiler-contracts.md) |
| SPEC-0234 | archive | [0234-block-newline-continuation.md](0234-block-newline-continuation.md) |
| SPEC-0235 | archive | [0235-approved-language-rules.md](0235-approved-language-rules.md) |
| SPEC-0236 | archive | [0236-explicit-string-clone.md](0236-explicit-string-clone.md) |
| SPEC-0237 | archive | [0237-local-integration.md](0237-local-integration.md) |
| SPEC-0238 | archive | [0238-guide-litmus-gate.md](0238-guide-litmus-gate.md) |
| SPEC-0239 | archive | [0239-linux-ci-gates.md](0239-linux-ci-gates.md) |
| SPEC-0240 | archive | [0240-integer-bitwise-execution.md](0240-integer-bitwise-execution.md) |
| SPEC-0241 | archive | [0241-return-control-operands.md](0241-return-control-operands.md) |
| SPEC-0242 | archive | [0242-automatic-borrow-call-migration.md](0242-automatic-borrow-call-migration.md) |
| SPEC-0243 | archive | [0243-receiver-two-phase-borrows.md](0243-receiver-two-phase-borrows.md) |
| SPEC-0244 | archive | [0244-root-ownership-primitives.md](0244-root-ownership-primitives.md) |
| SPEC-0245 | archive | [0245-resource-deinit.md](0245-resource-deinit.md) |
| SPEC-0246 | archive | [0246-direct-field-replace.md](0246-direct-field-replace.md) |
| SPEC-0247 | archive | [0247-multifile-baseline.md](0247-multifile-baseline.md) |
| SPEC-0248 | archive | [0248-unit-container-storage.md](0248-unit-container-storage.md) |
| SPEC-0249 | archive | [0249-owned-unit-borrowed-handoff.md](0249-owned-unit-borrowed-handoff.md) |
| SPEC-0250 | archive | [0250-unit-name-snapshot.md](0250-unit-name-snapshot.md) |
| SPEC-0251 | archive | [0251-lsp-unit-name-snapshot.md](0251-lsp-unit-name-snapshot.md) |
| SPEC-0252 | archive | [0252-basic-unit-ownership-driver.md](0252-basic-unit-ownership-driver.md) |
| SPEC-0253 | archive | [0253-single-file-analysis-facade.md](0253-single-file-analysis-facade.md) |
| SPEC-0254 | archive | [0254-const-owned-unit-borrowed-handoff.md](0254-const-owned-unit-borrowed-handoff.md) |
| SPEC-0255 | archive | [0255-neutral-lowering-support.md](0255-neutral-lowering-support.md) |
| SPEC-0256 | archive | [0256-current-tutorial.md](0256-current-tutorial.md) |
| SPEC-0257 | archive | [0257-bounded-integration-composition.md](0257-bounded-integration-composition.md) |
| SPEC-0258 | archive | [0258-direct-workspace-dependencies.md](0258-direct-workspace-dependencies.md) |
| SPEC-0259 | archive | [0259-recoverable-llvm-emission.md](0259-recoverable-llvm-emission.md) |
| SPEC-0260 | archive | [0260-shared-unit-source-query.md](0260-shared-unit-source-query.md) |
| SPEC-0261 | archive | [0261-finite-iteration-fact-validation.md](0261-finite-iteration-fact-validation.md) |
| SPEC-0262 | archive | [0262-current-tutorial-plan-coverage.md](0262-current-tutorial-plan-coverage.md) |
| SPEC-0263 | archive | [0263-unit-field-mutability.md](0263-unit-field-mutability.md) |
| SPEC-0264 | archive | [0264-unit-direct-field-borrow.md](0264-unit-direct-field-borrow.md) |
| SPEC-0265 | archive | [0265-unit-iteration-facts.md](0265-unit-iteration-facts.md) |
| SPEC-0266 | archive | [0266-native-sanitizer-wiring.md](0266-native-sanitizer-wiring.md) |
| SPEC-0267 | archive | [0267-editor-corpus-gate.md](0267-editor-corpus-gate.md) |
| SPEC-0268 | archive | [0268-unit-iteration-native.md](0268-unit-iteration-native.md) |
