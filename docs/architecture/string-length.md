# String 创建长度边界

> **性质**：实现事实 · **状态**：current · **读取时机**：修改 String 创建或长度验证时 · **唯一真源**：frontend 与 codegen 代码

single 与 compilation-unit 的 String 表达式共用无分配的 decoded byte 计数，超出
`MAX_STRING_BYTE_LENGTH` 的静态字面量产生 L0168，并保留表达式 Span。常量求值在
物化及连接前检查相同上限；无法完成的常量计算沿既有 L0158 失败路径返回。

String owner 保留 target usize 长度字段；SSA verifier 拒绝超出 Int 上限的 `StringLiteral`。
动态 concat 在分配和复制前合并机器溢出与 Int 上限检查，超限 Abort。argv 在 UTF-8 遍历和
owner 分配前拒绝超限参数并返回 operational failure。clone 不扩大已有长度。

[SPEC-0367](../archive/specs/0367-string-length-invariant.md)记录验收状态；新增 native 探针使用
合成 owner 长度及 strlen／allocator shim，检查失败先于读取、分配及调用 main，不能当作
真实 GB 字符串解析或分配证据。正常内容与 owner 生命周期复用已有 native String/clone/argv 测试。
插值仍保留既有未接入边界；公开文本及 IO API 尚未在本片实现。

## 文本字节接入（验收进行中）

`koven/text/text.ko` 承载 byteLength、toBytes、decodeUtf8、sliceBytes、checkedPlus 与 TextError；
两种 CLI 使用同一 loader，为真实标准 SourceId 单独授权文本扩展和存储原语。
扩展依赖显式可见性与 canonical callable 身份；普通同名源不能获得授权。
unit lowering 消费五种 typed primitive 事实，分别生成字节读取、共享 UTF-8 扫描与独立复制操作。
复制复用 String.clone 的分配／空串路径，范围和 UTF-8 失败先于发布新 String。

`toBytes` 的同步生成器通过私有 Borrow 参数 helper 捕获普通 symbol；现有 closure lowering
尚不接受直接 This capture。公共 Result 错误分类与 checkedPlus 长度策略留在 Koven 源码。
这些接线已存在于当前分支，完整公共 CLI 与 Result<String, TextError> 的 native 验收仍待
[SPEC-0368](../specs/active/0368-text-byte-foundation.md)闭合；不据静态实现宣称接口交付完成。
