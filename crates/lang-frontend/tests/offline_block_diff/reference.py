"""Offline reference for the SPEC-0478 block subset, not a source lexer."""

import hashlib
from pathlib import Path
import sys

WHEEL_SHA256 = "c2276486b02f0f1b90be155f2c8ba4a8e194d42775786db622faccd652d8e80c"
GRAMMAR = """
start: _LBRACE _NL* elements _RBRACE _EOF
elements: (expr rest)?
rest: (sep _NL* elements)?
sep: _NL | _SEMI
?expr: prefix | expr _MINUS _NL* prefix -> sub
?prefix: postfix | _MINUS prefix -> neg
?postfix: ID -> name | postfix _DOT ID -> member
        | postfix _NL+ _DOT ID -> member
%declare ID _NL _MINUS _DOT _SEMI _LBRACE _RBRACE _EOF
"""


def main():
    if sys.version_info < (3, 12):
        raise SystemExit("SPEC-0478 requires Python 3.12 or newer")
    if not (sys.flags.isolated and sys.flags.no_site):
        raise SystemExit("SPEC-0478 requires Python -I -S")
    with Path(sys.argv[1]).open("rb") as stream:
        wheel_bytes = stream.read(111_037)  # One byte beyond the pinned wheel size.
    if hashlib.sha256(wheel_bytes).hexdigest() != WHEEL_SHA256:
        raise SystemExit("SPEC-0478 Lark wheel SHA-256 mismatch")
    # Only use the private batch directory created by the Rust parent under /tmp.
    # Never reopen the original wheel path or pick a directory via TMPDIR.
    wheel = Path(sys.argv[2]) / "lark.whl"
    with wheel.open("xb") as stream:
        stream.write(wheel_bytes)
    sys.path.insert(0, str(wheel))
    grammar = GRAMMAR
    if sys.argv[3:] == ["--inject-ambiguous-subtraction"]:
        rule = "expr _MINUS _NL* prefix"
        if grammar.count(rule) != 1:
            raise SystemExit("SPEC-0478 ambiguity injection target changed")
        grammar = grammar.replace(rule, "expr _MINUS _NL* expr")
    elif sys.argv[3:]:
        raise SystemExit("SPEC-0478 unknown reference argument")
    recognize_batch(grammar)


def recognize_batch(grammar):
    from lark import Lark, Token, Transformer
    from lark.exceptions import UnexpectedInput
    from lark.lexer import Lexer

    class StreamLexer(Lexer):
        def __init__(self, _config):
            pass

        def lex(self, rows):
            for kind, value in rows:
                yield Token(kind, value)

    class Shape(Transformer):
        def start(self, xs):
            return "(block" + "".join(" " + item for item in xs[0]) + ")"

        def elements(self, xs):
            return [xs[0], *xs[1]] if xs else []

        def rest(self, xs):
            return xs[-1] if xs else []

        def name(self, xs):
            return f"(id {xs[0]})"

        def neg(self, xs):
            return f"(neg {xs[0]})"

        def sub(self, xs):
            return f"(sub {xs[0]} {xs[1]})"

        def member(self, xs):
            return f"(member {xs[0]} (id {xs[1]}))"

    parser = Lark(grammar, parser="earley", lexer=StreamLexer, ambiguity="explicit")
    rows = []
    for line in sys.stdin:
        if line != "\n":
            if len(rows) == 64:
                print("SPEC-0478 event limit exceeded", file=sys.stderr)
                raise SystemExit(43)
            rows.append(line.removesuffix("\n").split("\t", 1))
            continue
        try:
            tree = parser.parse(rows)
        except UnexpectedInput:
            print("syntax-error")
        else:
            if next(tree.find_data("_ambig"), None) is not None:
                print("SPEC-0478 reference grammar is ambiguous for this input", file=sys.stderr)
                raise SystemExit(42)
            print(Shape().transform(tree))
        rows.clear()
    if rows:
        raise SystemExit("SPEC-0478 unterminated event frame")


if __name__ == "__main__":
    main()
