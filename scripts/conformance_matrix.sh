#!/usr/bin/env bash
# Conformance probe 矩阵：每个 probe 分别以单文件模式和单文件 project 模式运行，
# 输出三列 TSV：名称、单文件结果、project 结果。结果记号见 conformance/README.md。
#
# 用法（在任意目录执行；相对路径一律按仓库根解释）：
#   KOVENC=target/release/kovenc scripts/conformance_matrix.sh [probe-dir] > matrix.tsv
#
# 环境变量：
#   KOVENC         编译器路径，默认 target/release/kovenc
#   PROBE_TIMEOUT  每次运行的秒数上限，默认 60
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2

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

if [ ! -d "$PROBES" ]; then
  echo "probe directory not found: $PROBES" >&2
  exit 2
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# classify <exit-code> <stdout+stderr> <stderr>
classify() {
  code=$1
  out=$2
  err=$3
  if [ "$code" -eq 0 ]; then
    if [ -n "$out" ]; then echo "OK"; else echo "OK(silent)"; fi
    return
  fi
  if [ "$code" -eq 124 ]; then
    echo "timeout"
    return
  fi
  if [ "$code" -eq 134 ]; then
    first=$(printf '%s\n' "$err" | head -n 1)
    case "$first" in
      "abort: "*)
        printf 'abort:%s\n' "${first#abort: }" | tr '\t' ' '
        return
        ;;
    esac
  fi
  c=$(printf '%s\n' "$out" | grep -oE "L[0-9]{4}" | head -n 1)
  b=$(printf '%s\n' "$out" | grep -oE "lowering failed with [A-Za-z]+" | head -n 1 | awk '{print $4}')
  o=$(printf '%s\n' "$out" | grep -oE "ownership checking failed: [a-z -]+" | head -n 1)
  if [ -n "$c" ]; then
    echo "FE:$c"
  elif [ -n "$b" ]; then
    echo "BE:$b"
  elif [ -n "$o" ]; then
    echo "ICE:ownership"
  elif [ -z "$out" ]; then
    echo "abort(exit $code,no msg)"
  else
    echo "ERR"
  fi
}

# run_in <dir> <command...>：在 dir 中运行命令，设置 RC、OUT（stdout+stderr）与 ERR（stderr）
run_in() {
  dir=$1
  shift
  (cd "$dir" && timeout "$TIMEOUT" "$@") >"$WORK/stdout" 2>"$WORK/stderr"
  RC=$?
  ERR=$(cat "$WORK/stderr")
  OUT=$(cat "$WORK/stdout" "$WORK/stderr")
}

for path in "$PROBES"/*.ko; do
  [ -e "$path" ] || continue
  file=$(basename "$path")
  name=${file%.ko}

  run_in "$PROBES" "$KOVENC" run "$file"
  single=$(classify "$RC" "$OUT" "$ERR")

  project="$WORK/$name"
  mkdir -p "$project/src/app"
  printf 'schema = "koven.project"\nversion = 1\n\n[project]\nname = "probe"\nsource-roots = ["src"]\n' \
    >"$project/project.toml"
  { echo "package app"; cat "$path"; } >"$project/src/app/main.ko"
  run_in "$project" "$KOVENC" run --project project.toml --entry app.main
  unit=$(classify "$RC" "$OUT" "$ERR")
  rm -rf "$project"

  printf '%s\t%s\t%s\n' "$name" "$single" "$unit"
done
