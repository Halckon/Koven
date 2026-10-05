# Linux P2 证据本机接收

> **性质**：本机证据接收记录 · **状态**：current · **读取时机**：复核十组Linux成本或接续预算接受时 · **唯一真源**：本页接收事实；成本原报告与raw在链接归档内

2026-10-05按用户授权，将云端材料通过Library支持的流程物化到Mac
`/tmp/koven-p2-transfer-20261005/koven-p2-transfer.tar.gz`，核大小/SHA并安全解包。
新worktree `/tmp/koven-spec0277`，分支 `feature/spec-0277`，基线main `2be64066`；
范围、红绿、定向验收及剩余决定见 [SPEC-0277](../specs/active/0277-p2-evidence-test-alignment.md)。

## 完整材料与复核

[原样压缩包](evidence/p2-linux-20261005/koven-p2-transfer.tar.gz)共5,648,921 bytes，
SHA256 `bf712bb7c7f47176e5422dabbebe556f31c04af39d39a12c5f244309811f3eb6`。
[接收记录](evidence/p2-linux-20261005/receipt.json)保留Library身份、云/Mac基线与校验范围。
不将5269个文件直接铺入Git，也不删除噪声、失败、中断或冻结旧汇总。

在仓库根选择一个新的空临时目录，再解包并验证：

```sh
destination=$(mktemp -d /tmp/koven-p2-review.XXXXXX)
tar -xzf docs/development/evidence/p2-linux-20261005/koven-p2-transfer.tar.gz -C "$destination"
python3 "$destination/koven-p2-linux-20261005/verify_transfer.py" "$destination/koven-p2-linux-20261005"
```

接收时已检查全部成员无绝对路径、`..`、symlink/hardlink和特殊文件；只读校验器核5266个发布payload摘要，
另3个原始/发布hash与脱敏manifest同包保存。复核脚本不执行采样，不需要改写云路径。
原报告内的相对链接在解包目录中可完整使用。

| 包内入口（根为 `koven-p2-linux-20261005/`） | 内容 |
|---|---|
| `TRANSFER.md`、`report-final.md` | 十组配对、全阶段统计、接受限制与移交规则 |
| `independent-audit.md/.json`、`semantic-coverage-audit.md/.json` | raw重算、失败/噪声、历史和精确CI的范围 |
| `run-*/`、`probe-frozen-v1.py`、`plan.md` | 917记录/4964 raw摘要、协议与旧冻结工具；含中断/失败 |
| `summarize-v2.py`、`test_summarize_v2.py`、`summary-v2-*` | 首启遗漏修复、20回归、确定性与原始hash保全 |
| `frontend-validation/`、`frontend-fix/` | 云1701 pass/2 fail原证据、云修后1704 pass、两文件format-patch及manifest |
| `published-sha256.json`、`original-sha256.json`、`redaction.json` | 发布字节、脱敏前摘要对应及明确路径token |

## 结论边界

Linux是在Debian13.6 x86_64/glibc2.41，不称Ubuntu24.04 exact镜像。冷/热构建、首启、热执行、no-op、
C observer child RSS各自独立；compile/link归属和最大报告RSS不是exclusive CPU或同时进程树峰值。
十组采样完成，12项指标噪声及plan-tests首启一个单对调查命中保留；配对中位未触线，
无构建/C child RSS增量调查触发。调查线不是用户接受预算，不证明等价或关闭P2。
旧Mac的超噪声停止不被本包覆盖；旧云raw缺失不被补造。

云base `c5c4a8df` 的CI37248672159和云修复 `2aa75b0c` 的1704全量通过各属其精确输入；
不当作最新Mac、0277新分支CI或后继0275成本证据。新分支CI尚未运行；
用户成本接受仍开放，证据已交付不会自动构成豁免。

## Mac 定向接收验收

最新main在两测试上仍含旧预期；修改前2 passed/2 failed，修改后默认debug两目标5 passed/0 failed，
没有ignored/filtered。两测试源字节与云修复manifest完全相同，测试diff摘要也相同；
旧120库存片段全部保留，新127，长非法数字矩阵16→24并增加4种合法radix表达式对照。
[Mac red日志](evidence/p2-linux-20261005/mac-red.log)、[green日志](evidence/p2-linux-20261005/mac-green.log)
和[green命令/口径](evidence/p2-linux-20261005/mac-green.json)独立保存，不混用云结果。
v2的20项Python回归在Mac实际通过，fmt/受影响严格Clippy/尺寸门禁亦通过；
尺寸报告781手写Rust、45旧超限，两变更文件579/396行，未扩大既有policy。
未执行Mac全量frontend、native/其他crate全回归或P2重采样；新分支CI未运行。

本地材料提交 `9c08a666c97336b260a6382d0f6e3594e69e54e8`；
修复与验收提交 `550d540e27e1a174c7a5ba6917b383a186b136fa` 对应云修复
`2aa75b0c86e9fe5cf4e2fc7600c657f7cc8c54c6` 的相同测试字节/delta；
Mac包含后继0275及本批Spec/证据，不宣称恢复云commit SHA或整树相同。

## 后续交付复核（2026-10-05）

用户已授权在合适时合并本地0277；原三个提交保留，接入最新main和PR验收见正式Spec§9。
独立审阅确认两测试与发布材料正确，但初次Mac v2及部分门禁只有receipt声明、缺原始输出。
本次在`f18e039`重新复核并保全[实际Mac门禁账本](evidence/p2-linux-20261005/integration-preflight.json)，
覆盖两测试5项、v2回归20项、5266payload验证、fmt/对应Clippy/尺寸/docs521页与37checker tests。
工具身份、命令、exit和原始字节hash均关联，不把云端日志冒充Mac，也不重建初次0.009s记录。
此补证不重跑十组成本采样，不接受预算；最新main/PR/最终归档/merge/main仍待真实交付。

最新main `18b89e5`在实际15/15 CI及独立产物核验后合入0277，原三提交保留。
[接入复验](evidence/p2-linux-20261005/main-integration.json)记录精确merge源码的Mac两目标5项、
严格Clippy/fmt及政策/文档/尺寸等九门禁实际成功；新PR head双宿主与最终交付仍待。
