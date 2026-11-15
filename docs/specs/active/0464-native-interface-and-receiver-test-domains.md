# SPEC-0464：Native 接口/委托与接收者测试领域归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 native 测试根尺寸欠账时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0464` |
| 所属 Phase | Phase 4，native object/link/run测试组织 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续下一批；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0463](0463-ownership-operation-and-lowering-domains.md)，同一批次输入4df265b1 |
| 前置 ADR | [ADR-0010](../../adr/accepted/0010-first-native-object-and-linker-contract.md) |
| 影响范围 | native/unit_tests.rs及两个私有测试领域模块、对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整测试领域

1. 接口/继承/委托十二项完整测试移入私有 `native_unit_interface_delegation_tests.rs`：
   interface default 与 super static 调用、abstract requirement override、继承与无关默认
   实现、list/parameter-independent/dependent inherited owner recipe、borrow-only 委托、
   same-requirement/identity-changing 委托链、generic interface owner 与 callable 委托、
   parameter-independent/nested generic nominal 委托、`Inout` 与 value interface default。
2. 类接收者与 inline 所有权十项完整测试移入私有 `native_unit_class_receiver_tests.rs`：
   object atomically replace/link、direct member receiver 顺序与唯一 drop、`Inout` 载荷
   变更、MoveOnly 载荷替换、stateless object 借用、inline `Inout` 只读/可写回写、
   MoveOnly inline take、inline 可复制字段变更与嵌套 drop glue。

原 `#[test]` 属性、正文、fixture 源码与断言逐字节保全；FQN 只增加一个领域层。根保留
全部 imports、`TestDirectory`、`assert_no_sibling_temporary` 及子模块加载顺序，子模块以
私有 `#[path]` + `use super::*` 继承原绑定，不复制 harness 或扩权。

## 2. 非目标

不改生产 native/LLVM/SSA 代码与任何断言、stdout 或对象/链接身份；不新增 ignored 测试、
不改 abort/失败路径期望。根与新文件均不超 1000，只删除 `native/unit_tests.rs` 的
baseline/exception 登记，其他 policy 逐 JSON 相同。不导入未合 #427，不为凑数删除
34 项合理保留。

## 3. 节奏与验收

- [ ] R1：二十项测试完整保全，旧完整 FQN 无副本，原根剩余内容与 import 不动。
- [ ] R2：逐片定向原领域GREEN→移动→GREEN（含真实 native link/run），本地提交。
- [ ] R3：与0463成组统一双配置组合、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [ ] R4：独立完整精确提交区间审查，有finding修后复核；Architecture/账本后一次push同PR。
- [ ] R5：精确新头官方CI另验；保持active/Draft，不归档或合入main。

原始日志与快照留仓库外；不把编译/list、CI-only fault 或未完成观察当通过。

## 4. 真实证据

待逐片记录。
