# M5 测量、分发与成本接受（候选）

> **性质**：非规范工程候选 · **状态**：未启用；P2预算延期/未接受 · **读取时机**：决定新测量、未选发布范围或P2预算时 · **唯一真源**：本页的开放决定；既有结果在正式Spec与历史材料

0272只承接已选preview范围；现有工具用法见[preview方法](../development/preview-candidate.md)。
[原M5草稿](../archive/development/measurement-distribution-spec-draft.md)的旧未运行表是历史，
不能当当前失败或整体完成。新实施先选有界Spec，发布动作按当次授权。

## M5a 新测量候选

D01/D02须为新问题固定协议：区分编译器构建与Koven程序编译，固定输入、before/after SHA、
工具链、优化选项和等价工作量，分别记录前端/codegen/链接或端到端。明确冷/暖缓存、首启、
重复次数/样本顺序/负载、elapsed/CPU/RSS口径、产物尺寸和运行开销；不混宿主单位、不省略
失败/噪声，未控制OS缓存不称完全冷启动。

先小样本确认预算再冻结协议与调查阈值，阈值不是退化接受承诺。Rust/C比较需等价输入、
算法、输出、错误和资源责任，不可比处明示，不宣传全面排名。使用独立目录、复用工具且
不争用Cargo target；原始输出遵守[证据保留](../development/testing.md#证据保留)，不重新入仓dump。

## M5b 未选分发范围

0272的D03–D05及候选D07有界结果不关闭：最低OS/glibc/CPU、实际LLVM/链接依赖、捆绑许可、
签名/下载隔离、D06安装负例、版本来源/校验和、撤回/替换。新目标需真实干净环境和预装
工具清单；包路径/非ASCII/输入输出、可移除范围、缺依赖诊断按新合同验收，checkout试跑不替代。

只承诺实际核实的平台/API，不要求用户无说明关闭系统防护。外部上传/tag/release另需授权；
自动更新、包管理器、Windows、musl与交叉编译未自动选入。

## P2成本预算接受

这是原治理P2预算的唯一活决定入口，保持延期/未接受。功能里程碑、文件缩小、测试修复或
0277交付均不构成成本接受，也不表示原P0–P5全部验收完成。

- 旧Mac超噪声后停止；稳定校准或明确新协议前不扩配对，旧云raw缺失不补造。
- Linux十组已交付，12项噪声及plan-tests首启单对调查保留；不覆盖旧Mac限制。
  未触构建/C child RSS调查线不等于接受预算。
- 待决定：接受哪些平台/阶段的何种证据与退化预算，或另选重测协议、资源和授权范围。
  M5a不能倒填P2历史，也不自动取得重测授权。

固定依据按需追溯[原计划](../archive/development/engineering-governance-plan.md)、
[原检查点](../archive/development/engineering-governance-progress.md)、
[Mac成本](../archive/development/p2-frontend-cost-followup.md)、
[Linux接收](../archive/development/p2-linux-cost-transfer.md)；不另抄一份数字账本。

M0剩余交接按真实缺口选Spec：先核调用方、single/unit差异及兼容入口迁移条件，不把全面合并
两个入口或清零尺寸欠账作为隐含收尾。README/editor事实由真实门禁核验，新长期决定须ADR；
有限工程交付不能推导全部P3/P4后继完成。
