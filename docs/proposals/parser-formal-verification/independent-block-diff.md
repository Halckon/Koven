# F4a：普通块表达式同语料差分原型

> **性质**：非规范验证账本 · **状态**：离线有界原型已实跑，未转为持久门禁 · **读取时机**：准备独立参考文法 DiffTest 的首个 Spec 时 · **唯一真源**：本页维护同语料比较边界、正反计数及模板推导

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47。
复用 [F3c 规则映射与词法流](token-stream-probe.md)。入口仅 `parse_block`，结构限 block、expression statement、
名称、前缀减号、二元减法和普通成员访问；不含 D1–D3。

## 比较合同

- 两路读取完全相同的 UTF-8 源码。生产路从公开 `parse_block` 提取有序元素及真实名称，显式匹配运算符和子边；
  不支持的可达节点、进程失败、超时或 Parser 内部失败均使运行失败，不记为语法拒绝。
- 参考路使用 Lark 1.2.2 Earley 和真实 `Token | NL | EOF`，独立决定绑定和分隔。两路共享 Lexer 实现，
  不复用生产 Parser 的 stop、continuation 或 AST；手填预期从 Guide 及 F3c 的窄映射给出。
- 投影可写成 `(block (sub (id a) (member (id b) (id c))))`。保留根、元素顺序、操作数方向、名称及绑定；
  不保留 arena ID 或绝对位置。生产路另验 source identity、节点/marker 范围包含关系及真实 `-`、`.`，
  对本批无外围 trivia 的接受输入核对整个块 Span。消费通过入口约束和尾部反例作证，不用 Span 反推出 cursor；
  参考路文法明确消费 EOF。词法 Span 和 owner 继续独立核对，不在树中替代。

## 实跑结果

macOS AArch64，仓库 Rust 1.96.0、Python 3.15.2 / Lark 1.2.2；探针未入仓库。

| 证据 | 结果 |
|---|---|
| F3c 16 个固定输入 | 11 个接受样例的两路投影精确一致，并符合手填预期；另有 2 个同类语法拒绝：`{a;\n.b}` 生产 L0029、`{a}b` 生产 L0013；2 个词法拒收及 1 个范围外输入未送入生产差分 |
| 有界模板推导 | 在 `{a-b.c}`、`{-a.b-c}`、`{a-b-c}`、`{a.b;-c}` 四种模板的三个名称槽各填 `a` 或 `b`，共 32 个不同输入；同一符号名可重复。全部接受，精确投影等于独立模板的 `sub(a,member(b,c))`、`sub(neg(member(a,b)),c)`、`sub(sub(a,b),c)`、`[member(a,b),neg(c)]` |
| 位置平移对照 | `{a\n-b}` 和 `{\n a\n-b}` 位置不同，`NL` 的字节/owner 预期各自通过，归一投影相同；不是跨 source identity 测试 |
| 差分负对照 | 删除参考路 `a\n-b` 的 `NL`、反转生产投影元素顺序、改写一个生产投影名称；三种不同错误均被比较器拒绝 |
| 词法投影负对照 | 复跑 F3c 的 4 种注入，仍全部检出；其中成员访问的 `NL` 漏发和 CRLF Span 截短不能仅靠归一树发现 |
| 生产回归 | `cargo test --locked -p lang-frontend --test parser_block_line_continuation --test parser_expression -- --quiet`：85 passed，0 failed、0 ignored、0 filtered |

生成是固定模板与枚举，不是完整文法推导穷举。两例语法拒绝未对齐恢复 AST，也没有要求参考路产生生产诊断码。
本次仅证明这些样例，未宣称整套文法 soundness、completeness、无歧义或资源安全；未启用 Guide 草案，
未新增依赖，未运行远端 CI。

## 后续决策

若要持久化，先在独立 Spec 中固定这四个模板、两条拒绝边界、四条词法故障和三条差分故障，
并决定离线参考运行方式与跨宿主复现合同。未完成依赖治理前不装入 required CI；不把临时本机路径
当作复现入口。F4b 的 Tree-sitter 支持交集及证据要求仍是独立切片。
