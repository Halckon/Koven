# SPEC-0464：Native 接口/委托与接收者测试领域归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 native 测试根尺寸欠账时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0464` |
| 所属 Phase | Phase 4，native object/link/run测试组织 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续下一批；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0463](0463-ownership-operation-and-lowering-domains.md)，同批输入4df265b1 |
| 前置 ADR | [ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md) |
| 影响范围 | native/unit_tests.rs及两个私有测试领域模块、对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整测试领域

1. 14 项接口/继承/委托测试移入私有 `native/unit_interface_delegation_tests.rs`：interface default
   与 super static 调用、abstract requirement override、继承与无关默认实现、list 与
   parameter-independent/dependent inherited owner recipe、borrow-only 委托、same-requirement
   与 identity-changing 委托链、generic interface owner/callable 委托、parameter-independent 与
   nested generic nominal 委托、`Inout` 与 value interface default。
2. 10 项类接收者/inline 所有权测试移入私有 `native/unit_class_receiver_tests.rs`：object
   atomically replace、direct member receiver 顺序与唯一 drop、`Inout` 载荷变更、MoveOnly 载荷
   替换、stateless object 借用、inline `Inout` 只读/写回、MoveOnly inline take、inline 可复制
   字段变更与嵌套 drop glue。

原 `#[test]` 属性、正文、fixture 源码与断言逐字节保全；FQN 只增加 `interface_delegation_tests`
或 `class_receiver_tests` 一层。根保留全部 imports、`TestDirectory`、
`assert_no_sibling_temporary` 及子模块加载，子模块以私有 `#[path]` + `use super::*` 继承原绑定。

## 2. 非目标

不改生产 native/LLVM/SSA 代码与任何断言、stdout 或对象/链接身份；不新增 ignored 测试。根与
新文件均不超 1000，只删除 `native/unit_tests.rs` baseline/exception 两项，其他 policy 逐 JSON
相同。不导入未合 #427，不为凑数删除合理保留模块。

## 3. 验收

- [x] R1：24 项测试完整保全，旧完整 FQN 无副本，原根剩余 import 不动。
- [x] R2：逐片定向原领域GREEN→移动→GREEN（真实 native link/run），本地提交。
- [x] R3：与0463成组统一双配置组合、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [x] R4：独立完整精确提交区间审查；Architecture/账本后一次push同PR。
- [ ] R5：精确新头官方CI另验；保持active/Draft，不归档或合入main。

## 4. 真实证据

| 切片 | 真实执行 | 尺寸 |
|---|---|---|
| 接口/委托 14 项 | 移动后 **14/0/0**，逐 leaf 名与基线子集一一对应，唯一模块层 `interface_delegation_tests` | 1953→1232，新文件 741 |
| 类接收者 10 项 | 移动后 **10/0/0**，同样的 leaf 级核对，唯一模块层 `class_receiver_tests` | 1232→517，新文件 731 |
| 基线 | 移动前 24 项真实 native object/link/run **24/0/0**，stdout/stderr 与 drop 断言未改 | — |

机械核对：两个新文件的 `#[test]` 块与移动前逐字节相同，根剩余 import、`TestDirectory`、
失败用例 helper 与 `#[allow(unused_imports)]` 绑定未动；policy 只删除
`crates/lang-codegen/src/native/unit_tests.rs` 的 baseline 与 exception。

真实失败保留：组合回归首次 634/0/0 通过但两个新域过滤器写成 `native_tests::`（实际在
`native::unit_tests::` 下）导致零命中、runner 失败，不计完整基线；改为正确前缀后单次
**658/0/0**，未重复计入。rustfmt 首次 fmt 门禁失败（新文件尾部空行与 `mod` 声明顺序），
格式化后重跑默认组合仍 658/0/0 并通过 fmt/尺寸/空白。

组末与0463同树：双配置各 **658/0/0**、workspace check/双Clippy/47项policy/docs961 通过；
历史超限 25→22。原始日志/快照/完整区间审查留仓库外。
