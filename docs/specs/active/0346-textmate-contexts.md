# SPEC-0346: TextMate 语境与插值恢复

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：维护编辑器词法资源时 · **唯一真源**：本次交付与验收

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0346` |
| 所属 Phase | Phase 6 |
| 语言规范 | [词法](../../guide/01-lexical.md)；与插件固定 v0.46 同章除版本号外一致 |
| 批准依据 | 用户要求按路线图持续推进；插件 M1/SPEC-0004 的生产方合同 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 影响范围 | TextMate grammar、fixture、真实引擎与生产 Lexer 对照测试 |
| 语言语义变更 | 否 |

## 1. Goal

消除软关键字普通名称误着色和多层插值过早结束，保证连续编辑按现行词法边界恢复。

## 2. 范围

- `companion`、`const val` 与普通声明、软关键字名称/调用和已知声明语境。
- 插值中多层花括号、字符串、Char、注释及转义；未闭合字符串按换行恢复，未闭合插值/块注释按 Guide 保持到闭合或 EOF。
- 共享源码和逐 scope 断言，由官方 vscode-textmate/Oniguruma 执行；生产 Lexer 检验相关拼写/恢复。
- 由 koven-editors SPEC-0004 锁定同步并运行 JetBrains 引擎；不在主仓库创建 IDE 插件。

## 3. 非目标

不变更 Guide、编译器语言行为、Tree-sitter、语义高亮或全量 Parser。TextMate 不承担语义合法性诊断。

## 4. 验收

- [x] 先用真实引擎复现错误，修复后同批逐 scope 断言通过。
- [x] 生产 Lexer 的软关键字/硬关键字、插值及恢复对照通过。
- [ ] 原有词法合同、TextMate 测试及 CI 编辑器门禁通过。
- [ ] 新增测试依赖许可与锁定审计、独立审查、Spec 归档、最终 PR CI。

## 5. 证据

原语法的真实引擎测试已复现普通软关键字误着色及多层插值提前结束。初版 Rust 断言的 DiagnosticCode 类型比较编译失败，按既有 Display 接口修正后 `cargo test -p lang-frontend --test textmate_grammar` 六项通过（0 ignored/filtered）。本机 `npm test --prefix editors/textmate` 通过 80 项旧合同与 12 个源码文件的 82 项真实引擎 scope 断言。独立审查发现普通表达式模式词误着色和非法 Char 花括号提前结束插值，两项均经失败用例、修复与复核关闭。语境规则仍为局部近似，不宣称完整 Parser 覆盖。JetBrains 消费端验收属于插件 SPEC-0004；本 Spec 最终 PR CI 尚未执行。`python3 scripts/check_docs.py` 通过 742 文件；脚本 suite 运行 398 项，397 通过、1 项 Linux `/proc` 专用测试在 macOS 跳过。`git diff --check` 通过。

测试依赖审计：vscode-textmate 9.3.2 与 vscode-oniguruma 2.0.1 为官方 VS Code 引擎（MIT），后者 NOTICE 包含 Oniguruma 6.9.5_rev1 的 BSD 许可。两项无 npm 传递依赖，精确锁版本及 integrity，安装禁用 scripts，npm audit 0 项已知漏洞；此结果不等于永久安全保证。只在 Node 22 开发/CI 测试运行，不随编译器或插件分发，不新增 Rust/native 构建依赖；WASM 由锁定 npm 包提供。仓库原 regex verifier 无状态栈，无法验证嵌套插值，复用官方引擎避免自建 tokenizer。API 依据 [官方说明](https://github.com/microsoft/vscode-textmate)。原始日志不入库。
