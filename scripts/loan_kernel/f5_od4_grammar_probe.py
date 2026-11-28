"""OD4（L2/L3 候选语法前三项）F5 增量门禁可复跑 GNU Bison 冲突与见证验证套件。

固化 docs/proposals/parser-formal-verification/f5-od4-l2-l3-syntax-gate-drill.md
所依赖的全部基准与增量纯 CFG 文法（无 %left/%right）以及 21 组可编译正反例见证，
确保评审者可在具备 bison 与 cc 的环境中一键复核冲突计数与接受/拒绝矩阵。

仅使用 Python 3.9+ 标准库与系统 bison / cc；编译产物在 TemporaryDirectory 中运行后自动清理。
"""
from __future__ import annotations

from dataclasses import dataclass
import os
import re
import shutil
import subprocess
import tempfile
from typing import Dict, List, Optional, Tuple


HEADER = r"""
%token ID NL SEMI COLON COMMA DOT EQ LT GT LPAREN RPAREN LBRACE RBRACE
%token FUN VAL VAR CLASS INTERFACE VIEW_KW FLOWS_KW INTO_KW FROM_KW ORIGINS_KW
%token OWN BORROW INOUT THIS
%%
"""

GRAMMARS: Dict[str, str] = {
    "0_baseline": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl ;
class_decl : CLASS ID LBRACE RBRACE ;

fun_decl
  : FUN opt_tparams ID LPAREN opt_vparams RPAREN opt_ret body
  ;

opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;
tparam : ID opt_bound ;
opt_bound : /* empty */ | COLON type_ref ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ID COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_atom
  | COLON type_ref FROM_KW origin_atom
  ;

origin_atom : ID | THIS ;
type_ref : ID ;
body : LBRACE RBRACE | EQ ID ;
%%
""",
    "1a_flows_comma": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl ;
class_decl : CLASS ident LBRACE RBRACE ;

ident : ID | FLOWS_KW | INTO_KW ;

fun_decl
  : FUN opt_tparams ident LPAREN opt_vparams RPAREN opt_ret opt_flows body
  ;

opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;
tparam : ident opt_bound ;
opt_bound : /* empty */ | COLON type_ref ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ident COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_source_list
  | COLON type_ref FROM_KW origin_source_list
  ;

opt_flows : /* empty */ | flows_list ;
flows_list : flows_clause | flows_list COMMA flows_clause ;
flows_clause : FLOWS_KW origin_source_list INTO_KW origin_target ;

origin_source_list : origin_source | origin_source_list COMMA origin_source ;
origin_source : origin_root | origin_root DOT ORIGINS_KW ;
origin_root : ident | THIS ;
origin_target : origin_root | origin_target DOT ident ;
type_ref : ident ;
body : LBRACE RBRACE | EQ ident ;
%%
""",
    "1a_flows_repeat": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl ;
class_decl : CLASS ident LBRACE RBRACE ;

ident : ID | FLOWS_KW | INTO_KW ;

fun_decl
  : FUN opt_tparams ident LPAREN opt_vparams RPAREN opt_ret opt_flows body
  ;

opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;
tparam : ident opt_bound ;
opt_bound : /* empty */ | COLON type_ref ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ident COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_source_list
  | COLON type_ref FROM_KW origin_source_list
  ;

opt_flows : /* empty */ | opt_flows flows_clause ;
flows_clause : FLOWS_KW origin_source_list INTO_KW origin_target ;

origin_source_list : origin_source | origin_source_list COMMA origin_source ;
origin_source : origin_root | origin_root DOT ORIGINS_KW ;
origin_root : ident | THIS ;
origin_target : origin_root | origin_target DOT ident ;
type_ref : ident ;
body : LBRACE RBRACE | EQ ident ;
%%
""",
    "1b_from_into_contrast": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl ;
fun_decl : FUN ID LPAREN opt_vparams RPAREN opt_ret opt_from_flows body ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ID COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_source_list
  | COLON type_ref FROM_KW origin_source_list
  ;

opt_from_flows
  : /* empty */
  | opt_from_flows FROM_KW origin_source_list INTO_KW origin_target
  ;

origin_source_list : origin_source | origin_source_list COMMA origin_source ;
origin_source : ID | THIS ;
origin_target : ID | THIS ;
type_ref : ID ;
body : LBRACE RBRACE | EQ ID ;
%%
""",
    "A_flows_nl": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;
opt_nl : /* empty */ | opt_nl NL ;

decl : fun_decl | iface_decl ;
iface_decl : INTERFACE ident LBRACE iface_members RBRACE ;
iface_members : fun_decl | iface_members sep fun_decl ;

ident : ID | FLOWS_KW | INTO_KW ;

fun_decl
  : FUN ident LPAREN opt_vparams RPAREN opt_ret opt_flows body
  ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ident COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_source_list
  | COLON type_ref FROM_KW origin_source_list
  ;

opt_flows : /* empty */ | opt_flows opt_nl flows_clause ;
flows_clause : FLOWS_KW origin_source_list INTO_KW origin_target ;

origin_source_list : origin_root | origin_source_list COMMA origin_root ;
origin_root : ident | THIS ;
origin_target : origin_root ;
type_ref : ident ;
body : LBRACE RBRACE | EQ ident | /* bodyless */ ;
%%
""",
    "2a_view_naive": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl ;
ident : ID | VIEW_KW ;

class_decl
  : CLASS ident opt_tparams LBRACE RBRACE
  | VIEW_KW CLASS ident opt_tparams LBRACE RBRACE
  ;

fun_decl : FUN opt_tparams ident LPAREN RPAREN LBRACE RBRACE ;
opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;
tparam : ident opt_bound | VIEW_KW ident opt_bound ;
opt_bound : /* empty */ | COLON type_ref ;
type_ref : ident ;
%%
""",
    "2b_view_factored": HEADER
    + r"""
file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl ;
ident : ID | VIEW_KW ;

class_decl
  : CLASS ident opt_tparams LBRACE RBRACE
  | VIEW_KW CLASS ident opt_tparams LBRACE RBRACE
  ;

fun_decl : FUN opt_tparams ident LPAREN RPAREN LBRACE RBRACE ;
opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;
tparam : ID opt_bound | VIEW_KW view_tparam_tail ;
view_tparam_tail : opt_bound | ident opt_bound ;
opt_bound : /* empty */ | COLON type_ref ;
type_ref : ident ;
%%
""",
    "B_origins_collision": HEADER
    + r"""
file : origin_source ;
ident : ID | ORIGINS_KW ;
origin_root : ident | THIS ;
origin_path
  : origin_root DOT ident
  | origin_path DOT ident
  ;
origin_source
  : origin_root
  | origin_path
  | origin_root DOT ORIGINS_KW
  | origin_path DOT ORIGINS_KW
  ;
%%
""",
}

