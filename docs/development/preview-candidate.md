# Preview 候选包：构建与独立安装验收

> **性质**：工程交付说明 · **状态**：current / SPEC-0272 双宿主实现验收归档 · **读取时机**：构建或验收有界 release 候选时 · **唯一真源**：脚本和 CI 定义执行行为；验收状态由 Spec 维护

合同及未选范围见 [SPEC-0272](../archive/specs/0272-preview-candidate-package.md)。
本页说明工具使用；双宿主独立 runner 已通过实现验收；工具协议测试仍不能替代真实包执行证据。
候选通过全部验收后仍是 CI artifact，不创建公开 Release/tag。

## 1. 生产候选

在干净的指定提交上准备固定 Rust 工具链、Rust 标准库版权材料及现有 LLVM 21 安装：

```sh
rustup component add rust-docs
python3 scripts/package_preview.py --output /tmp/koven-preview-output
```

输出目录必须是新建且位于 checkout 之外。工具真实构建宿主 release CLI，记录源码提交/tree、
lockfile、目标、工具链和命令，并检查实际加载依赖。原始命令证据保存在输出的 `evidence/`；
包内保留可追溯的版本和材料清单，不复制开发 checkout、编译缓存或私人构建路径。

包包含 `bin/kovenc`、教程唯一真源中的三文件参数报告、四组参数合同、安装验收工具、
依赖说明和实际许可材料。包中保留外部 LLVM 策略；不重定位或捆绑动态 LLVM。
标准库完整版权材料不意味着所有跨平台组件都进入了当前产物，构建输入也不等于全部机器码保留。

## 2. 消费同一运行的包

CI 为 macOS arm64 和 Ubuntu 24.04 x86_64 各安排独立生产/消费 job。
消费 job 不 checkout、不 Cargo 构建、不恢复编译缓存，下载对应生产 job 的归档、校验和与 bootstrap：

```sh
python3 check_preview_install.py \
  --archive candidate.tar.gz --sha256 candidate.tar.gz.sha256 \
  --evidence /tmp/koven-preview-evidence --prepare-dependencies
```

归档名以实际产物为准。`--prepare-dependencies` 使用包内说明对应的正常外部安装工具；
已按同一说明准备依赖的本机预检可以省略该参数。
bootstrap 必须与包内消费脚本同源；归档、完整 manifest、目标/宿主及示例合同通过预检后才执行包内工具。

工具创建自己拥有的中文与空格安装/项目目录，对四组 argv 各执行 build、直接运行产物、run，
逐项比较退出码、stdout/stderr 字节。依赖核验使用实际解析路径、版本和哈希；
macOS 同时核对实际 dyld 加载结果与声明闭包，不允许加载器环境覆盖。
最低系统要求以完整依赖闭包为依据，不从 `kovenc` 单个头部标记推导。

成功只移除本次创建的目录并核验同级哨兵；失败保留本次目录及原始证据。
Ubuntu 24.04 x86_64 的宿主预检仅对 libxml2 的两个已审阅发行修订
`2.9.14+dfsg-1.3ubuntu3.9` / `2.9.14+dfsg-1.3ubuntu3.10` 提供有界差hash预检；
包自身完整性、其它依赖/path/LLVM/C driver校验不变。包身份、本地文件一致性与ELF需求不完整
时拒绝例外；同hash保持原严格路径。来源URI/实际安装来源未证，不是供应链或通用ABI保证。
策略预检通过仍须完成全部真实命令与清理；新策略的CI验收证据与覆盖边界见
[SPEC-0322](../archive/specs/0322-preview-libxml2-host-compatibility.md)。

`results.json` 包含命令、输出字节、宿主检查、清理与失败信息。12 条正常命令有缺项时不能通过。

## 3. 验证与剩余范围

Python 协议测试覆盖校验、成员拒绝、缺项、字节差异、宿主失败和清理证据；
现有教程通过共享 `run_case` 保持命令与 oracle。CI 汇总分别要求四个生产/消费 job 成功，
仅在事件和改动路径明确豁免时允许跳过。已有双宿主门禁及选定 M4a 仍是前置。

本片不交付最低系统通用兼容承诺、签名/公证、浏览器下载隔离、缺失依赖负向安装、
包管理器上架、自动更新或性能排名；这些保留在 M5 后续合同。实际进度和所有未执行项只在 Spec 记录。

## 4. 本地版本管理（SPEC-0345 实施中）

候选包附带 `tools/preview_manager.py`，CI artifact 也提供与 bootstrap 同目录的副本。
使用来自可信发布输入的管理器、归档和 checksum；checksum 仅核完整性，不认证发布者。
无需源码 checkout 或 Cargo，但仍须先准备上述外部 LLVM／系统依赖。

```sh
python3 preview_manager.py --root "$HOME/.local/share/koven-preview" install \
  --archive candidate.tar.gz --sha256 candidate.tar.gz.sha256
python3 preview_manager.py --root "$HOME/.local/share/koven-preview" list
python3 preview_manager.py --root "$HOME/.local/share/koven-preview" use SOURCE_COMMIT-TARGET
export PATH="$HOME/.local/share/koven-preview/current/bin:$PATH"
python3 preview_manager.py --root "$HOME/.local/share/koven-preview" deactivate
python3 preview_manager.py --root "$HOME/.local/share/koven-preview" uninstall SOURCE_COMMIT-TARGET
```

把 `SOURCE_COMMIT-TARGET` 替换为 install/list 返回的完整 ID。根目录必须不存在或已由本工具
初始化；不接管现有普通目录。install 完整验证并运行十二条示例命令，成功才发布，不自动激活。
同 ID 拒绝覆盖；use 重新检查文件与宿主后原子切换 current，失败保持旧指向。
卸载当前版本前必须停用或切换；保留其他版本、操作证据和同级文件，不修改 shell 配置。
输出 JSON 记录 ID／当前版本，验证摘要走 stderr；失败退出非零。发布成功后的临时清理失败
以成功结果中的 `cleanup_warning` 报告并保留路径，不把已发布版本误报为未安装。安装证据位于根目录的
`evidence-*`，失败材料保留供排查；不是自动更新、签名／公证或对同用户恶意写入的隔离。
此片实现与双宿主验收见 [SPEC-0345](../specs/active/0345-preview-version-manager.md)，
不代表 M1-S8 三个示范程序的最终联合验收已完成。
