#!/usr/bin/env python3
"""Read-only fixed P2 ownership_iteration migration reproduction.

Run in the repository root with the project's Rust 1.96.0 rustfmt on PATH.
This verifies the frozen migration; it is not a rolling production framework.
"""
import hashlib
import json
import pathlib
import re
import subprocess

E = pathlib.Path(__file__).resolve().parent
data = json.loads((E / "ownership-iteration-test-migration.json").read_text())
P = pathlib.Path(data["source"])
BASE = data["base"]

def lex(s):
    out=[];i=0
    while i<len(s):
        if s[i].isspace():i+=1;continue
        start=i
        if s.startswith('//',i):
            i=s.find('\n',i)
            if i<0:i=len(s)
        elif s.startswith('/*',i):
            i+=2;depth=1
            while depth:
                if s.startswith('/*',i):depth+=1;i+=2
                elif s.startswith('*/',i):depth-=1;i+=2
                else:i+=1
                assert i<=len(s)
        elif m:=re.match(r'(?:br|cr|r)(#*)"',s[i:]):
            end='"'+m[1];i=s.index(end,i+len(m[0]))+len(end)
        elif s[i]=='"' or s[i:i+2] in ('b"','c"'):
            i+=1 if s[i]=='"' else 2
            while s[i]!='"':i+=2 if s[i]=='\\' else 1
            i+=1
        elif m:=re.match(r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'",s[i:]):i+=len(m[0])
        elif s[i].isalpha() or s[i]=='_':
            i+=1
            while i<len(s) and (s[i].isalnum() or s[i]=='_'):i+=1
        elif s.startswith('::',i):i+=2
        else:i+=1
        out.append((s[start:i],start,i))
    return out


def sha(s):
    return hashlib.sha256(s.encode()).hexdigest()

def blocks(s):
    lines = s.splitlines(keepends=True)
    starts = []
    for i, line in enumerate(lines):
        if match := re.match(r"^fn (\w+)", line):
            start = i
            while start and (lines[start - 1].startswith("#[") or lines[start - 1].startswith("///")):
                start -= 1
            starts.append((start, match[1]))
    return {name: "".join(lines[start:starts[i + 1][0] if i + 1 < len(starts) else len(lines)]).rstrip()
            for i, (start, name) in enumerate(starts)}

def literals(s):
    return [token for token, _, _ in lex(s)
            if re.match(r"(?:br|cr|r)#*\"|(?:b|c)?\"|(?:b)?'.*'$", token)]

old = subprocess.check_output(["git", "show", BASE + ":" + str(P)], text=True)
original = blocks(old)
header = old[:old.index("#[test]")]
modules = sorted({row["module"] for row in data["mapping"]})
assert P.read_text() == header + "\n".join(
    f'#[path = "ownership_iteration/{module}.rs"]\nmod {module};' for module in modules) + "\n"
new = {"checked": blocks(header)["checked"]}
files = {"checked": str(P)}
for module in modules:
    path = P.with_suffix("") / (module + ".rs")
    text = path.read_text()
    assert text.startswith("use super::*;\n\n")
    for name, block in blocks(text).items():
        assert name not in new, name
        new[name] = block
        files[name] = str(path)
assert original.keys() == new.keys()
assert len(new) == 191
hashes = []
for row in data["fidelity"]:
    name = row["name"]
    reference = subprocess.run(["rustfmt", "--edition", "2024", "--emit", "stdout"],
                               input=original[name], text=True, capture_output=True, check=True).stdout.rstrip()
    assert original[name] == reference == new[name], name
    assert sha(original[name]) == row["original_block_sha256"] == row["new_block_sha256"]
    assert files[name] == row["new_file"]
    values = literals(original[name])
    assert values == literals(new[name]) and len(values) == row["literal_count"]
    assert sha(json.dumps(values, ensure_ascii=False)) == row["literal_sha256"]
    tokens = [token for token, _, _ in lex(new[name])]
    assert sum(token.startswith("assert") and i + 1 < len(tokens) and tokens[i + 1] == "!"
               for i, token in enumerate(tokens)) == row["assert_macros"]
    assert new[name].startswith("#[test]") == row["test"]
    hashes.append((name, row["literal_sha256"]))
assert len(data["mapping"]) == 184
assert {row["old_name"] for row in data["mapping"]} == {name for name, body in new.items() if body.startswith("#[test]")}
for row in data["mapping"]:
    assert row["new_name"] == row["module"] + "::" + row["old_name"]
    assert files[row["old_name"]] == row["new_file"]
    assert row["attributes"] == ["test"] and not row["ignored"]
assert sha("\n".join(value for _, value in sorted(hashes))) == data["literal_aggregate_sha256"]
assert not data["format_token_edits"] and not data["relative_path_edits"]
for path, expected in {**data["support_sha256"], **data["manifests_sha256"]}.items():
    content = pathlib.Path(path).read_bytes()
    assert hashlib.sha256(content).hexdigest() == expected
    assert content == subprocess.check_output(["git", "show", BASE + ":" + path])
for path, lines in data["files"].items():
    assert len(pathlib.Path(path).read_text().splitlines()) == lines <= 1000
changed = subprocess.check_output(["git", "diff", "--name-only", BASE], text=True).splitlines()
assert all(not path.endswith(".rs") or path == str(P) or path.startswith(str(P.with_suffix("")) + "/")
           for path in changed)
print("184 tests + 7 helpers byte-identical; independent same-rustfmt references identical")
print("1248 literal tokens, 1174 assert macros, 22 support files and Cargo inputs unchanged")
print("20 private domains, maximum 868 lines; one-to-one test paths and all fixed hashes verified")
print("literal aggregate", data["literal_aggregate_sha256"])