COMBINED_GRAMMAR = r"""
%{
#include <stdio.h>
#include <string.h>
int yylex(void);
void yyerror(const char *s) { (void)s; }
%}

%expect 0
%token ID NL SEMI COLON COMMA DOT EQ LT GT LPAREN RPAREN LBRACE RBRACE
%token FUN VAL VAR CLASS INTERFACE VIEW_KW FLOWS_KW INTO_KW FROM_KW ORIGINS_KW
%token OWN BORROW INOUT THIS
%%

file : decl_list ;
decl_list : decl | decl_list sep decl ;
sep : NL | SEMI ;

decl : fun_decl | class_decl | iface_decl | val_decl ;

ident : ID | VIEW_KW | FLOWS_KW | INTO_KW | ORIGINS_KW ;
non_view_ident : ID | FLOWS_KW | INTO_KW | ORIGINS_KW ;
non_origins_ident : ID | VIEW_KW | FLOWS_KW | INTO_KW ;

val_decl : VAL ident EQ ident ;

class_decl
  : CLASS ident opt_tparams LBRACE RBRACE
  | VIEW_KW CLASS ident opt_tparams LBRACE RBRACE
  ;

iface_decl
  : INTERFACE ident LBRACE iface_members RBRACE
  ;

iface_members
  : fun_decl
  | iface_members sep fun_decl
  ;

fun_decl
  : FUN opt_tparams ident LPAREN opt_vparams RPAREN opt_ret opt_flows body
  ;

opt_tparams : /* empty */ | LT tparam_list GT ;
tparam_list : tparam | tparam_list COMMA tparam ;

tparam
  : non_view_ident opt_bound
  | VIEW_KW view_tparam_tail
  ;

view_tparam_tail
  : opt_bound
  | ident opt_bound
  ;

opt_bound : /* empty */ | COLON type_ref ;

opt_vparams : /* empty */ | vparam_list ;
vparam_list : vparam | vparam_list COMMA vparam ;
vparam : opt_mode ident COLON type_ref ;
opt_mode : /* empty */ | OWN | BORROW | INOUT ;

opt_ret
  : /* empty */
  | COLON type_ref
  | COLON BORROW type_ref FROM_KW origin_source_list
  | COLON type_ref FROM_KW origin_source_list
  ;

opt_flows
  : /* empty */
  | opt_flows flows_clause
  ;

flows_clause
  : FLOWS_KW origin_source_list INTO_KW origin_target
  ;

origin_source_list
  : origin_source
  | origin_source_list COMMA origin_source
  ;

origin_source
  : origin_root
  | origin_path
  | origin_root DOT ORIGINS_KW
  | origin_path DOT ORIGINS_KW
  ;

origin_root : ident | THIS ;

origin_path
  : origin_root DOT non_origins_ident
  | origin_path DOT non_origins_ident
  ;

origin_target
  : origin_root
  | origin_path
  ;

type_ref : ident ;
body : LBRACE RBRACE | EQ ident | /* bodyless */ ;
%%

static const char *g_tokens[256];
static int g_len = 0;
static int g_pos = 0;

int yylex(void) {
    if (g_pos >= g_len) return 0;
    const char *t = g_tokens[g_pos++];
    if (strcmp(t, "NL") == 0) return NL;
    if (strcmp(t, ";") == 0) return SEMI;
    if (strcmp(t, ":") == 0) return COLON;
    if (strcmp(t, ",") == 0) return COMMA;
    if (strcmp(t, ".") == 0) return DOT;
    if (strcmp(t, "=") == 0) return EQ;
    if (strcmp(t, "<") == 0) return LT;
    if (strcmp(t, ">") == 0) return GT;
    if (strcmp(t, "(") == 0) return LPAREN;
    if (strcmp(t, ")") == 0) return RPAREN;
    if (strcmp(t, "{") == 0) return LBRACE;
    if (strcmp(t, "}") == 0) return RBRACE;
    if (strcmp(t, "fun") == 0) return FUN;
    if (strcmp(t, "val") == 0) return VAL;
    if (strcmp(t, "var") == 0) return VAR;
    if (strcmp(t, "class") == 0) return CLASS;
    if (strcmp(t, "interface") == 0) return INTERFACE;
    if (strcmp(t, "view") == 0) return VIEW_KW;
    if (strcmp(t, "flows") == 0) return FLOWS_KW;
    if (strcmp(t, "into") == 0) return INTO_KW;
    if (strcmp(t, "from") == 0) return FROM_KW;
    if (strcmp(t, "origins") == 0) return ORIGINS_KW;
    if (strcmp(t, "own") == 0) return OWN;
    if (strcmp(t, "borrow") == 0) return BORROW;
    if (strcmp(t, "inout") == 0) return INOUT;
    if (strcmp(t, "this") == 0) return THIS;
    return ID;
}

int main(int argc, char **argv) {
    g_len = argc - 1;
    for (int i = 1; i < argc; i++) g_tokens[i - 1] = argv[i];
    g_pos = 0;
    return yyparse();
}
"""

