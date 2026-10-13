# SPEC-0306: 混合 Value 参数的唯一来源借用返回

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收混合 Value/Borrow 参数的普通借用返回时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0306` |
| 所属 Phase | Phase 4 |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)、[Guide05](../../guide/05-declarations-callables.md) |
| 批准依据 | 2026-10-13 用户要求先收尾现行里程碑；在 M2B 已启用合同内继续实现，不转新集合 API |
| 前置 Spec | [SPEC-0305](../../archive/specs/0305-multi-parameter-borrow-result.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | 无新结果 ABI 或语言决定 |
| 阻塞项 | 本地验证与独立审查通过；外部发布另等具体授权 |
| 影响范围 | lang-codegen single/unit lowering、verifier、LLVM call adapter、正常 source/native 测试 |
| 语言语义变更 | 否 |

## 1. Goal 与输入

唯一来源仍是 Borrow 参数，其余形参可为 Borrow 或显式 own 所映射的 Value；在已有直接
具名调用、表达式体、受限 wrapper 与直线 borrow val 范围内，正确运输 Copyable 与既有
MoveOnly/Resource 参数。不是完整 M2B 完成，不引入新的参数或结果表示。

基于 actual main `bfdb00bd455d6847a7f14cf7391a198664ee6f1e`、tree
`795ac4346dce3c098a0acfd0ebcf198001b11341` 建立 `feature/spec-0306`。
两个 parent 为 `f7a4d15cecfa71b2fb8a4e9d4648d56e53702c05` 与
`b7a269d2e5589260cc7bc26fb3cfc28ca3fa5808`；完整 API commit JSON 恢复后逐对象核 SHA。
0305 已由 PR83 实际合并；[actual-main CI](https://github.com/Halckon/Koven/actions/runs/38350665268)
已由独立监视确认 success，不代替本片未来远端 CI。

## 2. 有界合同

- source 必须由 typed Parameter(i) 和 ownership origin 证明为唯一 Borrow 参数；真实 source
  storage 可为已支持字段，保留已有泛型与 Group。其余参数为已支持的 Borrow/Value。
- 按源码顺序仅求值每个 operand 一次，立即应用 typed 参数契约，按 parameter_index 装配
  ABI。Value 按现行 Copyability copy/move，不伪造 loan，也不强制 Copyable 新 SSA ID/glue。
- BorrowCall verifier 验证 Value 活性与 MoveOnly 唯一消费、全部 Loan 活性；结果 alias/parent
  仅延续真正 source。共享源码产生的 Program 必须通过原有 normal-exit 检查，不放宽 verifier。
- CallReturn 只结束非来源本次新建 loan，清理其 Borrow temporary；复用外部 loan 不提前结束。
  Value temporary 转交后 caller 不重复清理；source/result 继续到已验证终止点。
- callee 使用既有 FunctionEntry、CallReturn、ControlTransfer drop facts，保持纯内存 ASAP
  与 Resource 逆声明顺序；先得到返回 loan，再按事实清理非来源 owner，最后 BorrowReturn。
  ControlTransfer 使用原外层 expression-body ID，不用 ungroup 后的内层 ID代替。
- wrapper 可转交 Value owner 或对它作辅助 shared reborrow；后者在嵌套调用后终止，前者只由
  新 owner 负责清理。返回 source 链始终存活；source/result 终止先于其 owner drop。
- single 与真正跨文件 unit 一致，缺失事实结构化拒绝；LLVM 实参复用既有 Value/Loan 映射，
  结果继续返回真实 storage pointer，不复制/retain borrowed payload。

## 3. 非目标

Inout 参数/来源、receiver、block 返回、caller CFG/loop 运输、间接/callable 参数、同步嵌套
borrow-result 实参、短 range/新 carrier ABI、条件借用、多来源、完整 NLL 或 std 新 API。
保持当前 structured rejection 与真实 Span。需要新语义或长期 ABI 决定时暂停报告。

本地禁止故障注入/校准、sanitizer、坏 IR/allocator/LLVM 重写和 aggregate 驱动；只运行审阅过的
正常 source→frontend→真实 SSA/verifier→原版 LLVM/object/link/run。语言定义的非法源码与
Abort 不属于故障注入；坏 IR verifier 负例仅静态编写交 CI，不能在云上执行。

## 4. 实施与验收账本

先移植仓库外15组已复现 Value 源码红测，扩唯一来源、参数顺序、owner/loan/drop 正反例；
再最小接线两入口、verifier 与 LLVM，按同一选择转绿，最后验证直接消费者和既有0305回归。

| 验收项 | 命令/选择 | 实际结果 |
|---|---|---|
| actual main / Spec 冻结 | 精确 commit/tree/双parent、库存/docs/diff | 精确对象已恢复，0306未占用；冻结时docs658通过 |
| 正常 source 红测 | 保存的精确actual-main与659ba4d9 public API探针 | 25组Borrow源码在main双入口UnsupportedNode；普通generic对照在main已InvalidSsa；三组Copyable泛型在初轮实现双入口InvalidSsa |
| SSA/Loan/Value/drop | `cargo test --locked --offline -p lang-codegen --lib mixed_value_borrow -- --nocapture` | 15/15；25组Borrow与1组ordinary generic双入口真实Program、SSA/原版LLVM通过，含精确Group事实和正常错误源码 |
| native stdout/deinit | 上行的两个native矩阵 | 26组×single/跨文件unit共52次原版native成功，完整stdout与空stderr逐字节核对 |
| 错误来源/冲突/保留边界 | 新矩阵7项正常源码边界；`multi_parameter_borrow`及旧Inout精确选择 | 错误owner、存活来源move、own后复用、重叠实参顺序拒绝；Inout/nested-use/callable/CFG等保留边界通过 |
| 共享消费者 | `multi_parameter_borrow` 10项、`ssa::borrow_call_lowering_tests` 5项、`ssa::unit_lower_borrow_tests` 2项；普通Value/nullable前缀/借用结果与storage精确19项 | 36/36通过；覆盖pending owner分支、提前transfer、Group临时、字段与generic nullable native |
| 静态门禁 | `cargo fmt --all -- --check`；`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`；docs/尺寸/diff | fmt/严格clippy通过；docs658通过；Rust尺寸1060文件、38项旧欠账、无新增例外；diff通过 |
| 独立审查 | 生产b8c0f279；最终测试928c82f4 | 11组独立正常source×两入口22次native通过；928c82f4独立15/15回归通过，无未决阻断 |
| 双宿主CI/归档/合并 | 精确远端head/merge/main | 未发布、未运行 |

准备阶段在0305冻结树已完成33组双入口正常探针，其中15组Value均前端接受而在真实lowering
拒绝；6次native对照和4项正常source SSA/LLVM测试成功。它们仅是准备证据，不能冒充本片
实现或本基线新测试通过。原始源码、stdout/stderr与恢复材料保留在仓库外。

## 5. 交付边界

最终代码/测试输入为 `928c82f4467a76e761350e2f6f3c79358eb3dba2`、tree
`3dd1e48f4b579eed3f38e961d10605bb36ed855a`；生产修复与b8c0f279一致。
本地Linux、Rust1.96.0/LLVM21.1.8验证；10项构造SSA的verifier合同测试仅静态编写和编译，
未在云上执行，交后续CI。没有运行全workspace、aggregate、故障/校准或sanitizer。

本地分步提交并保留仓库外恢复包与原始红绿日志，发布交接继续保留；Spec 保持 in-progress，
独立审查后另请求具体发布授权。macOS/双宿主CI、远端发布/归档/合并未运行；本片不关闭
Inout、同步 nested-use 等其他 M2B 缺口。

## 6. 独立审查中的泛型修复

初轮20组通过后，独立审查补出无上界own T在T=Int的正常源码失败：frontend接受，
ControlTransfer的参数drop事实在具体化后仍生成Drop Int，真实SSA verifier以InvalidSsa拒绝。
同类型命名值、Copyable value class Pair及Int/Resource同generic并存形成补充回归；没有把
泛型Copyable缩成不支持边界，也没有放宽verifier。两入口共享drop消费者在取得typed模板与
concrete类型后，仅为具体化Copyable跳过义务，且在任何binding/temporary移除之前判断。

不含borrow-return的普通generic unused/forward在保存的actual-main探针中同样双入口失败，
是原有共享drop消费者缺口，不是本片新引入回归。本次同一最小修复自然覆盖它，增加普通
消费者对照，不改前端所有权语义或参数/结果ABI。生产修复提交b8c0f279；保存初轮659ba4d9
精确红测、修复后双入口绿测及独立22次native复验，最终固定回归包含未用、转交、读取后
返回与多次读取的Int/Resource实例，验收见上表。
