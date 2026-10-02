#!/usr/bin/env python3
"""Read-only reproduction of the fixed P2 multifile type-test migration.

Run at repository root with project Rust 1.96.0 rustfmt on PATH.
This frozen acceptance verifier is not a rolling test framework.
"""
import hashlib
import json
import pathlib
import re
import subprocess

E = pathlib.Path(__file__).resolve().parent
data = json.loads((E / "multifile-type-test-migration.json").read_text())
BASE = data["base"]
P = pathlib.Path(data["source"])
B = pathlib.Path(data["baseline_regressions"])

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


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def base_text(path):
    return subprocess.check_output(["git", "show", BASE + ":" + str(path)], text=True)


def blocks(text):
    if "#[path =" in text:
        text = text[:text.index("#[path =")]
    lines = text.splitlines(keepends=True)
    starts = []
    result = {}
    for i, line in enumerate(lines):
        if match := re.match(r"^fn (\w+)", line):
            start = i
            while start and (lines[start - 1].startswith("#[") or lines[start - 1].startswith("///")):
                start -= 1
            starts.append((start, match[1]))
    for i, (start, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(lines)
        body = "".join(lines[start:end]).rstrip()
        result[name] = (body, start + 1, start + len(body.splitlines()))
    return result


old = base_text(P)
original = blocks(old)
baseline = blocks(base_text(B))
assert not original.keys() & baseline.keys()
original.update(baseline)
assert B.read_text() == base_text(B)
imports = old[:old.index("fn parsed")]
root = imports + "\n\n".join(original[name][0] for name in data["root_helpers"]) + "\n\n"
modules = sorted([*data["groups"], "baseline_regressions"])
root += "\n".join(f'#[path = "multifile_type_checking/{module}.rs"]\nmod {module};' for module in modules) + "\n"
assert P.read_text() == root
for module, names in data["groups"].items():
    path = P.with_suffix("") / (module + ".rs")
    expected = "use super::*;\n\n" + "\n\n".join(original[name][0] for name in names) + "\n"
    assert path.read_text() == expected, str(path)
new = {}
files = {}
for path in [P, *sorted(P.with_suffix("").glob("*.rs"))]:
    for name, value in blocks(path.read_text()).items():
        assert name not in new
        new[name] = value
        files[name] = str(path)
assert new.keys() == original.keys() and len(new) == 120
literal_hashes = []
for row in data["fidelity"]:
    name = row["name"]
    original_body, start, end = original[name]
    new_body, new_start, new_end = new[name]
    assert (start, end) == (row["old_start"], row["old_end"])
    assert (new_start, new_end) == (row["new_start"], row["new_end"])
    reference = subprocess.run(["rustfmt", "--edition", "2024", "--emit", "stdout"],
                               input=original_body, text=True, capture_output=True, check=True).stdout.rstrip()
    assert original_body == reference == new_body, name
    assert sha(original_body) == row["original_block_sha256"] == row["new_block_sha256"]
    assert files[name] == row["new_file"]
    tokens = [token for token, _, _ in lex(new_body)]
    literals = [token for token in tokens if re.match(r"(?:br|cr|r)#*\"|(?:b|c)?\"|(?:b)?'.*'$", token)]
    assert len(literals) == row["literal_count"]
    assert sha(json.dumps(literals, ensure_ascii=False)) == row["literal_sha256"]
    assert sum(token.startswith("assert") and i + 1 < len(tokens) and tokens[i + 1] == "!"
               for i, token in enumerate(tokens)) == row["assert_macros"]
    assert new_body.startswith("#[test]") == row["test"]
    literal_hashes.append((name, row["literal_sha256"]))
assert sha("\n".join(value for _, value in sorted(literal_hashes))) == data["literal_aggregate_sha256"]
assert len(data["mapping"]) == 107
for row in data["mapping"]:
    name = row["old_name"].split("::")[-1]
    assert row["old_name"] == ("baseline_regressions::" if name in baseline else "") + name
    assert row["new_name"] == row["module"] + "::" + name
    assert row["new_file"] == files[name] and row["attributes"] == ["test"] and not row["ignored"]
assert sorted(data["libtest_identity_before"]) == sorted(row["old_name"] for row in data["mapping"])
assert sorted(data["libtest_identity_after"]) == sorted(row["new_name"] for row in data["mapping"])
assert len(data["frontend_library_identities"]) == 187 and data["target_count"] == 129
assert not data["format_token_edits"] and not data["relative_path_edits"]
for path, expected in {**data["support_sha256"], **data["unchanged_inputs_sha256"]}.items():
    content = pathlib.Path(path).read_bytes()
    assert hashlib.sha256(content).hexdigest() == expected
    assert content == subprocess.check_output(["git", "show", BASE + ":" + path])
for path, lines in data["files"].items():
    assert len(pathlib.Path(path).read_text().splitlines()) == lines <= 1000
old_policy = json.loads(base_text("scripts/rust-size-policy.json"))
new_policy = json.loads(pathlib.Path("scripts/rust-size-policy.json").read_text())
assert old_policy["baseline"].pop(str(P)) == 7202
assert old_policy == new_policy
changed = subprocess.check_output(["git", "diff", "--name-only", BASE], text=True).splitlines()
assert all(not path.endswith(".rs") or path == str(P) or path.startswith(str(P.with_suffix("")) + "/")
           for path in changed)
print("107 tests + 13 helpers byte-identical; independent same-rustfmt references identical")
print("1559 literal tokens, 781 assert macros, baseline regressions, support and Cargo/CI inputs unchanged")
print("17 new private domains; entry 160 lines, maximum domain 732; 129 targets and 187 library identities")
print("Only the original 7202-line baseline retired; all other policy entries unchanged")
print("literal aggregate", data["literal_aggregate_sha256"])