EXPECTED_CONFLICTS: Dict[str, Tuple[int, int]] = {
    "0_baseline": (0, 0),
    "1a_flows_comma": (0, 0),
    "1a_flows_repeat": (0, 0),
    "1b_from_into_contrast": (1, 0),
    "A_flows_nl": (0, 1),
    "2a_view_naive": (0, 0),
    "2b_view_factored": (0, 0),
    "B_origins_collision": (0, 2),
    "3_combined_od4": (0, 0),
}

POSITIVES: Tuple[Tuple[str, str], ...] = (
    ("P1: flows single source into param", "fun push ( inout b : Bag , borrow x : Item ) flows x into b { }"),
    ("P2: flows multi source + p.origins into field path", "fun merge ( inout dst : Slot , borrow a : Item , borrow p : View ) flows a , p . origins into dst . target { }"),
    ("P3: multiple flows clauses after borrow return", "fun attach ( inout a : Slot , inout b : Slot , borrow x : Item , borrow y : Item ) : borrow Item from x flows x into a flows y into b { }"),
    ("P4: view T in generic class & fun", "view class Bag < view T : Display > { } NL fun < view T > hold ( borrow x : T ) : T from x { }"),
    ("P5: soft keywords as identifiers & type param names", "val view = origins NL class view < view : Any > { } NL fun < view view : Any > flows ( borrow into : view ) { }"),
    ("P5b: valid <view > type param named view", "fun < view > id ( ) { }"),
    ("P6: p.origins in return from clause", "fun < view T > forward ( borrow p : T ) : borrow Item from p . origins { }"),
    ("P7: nested field path + .origins source", "fun extract ( borrow p : Holder ) : borrow Item from p . inner . origins { }"),
    ("P8: bodyless interface method with flows and next member", "interface Store { fun push ( inout b : Bag , borrow x : Item ) flows x into b NL fun size ( ) : Int }"),
    ("P9: expression body after flows", "fun id ( inout dst : Slot , borrow x : Item ) : Int flows x into dst = x"),
    ("P10: this.origins into field", "fun stash ( inout dst : Slot ) flows this . origins into dst . slot { }"),
)

