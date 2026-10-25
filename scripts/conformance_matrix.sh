#!/usr/bin/env bash
# Conformance probe 矩阵与回归门禁入口：
# 1. 默认兼容模式：每个 probe 分别以单文件模式和单文件 project 模式运行，
#    输出三列 TSV：名称、单文件结果、project 结果。
# 2. 分别保存原始 stdout 与 stderr 字节、退出状态和超时结果（若设置 KOVEN_CONFORMANCE_EVIDENCE
#    则落盘保存每个 probe 的 .single.stdout/.stderr、.project.stdout/.stderr 与 manifest.json）。
# 3. 门禁比较模式：传入 --gate [args...] 时转发给 scripts/conformance_gate.py 执行强 oracle 与回归比对。
#
# 用法（在任意目录执行；相对路径一律按仓库根解释）：
#   KOVENC=target/release/kovenc scripts/conformance_matrix.sh [probe-dir] > matrix.tsv
#   KOVENC=target/release/kovenc KOVEN_CONFORMANCE_EVIDENCE=/tmp/conf-ev scripts/conformance_matrix.sh > matrix.tsv
#   scripts/conformance_matrix.sh --gate --cli target/release/kovenc --evidence /tmp/conf-ev
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2

if [ "${1:-}" = "--gate" ]; then
  shift
  exec python3 "$ROOT/scripts/conformance_gate.py" "$@"
fi

KOVENC=${KOVENC:-target/release/kovenc}
case "$KOVENC" in
  /*) ;;
  */*) KOVENC="$ROOT/$KOVENC" ;;
esac
PROBES=${1:-conformance/probes}
case "$PROBES" in
  /*) ;;
  *) PROBES="$ROOT/$PROBES" ;;
esac
TIMEOUT=${PROBE_TIMEOUT:-60}
EVIDENCE=${KOVEN_CONFORMANCE_EVIDENCE:-}
if [ -n "$EVIDENCE" ]; then
  case "$EVIDENCE" in
    /*) ;;
    *) EVIDENCE="$ROOT/$EVIDENCE" ;;
  esac
  mkdir -p "$EVIDENCE/raw" || exit 2
fi

if [ ! -d "$PROBES" ]; then
  echo "probe directory not found: $PROBES" >&2
  exit 2
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# classify_files <exit-code> <stdout-file> <stderr-file>
# 直接读取独立的原始字节文件，避免合并 stdout/stderr 或由命令替换 $(cat ...) 吞掉尾换行。
classify_files() {
  code=$1
  out_file=$2
  err_file=$3
  if [ "$code" -eq 0 ]; then
    if [ -s "$out_file" ]; then echo "OK"; else echo "OK(silent)"; fi
    return
  fi
  if [ "$code" -eq 124 ]; then
    echo "timeout"
    return
  fi
  if [ "$code" -eq 134 ]; then
    first=$(head -n 1 "$err_file" | tr -d '\r')
    case "$first" in
      "abort: "*)
        printf 'abort:%s\n' "${first#abort: }" | tr '\t' ' '
        return
        ;;
    esac
  fi
  c=$(grep -oE "L[0-9]{4}" "$out_file" "$err_file" 2>/dev/null | head -n 1 | sed 's/^.*://')
  b=$(grep -oE "lowering failed with [A-Za-z]+" "$out_file" "$err_file" 2>/dev/null | head -n 1 | awk '{print $4}')
  o=$(grep -oE "ownership checking failed: [a-z -]+" "$out_file" "$err_file" 2>/dev/null | head -n 1)
  if [ -n "$c" ]; then
    echo "FE:$c"
  elif [ -n "$b" ]; then
    echo "BE:$b"
  elif [ -n "$o" ]; then
    echo "ICE:ownership"
  elif [ ! -s "$out_file" ] && [ ! -s "$err_file" ]; then
    echo "abort(exit $code,no msg)"
  else
    echo "ERR"
  fi
}

# run_in <dir> <out-file> <err-file> <status-file> <command...>
run_in() {
  dir=$1
  out_file=$2
  err_file=$3
  status_file=$4
  shift 4
  (cd "$dir" && timeout "$TIMEOUT" "$@") >"$out_file" 2>"$err_file"
  RC=$?
  timed_out=false
  if [ "$RC" -eq 124 ]; then
    timed_out=true
  fi
  printf '{"exit":%d,"timed_out":%s}\n' "$RC" "$timed_out" >"$status_file"
}

for path in "$PROBES"/*.ko; do
  [ -e "$path" ] || continue
  file=$(basename "$path")
  name=${file%.ko}

  if [ -n "$EVIDENCE" ]; then
    s_out="$EVIDENCE/raw/$name.single.stdout"
    s_err="$EVIDENCE/raw/$name.single.stderr"
    s_st="$EVIDENCE/raw/$name.single.status.json"
    p_out="$EVIDENCE/raw/$name.project.stdout"
    p_err="$EVIDENCE/raw/$name.project.stderr"
    p_st="$EVIDENCE/raw/$name.project.status.json"
  else
    s_out="$WORK/single.stdout"
    s_err="$WORK/single.stderr"
    s_st="$WORK/single.status.json"
    p_out="$WORK/project.stdout"
    p_err="$WORK/project.stderr"
    p_st="$WORK/project.status.json"
  fi

  run_in "$PROBES" "$s_out" "$s_err" "$s_st" "$KOVENC" run "$file"
  single=$(classify_files "$RC" "$s_out" "$s_err")

  project="$WORK/$name"
  mkdir -p "$project/src/app"
  printf 'schema = "koven.project"\nversion = 1\n\n[project]\nname = "probe"\nsource-roots = ["src"]\n' \
    >"$project/project.toml"
  { printf 'package app\n'; cat "$path"; } >"$project/src/app/main.ko"
  run_in "$project" "$p_out" "$p_err" "$p_st" "$KOVENC" run --project project.toml --entry app.main
  unit=$(classify_files "$RC" "$p_out" "$p_err")
  rm -rf "$project"

  printf '%s\t%s\t%s\n' "$name" "$single" "$unit"
done
