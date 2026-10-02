# Rust 与模块规范

> **性质**：工程规则 · **状态**：current · **读取时机**：修改 Rust 生产代码或模块结构时 · **唯一真源**：本页与作用域 AGENTS

- 遵循 rustfmt、Rust 命名约定和 workspace lint；目标语言的 Kotlin 风格命名不得进入 Rust API。
- 模块按语法领域、阶段、状态机或恢复边界拆分，一个模块只有一个主要变化原因。
- `mod.rs`/门面模块只声明结构、入口和必要 re-export；实现细节保持最小可见性和单向依赖。
- 不创建无明确领域含义的 `utils.rs`/`common.rs`；单次调用不建立未来化 facade 或 trait。
- 手写 Rust 生产、测试与 helper 文件以 1000 物理行为软上限；[尺寸护栏](rust-size-policy.md)登记历史欠账并阻止无例外增长，不压行或继续塞入新职责。
- 优先 early return、具体错误类型和 `Result`；正常用户输入路径不使用无说明的 panic/unwrap/expect。
- `unsafe` 只在安全 Rust 无法表达的最小边界使用，每个块前说明调用义务和被维护不变量。
- 库层不使用 `println!`、`eprintln!`、`dbg!`；用户诊断返回结构化数据，CLI/LSP 负责呈现。
- 注释只解释不变量、恢复边界、Span/所有权、复杂度或非显然取舍，不逐行翻译代码。
- 只删除本次改动造成的无用项；发现无关死代码时报告而不顺手清理。

拆分大型模块前先用 characterization tests 锁定公共 API、AST、诊断内容与顺序；每次提取后先运行
最近窄测试，再按 [测试规则](testing.md)升级门禁。