NEGATIVES: Tuple[Tuple[str, str], ...] = (
    ("N1: p.origins as into target (direct)", "fun bad ( inout dst : Slot , borrow x : Item ) flows x into dst . origins { }"),
    ("N2: p.origins as into target (nested)", "fun bad ( inout dst : Holder , borrow x : Item ) flows x into dst . inner . origins { }"),
    ("N3: double .origins selector", "fun bad ( borrow p : View ) : borrow Item from p . origins . origins { }"),
    ("N4: missing into target", "fun bad ( inout dst : Slot , borrow x : Item ) flows x into { }"),
    ("N5: missing flows source", "fun bad ( inout dst : Slot ) flows into dst { }"),
    ("N6: from s into v rejected", "fun bad ( inout dst : Slot , borrow x : Item ) : View from x into dst { }"),
    ("N7: trailing comma in flows source list", "fun bad ( inout dst : Slot , borrow x : Item ) flows x , into dst { }"),
    ("N8: semicolon breaking flows clause", "fun bad ( inout dst : Slot , borrow x : Item ) ; flows x into dst { }"),
    ("N9: newline breaking flows clause (strict same-line)", "fun bad ( inout dst : Slot , borrow x : Item ) NL flows x into dst { }"),
    ("N10: duplicate view modifier on type param", "fun < view view T > bad ( ) { }"),
)

SR_RE = re.compile(r"(\d+)\s+shift/reduce")
RR_RE = re.compile(r"(\d+)\s+reduce/reduce")


@dataclass(frozen=True)
class ConflictResult:
    name: str
    shift_reduce: int
    reduce_reduce: int
    returncode: int
    stderr: str


def check_grammar_conflicts(name: str, grammar: str, bison: str = "bison") -> ConflictResult:
    with tempfile.TemporaryDirectory(prefix=f"koven_f5_{name}_") as tmp:
        y_path = os.path.join(tmp, f"{name}.y")
        c_path = os.path.join(tmp, f"{name}.tab.c")
        with open(y_path, "w", encoding="utf-8") as f:
            f.write(grammar)
        proc = subprocess.run([bison, "-v", "-o", c_path, y_path], capture_output=True, text=True)
        sr_match = SR_RE.search(proc.stderr)
        rr_match = RR_RE.search(proc.stderr)
        return ConflictResult(
            name=name,
            shift_reduce=int(sr_match.group(1)) if sr_match else 0,
            reduce_reduce=int(rr_match.group(1)) if rr_match else 0,
            returncode=proc.returncode,
            stderr=proc.stderr.strip(),
        )


def run_witness_matrix(bison: str = "bison", cc: str = "cc") -> Tuple[List[Tuple[str, bool]], List[Tuple[str, bool]]]:
    with tempfile.TemporaryDirectory(prefix="koven_f5_od4_exec_") as tmp:
        y_path = os.path.join(tmp, "od4.y")
        c_path = os.path.join(tmp, "od4.tab.c")
        bin_path = os.path.join(tmp, "od4_parser")
        with open(y_path, "w", encoding="utf-8") as f:
            f.write(COMBINED_GRAMMAR)
        r1 = subprocess.run([bison, "-o", c_path, y_path], capture_output=True, text=True)
        if r1.returncode != 0:
            raise RuntimeError(f"bison failed: {r1.stderr}")
        r2 = subprocess.run(
            [cc, "-Wall", "-Wextra", "-Werror", "-Wno-unused-function", c_path, "-o", bin_path],
            capture_output=True,
            text=True,
        )
        if r2.returncode != 0:
            raise RuntimeError(f"cc failed: {r2.stderr}")
        pos_results = [
            (label, subprocess.run([bin_path] + toks.split()).returncode == 0)
            for label, toks in POSITIVES
        ]
        neg_results = [
            (label, subprocess.run([bin_path] + toks.split()).returncode != 0)
            for label, toks in NEGATIVES
        ]
        return pos_results, neg_results


def main() -> int:
    bison = shutil.which("bison")
    cc = shutil.which("cc")
    if not bison or not cc:
        raise RuntimeError("bison and cc are required in PATH")
    failed = 0
    all_grammars = {**GRAMMARS, "3_combined_od4": COMBINED_GRAMMAR}
    for name, expected in EXPECTED_CONFLICTS.items():
        res = check_grammar_conflicts(name, all_grammars[name], bison=bison)
        actual = (res.shift_reduce, res.reduce_reduce)
        ok = actual == expected and res.returncode == 0
        if not ok:
            failed += 1
        print(f"[{'PASS' if ok else 'FAIL'}] {name}: S/R={res.shift_reduce}, R/R={res.reduce_reduce} (expected {expected})")
    pos, neg = run_witness_matrix(bison=bison, cc=cc)
    for label, ok in pos:
        if not ok:
            failed += 1
        print(f"[POS {'PASS' if ok else 'FAIL'}] {label}")
    for label, ok in neg:
        if not ok:
            failed += 1
        print(f"[NEG {'PASS' if ok else 'FAIL'}] {label}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
