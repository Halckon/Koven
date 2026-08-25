# SPEC-0183：构造目标、实例化与 typed facts

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-183` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.29 §29](../guide/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029) |
| 批准依据 | 用户于 2026-08-25 明确启用 v0.29；实现状态仍按本 Spec 推进 |
| 前置 Spec | SPEC-0020、0022、0067、0177 `done` |
| 前置 ADR | 无；本 Spec 不改变既有编译阶段或 runtime ABI |
| 关联 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) |
| 阻塞项 | 无；前置 Spec 均已完成，guide 门禁已解除 |
| 影响范围 | `lang-frontend` name/type checker、typed model、L0143–L0144、Phase 2 fixtures；Architecture、Roadmap |
| 语言语义变更 | 否；只在候选 guide 获得版本级效力后实施其已确定语义 |

## 1. Goal

完成后，frontend 能为普通/泛型 class、value class、enum case 与 compiler-bound `Box`
构造发布唯一 target、完整实例类型、Value 参数映射和稳定 construction descriptor；非法目标、
推导失败或参数不匹配产生确定诊断，不再以普通 `DeferredReason::Call` 冒充成功。

## 2. 背景

SPEC-0020 已发布 nominal/field/enum-case identity，SPEC-0022 已发布 `Box` kind 与结构事实，
SPEC-0067/0177 已提供调用参数映射和局部泛型实例化算法；constructor 在本 Spec 完成前仍
保持 deferred。没有 construction descriptor，Phase 3 无法证明字段 Value delivery，
Phase 4 也不能把源码构造接到 SPEC-0035/ADR-0008 的聚合与 heap-owner 基元。

## 3. 范围与需求

- 名称/类型入口只识别 v0.29 §29 封闭的 source nominal、enum case 与环境绑定 intrinsic
  `Box`；interface/object/enum root 使用 L0143，源码同名 `Box` 保持普通 class identity。
- 普通 class 缺显式主构造器时发布零参数构造；value class 使用非空字段序列；payload case
  使用 payload symbol 序列；无 payload case 直接在解析后的 `Name` / `Member` 值表达式发布
  零参数 descriptor，不伪造成 function value 或空参数 `Call`。
- 完整显式类型实参复用 L0091 和既有 bound；完全省略时严格按“已定型非 lambda operand →
  独立确定的同 root complete expected result 补未决项”求解。expected type 不得包含本次
  constructor/deferred unknown，也不得来自尚未唯一的 overload candidate 或仍待本次构造反向
  完成的外层泛型 callable 参数。冲突/缺失使用 L0144，不接受部分实参、`_`、后续使用或普通
  callable 式返回推导。
- 参数名称、位置/命名混排、arity、mode 与类型筛选复用 SPEC-0067 的 L0120–L0123；全部字段、
  payload 与 `Box.element` 均规范化为 `ParameterMode::Value`，显式 `borrow` / `&` 不匹配。
- `TypedFile` 发布 source-ordered `ConstructionDescriptor`：expression、`ConstructionTarget`
  （Nominal/EnumCase/IntrinsicBox）、target + 完整类型实参 instance key、result `TypeId`，以及按
  参数声明顺序的 symbol/稳定名称/Value mode/source argument/evaluation index。
- descriptor 只能在 target、实例化、bound、映射和 operand 类型全部成功后原子提交；错误或
  deferred 节点不得留下可被 ownership/codegen 消费的半成品。construction descriptors 必须
  纳入 overload/lambda `TrialState` 的完整 snapshot/restore，失败或非唯一 trial 不得泄漏 nested
  construction fact。
- constructor 专项分派优先于 ordinary callable/function-value 分派；成功 construction 不发布
  `CallDescriptor`，type/case callee 或 receiver 也不属于后续 ownership/runtime 的求值 operand。
- `Result` payload 拼写变化只属于后续 SPEC-0044；本 Spec 不修改 Lexer、Parser 或标准库源码。

## 4. 非目标

- 不检查 copy/move、use-after-move、construction temporary、drop 或 loan；由 SPEC-0188 负责。
- 不生成 SSA/LLVM、enum tag/payload storage、allocation、Box、projection 或 destructuring；由
  SPEC-0184 负责。
- 不实现 secondary constructor、default/vararg、constructor/function overload 合并、function
  value/callable reference、factory、safe call、instance receiver、跨文件 visibility 或 import。
- 不改变普通泛型 callable 的 v0.29 §28 推导，不把 expected result 推导扩散到函数调用。

## 5. 验收标准

- [ ] compile-pass 覆盖非泛型/泛型 class 与 value class、显式/operand/expected-result 实例化、
      payload/无 payload enum case、显式/推导 `Box`、位置/命名参数和同名 source `Box`。
- [ ] compile-fail 覆盖 interface/object/enum-root target（L0143），无约束/冲突推导（L0144），
      arity/bound/参数 name-count-mode-type 复用诊断，并断言 primary/label `Span`；尚未决 overload
      的 candidate-local expected type 与未实例化外层泛型参数都不能补齐 constructor 类型实参。
- [ ] 白盒测试证明 target/instance/result/参数声明顺序/evaluation index 精确，成功重复运行确定，
      失败不发布 descriptor；generic no-payload case 只从独立确定的同 root expected type 得到
      实例，并且 descriptor 绑定裸 `Name` / `Member` expression。
- [ ] trial 回归证明失败/歧义 overload 丢弃 nested construction descriptor；成功 construction
      没有普通 `CallDescriptor`，Phase 3 可只遍历源码 operand 而不读取 type/case callee。
- [ ] ordinary generic call、container intrinsic、overload-lambda、field projection、when smart cast
      与 `Box<T>` kind 检查回归不变。
- [ ] `lang-frontend` 窄测试和 workspace 五项标准基线通过；Architecture/Roadmap/Spec 只陈述
      已实现 typed facts，production 文件遵守 1000 行软上限。

## 6. 技术方案与边界

- 在 `type_checking/checker/construction.rs` 增加先于 callable/container 的职责单一 constructor
  checker；把 callable 内现有 structural matcher 与 argument mapping 分别提取到
  `checker/type_inference.rs`、`checker/argument_mapping.rs` 作为 constructor-neutral 内部模块
  后共同复用，不复制两套确定性映射/推导算法，也不把 constructor 伪造成环境 function。
- `ConstructionTarget`、instance/argument descriptor 等 model 放入
  `type_checking/construction.rs` 并由既有门面最小 re-export，避免继续扩大集中 model/callable
  文件；frontend model 不包含 SSA/LLVM 类型，source target 使用已有 `NominalId` / `EnumCaseId`，
  intrinsic 使用编译器 identity。
- expected-result 只由 `check_expression` 当前入参及其“独立、完整”来源标记传入 constructor
  checker，不读取 parent AST 或 symbol initializer；多候选 trial 的 candidate-local expected
  不获得该标记，防止形成第二套全局约束求解。
- `ConstructionDescriptor` 的 table 与 `TrialState` 同步 snapshot/restore；无 payload case 和
  payload call 都经过同一原子提交入口。

## 7. 实施计划

1. [ ] 注册 L0143–L0144 与 construction model → 验证：catalog/model 单测。
2. [ ] 实现 target 识别、受控实例化与参数映射 → 验证：constructor type 专测正反矩阵。
3. [ ] 发布原子 descriptor 并接入 Phase 2 fixtures → 验证：白盒 facts、determinism、相邻回归。
4. [ ] 同步 Architecture/Roadmap/Spec，运行 workspace 基线 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | construction model/诊断、target/实例化/映射、测试与完成文档 | `feat(frontend): type nominal constructions (SPEC-0183)` |

## 9. 未决问题

- 无设计留白或版本门禁；可按持续 Goal 的站立授权进入实施。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过但有版本门禁 | 0020/0022/0067/0177 `done`；所需 identity、字段/case 顺序、expected-type 入口、trial 与泛型 matcher 已存在；已收紧 expected 来源、裸 no-payload descriptor 和 trial 回滚契约；v0.29 尚未启用 |
| 2026-08-25 v0.29 启用 | 通过 | 用户明确指定 v0.29 取代 v0.28；版本门禁解除，尚未开始实现 |
