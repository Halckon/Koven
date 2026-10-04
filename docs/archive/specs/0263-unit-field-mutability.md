# SPEC-0263: 跨文件字段可变性查询

> **性质**：有界变更合同 · **状态**：done · **读取时机**：追溯 M1A 跨文件字段首片验收时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-263` |
| 所属 Phase | Phase 3；现行语义修复 |
| 语言规范 | [Guide v0.40 所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户于 2026-10-04 要求提交/合并草稿后开始里程碑；允许按实际情况调整实施切片 |
| 前置 Spec | SPEC-0246 |
| 前置 ADR | 无 |
| 影响范围 | `lang-frontend` unit ownership；既有 field replace 消费者 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

普通 class 的字段声明和使用分属不同 source 时，unit ownership 按真实声明的 `var`/`val`
判定 Inout 权限；合法 owned local 的一级 var 字段 replace 不再被误报 L0134，
val 字段和 Borrow receiver 仍拒绝。字段身份保留 source，不能被同名或同局部 SymbolId 覆盖。

基线为文档 PR39 merge `b92593d6f5c8d11c64bac6d0a84811b6b36a7fa1`。
治理 PR38 `b32602e` 已在其中，双宿主 CI37188477831 成功；P2 成本预算等保留项继续开放。
用户此次明确进入里程碑，作为原 G0 等待条件的推进决定，不声明治理全部完成。
另一工作区的未提交网站/成本材料未接收。主干无 active Spec，编号核对至0262后分配0263。

## 2. 起草假设与本片范围

[M1A 起草材料](../../development/multifile-program-spec-draft.md)包含字段可变性、直接 Borrow、
unit for 三条缺口。本片只承接 A2 及其直接消费者；M1A 总退出仍保持原合同。
修复前 `Checker` 按使用 source 收集字段可变性，而 projection 使用跨 source 的字段身份，
导致其它文件声明的 var 缺席。现有 typed field signature 不含可变性；不为此新增公共产物。

复用同轮 validated names 的 source-qualified symbol 与各 ParsedFile 的字段声明，
在本次 unit 分析内收集一次完整字段表，供各 body checker 只读使用。
局部变量可变性仍按当前 source 收集；receiver 权限、loan 冲突、字段 replace 能力 gate 不放宽。
不按字符串名合并，不从后端推测权限，不复制单文件检查器。

## 3. 非目标

直接字段 Borrow lowering、unit for 的 typed/ownership/native、普通字段赋值 lowering、
新标准库 API、单/unit driver 合并、P2 成本实验及 frontend 全量均不属于本片。
三文件完整程序的首轮红测用于定位；其尚未通过不能隐去，也不阻止独立关闭 A2。

## 4. 单一验收账本

| ID | 合同与选择 | 实际结果 |
|---|---|---|
| F1 | 原三文件项目真实 CLI build，记录首个失败；更小输入分离 A2 | 修复前/后 exit1，native UnsupportedSource / UnsupportedNode，无产物；完整 M1A 仍未通过 |
| F2 | `multifile_ownership_checking field_mutability::`：跨文件 var、val/共享拒绝、同名/同局部 ID、inputs 置换、诊断 source/Span、失败不发布执行计划 | 有效红测2 passed/3 failed；修复后5 passed/0 failed/0 ignored、72 filtered；每项覆盖正反 inputs 顺序 |
| F3 | 完整 `multifile_ownership_checking`、`ownership_field_replace`、`ownership_primitives`、`multifile_constant_ownership`；codegen `--lib field_replace` | 前端77+13+14+20共124 passed，零失败/忽略/过滤；codegen25 passed、763 filtered，零失败/忽略，含真实 native 资源和 Abort 用例 |
| F4 | CLI `project_cli project_cross_file_var_field_replace_preserves_old_and_new_values` | 1 passed、9 filtered；val/var父绑定各经build/artifact/run，stdout精确为“旧值\n新值\ndone\n”、stderr空、exit0；无临时产物残留 |
| F5 | fmt、frontend/CLI all-targets 严格 Clippy、docs、inventory 测试、尺寸/diff；独立实现评审 | 全部通过：docs505，Python docs37+尺寸47；独立代码评审无阻断发现，三行增长例外已审阅；本机macOS arm64，LLVM21.1.8 |
| F6 | 精确 head PR/CI、Spec 归档、Architecture 同步 | 实现 head 67e6f8b 的 PR40 CI37192335440 全部10个job成功；双host新增六项各恰一次ok；Architecture同步，同PR归档，归档提交的最终CI另在PR核验后才合并 |

## 5. 实施与交付

1. 固定 F1、添加 F2 红测，确认错误确实来自字段查询而非 parser/type/其它限制。
2. 最小修复 unit 私有字段表，完成 F2/F3；验证普通 receiver 与失败回滚没有被放宽。
3. 用 F4 验证已有 native 消费者，F5 后独立评审并修正发现；记录 M1A 剩余缺口。
4. 一个逻辑提交包含本片源码、测试与账本，提交信息含 SPEC-0263；F6 按仓库 PR 闭环交付。

本片没有公开 API 或语言规则变更，不新增 ADR。后续发现额外行为缺口时另定切片，
不能悄悄吸收到这项可变性修复中。

## 6. 本地验证命令与边界

Cargo 使用 Rust1.96.0、`--locked --offline`，native 与 Clippy 设置
`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，命令串行执行：

