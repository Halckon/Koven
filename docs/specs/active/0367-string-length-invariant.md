# SPEC-0367: String 创建入口的 Int 字节长度不变量

> **性质**：实现合同 · **状态**：in-progress · **读取时机**：实现或验收 String 长度边界时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0367` |
| 所属 Phase | Phase 2、4、5 |
| 语言规范 | [Guide v0.47](../../guide/13-program-runtime-standard-library.md#string-长度与可恢复连接) |
| 批准依据 | [生态持续实施授权](../../development/ecosystem/governance.md#立项与依赖)；已启用的 v0.47 合同 |
| 主里程碑 | CORE-V1-M1 / ECO-V1-M1，M1-S1 前置切片 |
| 前置 Spec | [SPEC-0366](../../archive/specs/0366-text-io-guide.md) |
| 关联 ADR | [0018](../../adr/accepted/0018-string-owner-runtime-abi.md)、[0019](../../adr/accepted/0019-parameterized-process-entry-bridge.md) |
| 阻塞项 | 无语义阻塞；生产验收待执行 |
| 影响范围 | frontend 字面量检查、LLVM String 创建与 argv bridge、对应测试与事实文档 |
| 语言语义变更 | 否 |

## 1. Goal 与输入身份

从 main `2e229806c42b726eff4ba2f03790681daff372ff` 建立
`feature/spec-0367-string-length-invariant`，让已有 String 创建入口遵守现行 Int 字节长度上限，
为公开文本 API 提供可靠前置；内部 target usize ABI 保持不变。

## 2. 范围与失败边界

- 字面量按解码后的 UTF-8 字节长度检查，超限返回带原始 Span 的 L0168 编译诊断；常量计算超限沿现有 L0158。
- 动态连接在分配、复制及发布 owner 前检查上限，超限进入既有 Abort。当前插值仍由 codegen 拒绝，不扩大其准入；后继插值必须复用此边界。
- argv 在无分配全量预检中检查每个参数的长度；超限返回 operational failure，不调用 main。
- clone 不扩大已有长度，保持独立 owner 与空串行为；盘点其他已有创建入口，不能遗漏旁路。
- 保留已有机器宽度溢出、OOM、UTF-8 校验、内嵌 NUL 和唯一析构合同。

## 3. 非目标

本片不实现 byteLength、checkedPlus、decodeUtf8、sliceBytes、split/lines、parseLong 或 IO API；
这些后继仍须分别交付，不能因长度守卫完成而将文本 API 或 M1-S1 标记完成。
不缩窄 String 布局，不新增依赖，不修改其他在途 Spec。

## 4. 验收标准

- [ ] 生产谓词的 0、Int.MAX_VALUE、Int.MAX_VALUE + 1 边界；不靠真实巨型分配造用例。
- [ ] 字面量超限诊断身份与 Span，经实际生产检查入口测试；普通 Unicode／转义字面量保持通过。
- [ ] 动态连接超限先 Abort，覆盖 target-width 溢出；正常空串、中文、内嵌 NUL 内容与 owner 保持。
- [ ] argv 超限走非零返回，且发生在 UTF-8 遍历、owner 分配和 Koven main 之前。
- [ ] 合成长度测试只证明守卫行为；与真实小输入 native build/run 分别记账，不冒充真实 GB 输入验收。
- [ ] single 与 compilation-unit 入口无遗漏；clone、插值与当前创建点均有明确覆盖或既有证据。
- [ ] 独立审查、定向测试、双宿主官方 CI、文档门禁通过后归档并完成 PR。

## 5. 实施计划

1. [x] 盘点生产创建点、共享常量与现有测试 → 确定最小复用点和失败回归。
2. [ ] 先增加失败测试，再实施字面量、concat、argv 的最小守卫 → 验证边界及正常路径。
3. [ ] 独立审查并修复实际发现 → 复核单一最大剩余风险。
4. [ ] 同步 Architecture 与验收账本 → 运行文档门禁、官方 CI，归档、合并。

## 6. 验证与交付记录

Rust/native 动态验收先使用官方 CI；本地文档检查不能替代运行结果。
保留输入身份、命令、命中数及稳定来源，原始日志不入库。

| 验收项 | 结果 | 边界 |
|---|---|---|
| 创建点源码盘点 | 已完成 | 两 checker、常量求值／连接、SSA literal、concat、argv 已接线；clone 不扩大长度；插值未接入 |
| Rust/native 测试 | 未执行 | 新增 3 个 frontend helper 测试、2 个 native 合成长度探针及 argv IR 断言，待官方 CI |
| `python3 scripts/check_docs.py`、`git diff --check` | 820 Markdown 文件通过；diff 无错误 | 仅合同建立，不证明生产行为 |

提交按单一逻辑分片，消息包含 SPEC-0367；实现、测试与事实保持一致，归档单独记录最终 CI 身份。

### 独立审查记录

独立上下文检查两 checker、常量路径、concat/argv 顺序和 clone：发现纯字面量 IR 断言会被
LLVM 常量折叠，以及 SSA literal 缺少上限。前者改为动态借用参数 native 探针，后者已补 verifier
守卫；复核 native C ABI、函数导出及 argv 注入路径未发现阻塞，运行验证尚未执行。
前端 helper 的合成长度测试证明上限谓词与诊断 Span，不声称实际解析了 GB 级源码。

尺寸门禁 `python3 scripts/check_rust_sizes.py --base origin/main` 通过：保留 39 份超千行旧欠账；
两个 checker 各 +5 行、native 注册 +3 行、verifier +1 行均经独立审阅，沿用既有额度，
新 helper／native 探针独立成短文件。首次文档检查发现入口页超限，已改为独立专题并补索引。

### 首轮官方 CI 与夹具修复

实现 head `64e8877993dd1b97cbfbf5361d22d4cc6a077d60` 的
[PR CI 40903473887](https://github.com/Halckon/Koven/actions/runs/40903473887)
中 Linux frontend lib 为 229 passed / 0 failed / 0 ignored，包含本片 3 个 helper 测试。
codegen 为 2477 passed / 1 failed / 0 ignored；argv 超限 native 探针通过，concat 探针在查找
函数定义时失败，尚未执行其长度断言。原因是 Unit 函数包含 declaration/instance 后缀，测试
错误地依赖 `.combine(`。修复改用返回的 FunctionId 与实际 SSA 名称，并断言唯一 LLVM 定义；
生产守卫和原有边界断言保持不变。该修复仍须由后续官方 CI 验证，首轮失败不记为通过。


修复 head `39a22bcd7205fdcc315a6058c939923e302252da` 的
[CI 40906179616](https://github.com/Halckon/Koven/actions/runs/40906179616) 中，macOS codegen
为 2475 passed / 2 failed / 1 ignored；两个新探针都在系统 Clang 解析 LLVM 21 的
`captures(none)` 时失败，未执行行为断言。测试改用已有 `test_support::ir_clang()`，与相邻
String clone 插桩测试一致；不修改 IR 内容或生产逻辑。既有 ignored 项不计通过。
