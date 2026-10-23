# API 声明索引与示例协议

> **性质**：工具协议与使用入口 · **状态**：current · **读取时机**：导出声明或验证 API 示例时 · **唯一真源**：真实解析器、教程与版本化输出

本首片使用 Guide v0.45，不依赖 v0.46。它不是完整标准库参考，也不包含网站呈现。

## 抽取声明

```sh
target/debug/kovenc doc path/to/source.ko
```

输出 JSON `koven.api-declarations` version 1：Guide/编译器版本、source 文件名、
`validation: syntax-only`、`scope: top-level-ordinary-functions`、functions 和 omitted。
顶层普通函数按源码顺序输出 name、原始 signature、location、parameters。
location 含0基 UTF-8 startByte/endByte（半开区间）、1基 line/column（字符列）；函数范围覆盖完整声明。
参数 type 是原始类型语法，mode 为 default/borrow/own/inout。default 保留未写 marker 的来源，
按 Guide 对应 Borrow；不将 own 误称为必然移动，Copyable 的具体行为仍由语言语义决定。
原始签名保留泛型、修饰符和返回来源，省略返回标注不伪造文本；成员及扩展函数等未覆盖根列在 omitted。
源码没有函数时可返回空 functions；这只说明没有可抽取的函数，不代表示例执行通过。

命令只解析，不运行、不写回输入。可解析但类型错误的源码仍可导出；输出明确未验证类型、
所有权或可执行性。语法失败时 stdout 为空、退出2，支持 `--message-format=json` 既有诊断。
IO/用法失败退出2。输出不含源码绝对路径，源码文件名、字节及编译器版本相同则重复导出一致。

## 绑定并验证教程示例

[examples.json](examples.json) 的 version 1 entries 每项含 example、file、function：
分别引用现有教程合同 ID、该教程的源码路径、该文件内唯一顶层普通函数名。
首片选 greeting 和 inspect；重复绑定、缺声明、重载歧义、空库存、planned 或诊断教程均失败。

```sh
python3 scripts/check_api_examples.py > /tmp/koven-api-examples.json
```

检查器使用真实 doc 导出，再复用教程执行器核 build、直接运行、run 的完整结果。
stdout 只有成功后生成的 `koven.api-examples` version 1；进度写 stderr，失败非零且无成功 JSON。
产物含 Guide、实际 CLI 二进制 SHA256、compilerVersion、源码 fence ID、源码 SHA256 和声明。
声明位置相对 fence 内容，不是 Markdown 行号；通过 tutorial 路径与 sourceFence 定位原文。
不复制教程源码、不创建第二套运行 oracle，不把调用者填写的版本当成已验证产物身份。
本机生成物不是跨平台相同的发布包，compilerSha256 会随实际编译器二进制变化。

双宿主门禁实际运行此检查器。实施与验收见 [SPEC-0348](../../archive/specs/0348-api-documentation.md)。