```sh
cargo test --locked --offline -p lang-frontend --test multifile_ownership_checking field_mutability::
cargo test --locked --offline -p lang-frontend --test multifile_ownership_checking --test ownership_field_replace --test ownership_primitives --test multifile_constant_ownership --no-fail-fast
cargo test --locked --offline -p lang-codegen --lib field_replace
cargo test --locked --offline -p lang-cli --test project_cli project_cross_file_var_field_replace_preserves_old_and_new_values
cargo clippy --locked --offline -p lang-frontend -p lang-cli --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts/tests -p test_check_docs.py
python3 -m unittest discover -s scripts/tests -p test_check_rust_sizes.py
python3 scripts/check_rust_sizes.py --base origin/main
git diff --check
```

初写夹具曾因私有构造器、package/path 不匹配及诊断 Span 预期错误失败；修正夹具后才取得
F2 的有效行为红测，不能把这些测试编写错误算成生产缺陷。原程序源码从 M1A 文档提取到
临时目录，无第二份手写源码；独立 CLI 回归的最小程序只在测试文件维护。

尺寸门禁报告752份手写 Rust、45份超千行。`dataflow.rs` 1185→1188 仅为一次收集、
参数声明和借用传递三行；收集职责位于692行的 `traversal.rs`。保留原baseline、增加精确有限
例外及复查条件，不为三行搬迁无关算法，不清除历史欠账。独立评审核对 source identity、
receiver/Rc/一级字段 gate、回滚和 CLI oracle，未运行或代替上述测试。

未运行 frontend 全量、LSP 全量、性能实验或本地 Linux；无公开类型变化，未额外执行
workspace check。远端所选双宿主结果由 F6 另记，不把本地窄测称为全部支持。

## 7. 首轮双宿主与归档

[PR40](https://github.com/Halckon/Koven/pull/40) 实现 head
`67e6f8b27b3cd6b55958cc69d7c7b5eef6406d17` 的
[CI37192335440](https://github.com/Halckon/Koven/actions/runs/37192335440) completed/success，
10个必需job均实际成功。macOS14与Ubuntu24.04的workspace check/Clippy及bounded composition
执行成功；独立读取两份 Targeted Tests 原始job日志，新 `field_mutability::` 五项与
`project_cross_file_var_field_replace_preserves_old_and_new_values` 每host各恰一次 `ok`。
新增用例没有ignore；既有macOS LLDB ignore保持，不以平台过滤或跳过冒充覆盖。

本次仅迁移Spec、同步索引/inventory/生成DAG和状态链接，没有再改实现、测试、依赖或工具链。
归档后的精确head仍需PR最终门禁通过才合并，不把旧head绿灯替代新head检查。
独立代码评审及文档复核均已完成；M1A直接字段Borrow与unit for的独立最小CLI复现仍均为
exit1 / native UnsupportedNode，完整应用尚未通过，不在本片归档中关闭。
