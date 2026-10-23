# Kovenlings：两道本地练习

> **性质**：学习材料与执行合同 · **状态**：current · **读取时机**：练习可变位置与所有权参数时 · **唯一真源**：各题源码与 exercises.json 的提示、运行合同

本页使用 Guide v0.45 已有能力，不依赖 v0.46。先按[开发环境](../../development/README.md)构建
`kovenc`，下列命令在仓库根目录执行。这里验证你提供的本地源码，会编译并运行它；仅使用你信任的练习文件。

| 题目 ID | 目标 | 起点与参考答案 |
|---|---|---|
| `mutable-place` | 让 replace 更新 number，并保留旧值；输出 replaced | [起点](mutable-place/starter.ko) · [答案](mutable-place/solution.ko) |
| `borrow-owner` | 同一个 Resource 被读取两次，最后只析构一次 | [起点](borrow-owner/starter.ko) · [答案](borrow-owner/solution.ko) |

## 做题

复制题目后编辑副本；不要修改参考答案或测试合同。以第一题为例：

```sh
cp docs/tutorials/kovenlings/mutable-place/starter.ko /tmp/koven-exercise.ko
# 在编辑器中修改 /tmp/koven-exercise.ko
python3 scripts/check_kovenlings.py --exercise mutable-place --source /tmp/koven-exercise.ko
```

未修复的起点应失败。判定依据是成功构建、直接执行及 `kovenc run` 的完整输出和退出状态，
不是源码必须与参考答案逐字一致。校验使用临时目录，不改你的副本。需要提示时：

```sh
python3 scripts/check_kovenlings.py --exercise mutable-place --hint
python3 scripts/check_kovenlings.py --exercise borrow-owner --hint
```

第二题应输出两行 `read` 后一行 `drop`；第一次读取不应取得并销毁 owner。
要逐步查看原始编译诊断，也可直接使用 `kovenc build /tmp/koven-exercise.ko -o /tmp/koven-exercise`。

## 维护与验收

```sh
python3 scripts/check_kovenlings.py
```

默认审计会确认两个起点按预期编译失败、两个答案成功构建并执行，共四个源码变体、八条公开命令。
“起点预期失败通过”不等于练习已经做对；`--source` 才检查你的解法。
完整诊断（含代码与位置）、stdout、stderr 和退出状态固定在 [exercises.json](exercises.json)。
复用教程 `run_case`，由双平台 CI 的 bounded composition 执行；不提供网页运行、项目题或完整测试框架。
交付状态见 [SPEC-0347](../../archive/specs/0347-local-kovenlings.md)。
