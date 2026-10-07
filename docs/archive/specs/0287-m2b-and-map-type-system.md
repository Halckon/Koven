# SPEC-0287: M2B 通用借用合同冻结与 Map 键值容器类型系统基础 (`Map<K, V>` / `MutableMap<K, V>`)

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或评审 M2B 通用借用合同与 Map 容器前端类型系统时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-0287` |
| 所属 Phase | Phase 1 规范与语法；Phase 2 类型检查、内建能力与符号绑定；Phase 3 所有权分析与 drop planning |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合、索引与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户推进里程碑中 M2B 工作及 Map 实现授权；现行 Guide v0.41 §12 键值容器所有权规范 |
| 前置 Spec | SPEC-0286 编译单元局部解构已合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`（能力 trait、类型模型、单文件与多文件类型检查器、所有权 drop planner） |
| 语言语义变更 | 是（在 Guide v0.41 §12 正式启用 `Map` 与 `MutableMap` 所有权规范，冻结 M2B 通用借用合同方案 A） |

## 1. Goal

推进里程碑 M2B（通用借用访问结果）与 M3B（Map 键值集合）基础能力落地：
1. **M2B 合同冻结**：正式冻结 M2B 方案 A（声明端 `borrow V from receiver` / 局部 `borrow val`）作为通用借用访问结果的基础设计，闭合 B01–B15 判定规则，明确区分 owned copy/move 与借用查询结果；
2. **Map 规范启用**：在 Guide v0.41 §12 中将 `Map` 与 `MutableMap` 升格为现行规范，确立键值映射容器的独占 owning 模型；
3. **前端类型系统落地**：
   - 增加编译器内建 capability `Hashable`，首版内建支持标量与基础类型（`Int`, `Boolean`, `Char`, `String`）；
   - 增加内建类型构造器 `IntrinsicTypeConstructor::Map` 与 `IntrinsicTypeConstructor::MutableMap`（支持两个类型实参 `<K, V>`）；
   - 增加内建工厂函数 `mapOf` 与 `mutableMapOf`；
   - 在单文件与编译单元类型检查器中支持双类型实参解析，验证 `K : Hashable` 与 `V` 可结构化存储；
   - 支持只读属性 `map.size`、变异方法 `mutableMap.put(key, value)`（及下标赋值脱糖）、`mutableMap.remove(key)` 以及查询方法 `map.get(key)` / `map[key]`（Copyable 返回 `V?`，MoveOnly 引导使用 M2B 借用查询）；
   - 所有权检查将 `Map` 与 `MutableMap` 纳入独占 owning container，正确规划其作用域退出 drop 事实。

## 2. 背景与需求

根据里程碑规划（`docs/development/post-governance-milestones.md`）：
- M2B 目标在于解决容器元素/字段非 owning 借用查询的局部绑定与受控转发问题，消除不必要的隐式 clone 与 move；
- M3B Map 集合要求建立完整的键值集合语义，以服务于词频统计等实用程序（M1B）；
- 现行 Guide 中曾将 `Map` / `MutableMap` 列为未启用语义，导致类型系统缺少内建 Map 识别。
本 Spec 完成 M2B 的方案收敛与 Map 前端类型系统全部基础设施，为后续 SSA lowering 与运行时哈希表提供完备的前端类型与所有权保证。

## 3. 技术方案

1. **类型模型与符号环境 (`crates/lang-frontend/src/type_checking/`)**：
   - `Capability::Hashable` 加入预声明能力列表；
   - `IntrinsicTypeConstructor` 增加 `Map` 与 `MutableMap`，其 `expected_type_argument_count()` 为 2；
   - `IntrinsicCallable` 增加 `MapOf` 与 `MutableMapOf`；
   - `standard_environments` 绑定对应符号与类型。
2. **类型检查与能力判定**：
   - 实现 `is_hashable_type`：`Int`、`Boolean`、`Char`、`String` 返回 true；普通 class、容器、未受限类型返回 false；
   - `resolve_intrinsic_segment` 与编译单元 `instantiate_intrinsic_type` 根据构造器实参需求检查实参数量（2 个）；
   - 验证第一个实参 `K` 满足 `is_hashable_type`，若不满足发射诊断 `L0136`；第二个实参 `V` 满足 `is_structurally_storable_type`；
   - 实现 Map 容器操作的类型检查：
     - `map.size` -> `Int`；
     - `mutableMap.put(key, value)` -> Inout receiver，参数 `key: K`，`value: V`（按值转移）；
     - `mutableMap[key] = value` -> 语法糖脱糖并校验；
     - `map.get(key)` / `map[key]` -> Borrow `key: K`；若 `V: Copyable` 返回 `V?`；若 `V: MoveOnly` 拒绝并提示借用查询；
     - `mutableMap.remove(key)` -> Inout receiver，Borrow `key: K`，返回 `V?`（或 `V`）；
3. **所有权分析**：
   - `Map` 与 `MutableMap` 为 `MoveOnly` 独占 owning 容器；
   - 构造与绑定注册为有效局部变量，并在作用域退出或消费时生成正确的 drop 义务。

## 4. 非目标

- 本 Spec 不包含 Phase 4 的 SSA/LLVM lowering 和实际运行时哈希表 C/LLVM 代码（留作紧随其后的后续 Spec）；
- 不开放用户自定义类型的 `Hashable` 实现（v1 仅限内建支持标量与 String）；
- 不包含开放式迭代器或高阶函数（`map`, `filter`）。

## 5. 验收标准

- [x] G1: Guide v0.41 §12 正式启用 `Map` 与 `MutableMap` 规范，冻结 M2B 方案 A 设计决定。
- [x] G2: 前端正确识别 `Map<K, V>` 与 `MutableMap<K, V>` 类型引用，精准校验 2 个类型实参及 `K: Hashable` 约束。
- [x] G3: 非 Hashable 键类型（如普通 class 或 List）触发确定性结构化诊断（L0161）。
- [x] G4: 正确识别 `mapOf()`、`mutableMapOf()` 构造器及 `size`、`put`、`get`、`remove` 操作的签名与类型规则。
- [x] G5: Copyable `V` 支持 `V?` 查询，MoveOnly `V` 按值查询被正确拦截（L0136）。
- [x] G6: 全套 Rust 尺寸护栏（`check_rust_sizes.py`）及所有文档门禁 100% 绿灯。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - 运行 `python3 scripts/check_docs.py`，全绿通过（commit `54fc531`）。
- **Phase 2 (前端实现)**:
  - 运行 `cargo check --workspace`，全工作区无警告编译通过。
  - 审阅并更新 `scripts/rust-size-policy.json`，确保无未受控超限。
- **Phase 3 (测试与门禁)**:
  - 新增专用集成测试文件 `crates/lang-frontend/tests/type_map.rs`（11 个场景全绿）；
  - 运行 `cargo test -p lang-frontend`，整库所有测试及文档测试全绿通过；
  - 运行 `cargo clippy --all-targets`，零警告通过；
  - 运行 `cargo fmt --check`，格式规范检查通过；
  - 运行 `python3 scripts/check_rust_sizes.py --base origin/main`，865 个 Rust 文件尺寸检查 100% 绿灯（commit `f1ec58f`）。
- **Phase 4 (归档与 PR 闭环)**:
  - 归档本 Spec 至 `docs/archive/specs/0287-m2b-and-map-type-system.md`；
  - 更新 active/archive README、`scripts/check_docs.py`，重建 DAG 拓扑；
  - 提交 PR 并完成双宿主 CI 自动化验证闭环合入。
