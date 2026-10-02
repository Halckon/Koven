#!/usr/bin/env python3
"""Read-only reproduction of the fixed P2 iteration-test migration evidence.

Run from the repository root with Rust 1.96.0/rustfmt on PATH. This is a frozen
migration artifact, not a rolling production check or a replacement for tests.
"""
import difflib
import hashlib
import json
import pathlib
import re
import subprocess

E = pathlib.Path(__file__).resolve().parent
data = json.loads((E / "drop-iteration-test-migration.json").read_text())
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

def relocate(body):
    toks=lex(body);at=[]
    for i,(t,a,b) in enumerate(toks):
        if t=='super' and i+1<len(toks) and toks[i+1][0]=='::' and (not i or toks[i-1][0]!='::'):at.append(a)
    for a in reversed(at):body=body[:a]+'super::'+body[a:]
    return body,len(at)

def tokens(s):return [x[0] for x in lex(s)]
def blocks(s):
    lines=s.splitlines(keepends=True);starts=[]
    for i,l in enumerate(lines):
        if re.match(r'^(?:fn |struct )',l):
            a=i
            while a and (lines[a-1].startswith('#[') or lines[a-1].startswith('///')):a-=1
            starts.append((a,re.search(r'(?:fn|struct) (\w+)',l)[1]))
    rows={}
    for k,(a,n) in enumerate(starts):
        b=starts[k+1][0] if k+1<len(starts) else len(lines)
        body=''.join(lines[a:b]).rstrip()
        if '\nmod ' in body:body=body.split('\nmod ',1)[0].rstrip()
        rows[n]=body
    return rows

def literal(t):return re.match(r'(?:br|cr|r)#*"|(?:b|c)?"|(?:b)?\'.*\'$',t)

def superpaths(body,module):
    ts=lex(body);paths=[]
    for i,(t,a,b) in enumerate(ts):
        if t!='super' or (i and ts[i-1][0]=='::'):continue
        j=i;parts=[]
        while j<len(ts) and re.fullmatch(r'[A-Za-z_]\w*',ts[j][0]):
            parts.append(ts[j][0]);j+=1
            if j>=len(ts) or ts[j][0]!='::':break
            j+=1
        depth=0
        while parts[depth]=='super':depth+=1
        paths.append({'path':'::'.join(parts),'resolved':'::'.join(module[:-depth]+parts[depth:]),'line':body[:a].count('\n')+1})
    return paths

old=subprocess.check_output(['git','show',BASE+':'+str(P)]).decode()
rows=data['inventory'];
for r in rows:r['body']=''.join(old.splitlines(keepends=True)[r['start']-1:r['end']])
mapping=data['mapping'];byname={r['name']:r for r in mapping}
parent=P.with_suffix('')/'tests.rs';new={};filemap={}
for p in [parent,*sorted(parent.with_suffix('').glob('*.rs'))]:
    for n,b in blocks(p.read_text()).items():
        assert n not in new,n
        new[n]=b;filemap[n]=p
assert len(new)==len(rows)==51
hashrows=[];format_edits=[];path_edits=[]
module='ownership_checking::checker::drop_planner::iteration::tests'.split('::')
for r in rows:
    moved=r['name'] in byname
    relocated,count=relocate(r['body']) if moved else (r['body'],0)
    # Generate an independent rustfmt reference from the ORIGINAL complete block.
    result=subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=relocated,text=True,capture_output=True,check=True)
    expected=result.stdout.rstrip()
    assert expected==new[r['name']],r['name']
    raw=tokens(relocated);a=tokens(expected);b=tokens(new[r['name']]);assert a==b
    matcher=difflib.SequenceMatcher(None,raw,a,autojunk=False)
    edits=[]
    for tag,i,j,k,l in matcher.get_opcodes():
        if tag!='equal':
            oldtokens=raw[i:j];newtokens=a[k:l]
            # No identifier, keyword, operator, literal, or comment change is permitted.
            assert set(oldtokens+newtokens)<={'{','}',','},(r['name'],oldtokens,newtokens)
            edits.append({'old_token_index':i,'new_token_index':k,'old':oldtokens,'new':newtokens})
    if edits:format_edits.append({'name':r['name'],'edits':edits})
    oldl=[t for t in tokens(r['body']) if literal(t)];newl=[t for t in b if literal(t)];assert oldl==newl
    oldp=superpaths(r['body'],module);newp=superpaths(new[r['name']],module+[byname[r['name']]['module']] if moved else module)
    assert len(oldp)==len(newp)==count
    for a0,b0 in zip(oldp,newp):
        assert a0['resolved']==b0['resolved']
        assert b0['path']=='super::'+a0['path']
        path_edits.append({'name':r['name'],'old_line':r['start']+a0['line']-1,'new_file':str(filemap[r['name']]),'old_path':a0['path'],'new_path':b0['path'],'resolved_path':a0['resolved']})
    hashrows.append({'name':r['name'],'test':r['test'],'module':byname.get(r['name'],{}).get('module','tests'),
        'original_block_sha256':hashlib.sha256(r['body'].encode()).hexdigest(),
        'new_block_sha256':hashlib.sha256(new[r['name']].encode()).hexdigest(),
        'literal_sha256':hashlib.sha256(json.dumps(oldl,ensure_ascii=False).encode()).hexdigest(),
        'literal_count':len(oldl),'super_adjustments':count,'format_token_edits':len(edits)})
assert P.read_text()==old.split('mod tests {',1)[0]+'mod tests;\n'
oldheader=old.split('mod tests {\n',1)[1].split('    /// 测试回放',1)[0]
newheader=parent.read_text().split('/// 测试回放',1)[0]
assert tokens(oldheader)==tokens(newheader)
changed=subprocess.check_output(['git','diff','--name-only',BASE]).decode().splitlines()
assert not any(x.endswith('.rs') and x!=str(P) and not x.startswith(str(P.with_suffix('')/'tests')) for x in changed)
assert hashrows == data['fidelity']
assert format_edits == data['format_token_edits']
assert path_edits == data['super_path_mapping']
print('40 tests + 11 helper blocks equal independent same-rustfmt references; 169 exact path resolutions checked')
print('literal tokens',sum(r['literal_count'] for r in hashrows),'byte-identical')
print('rustfmt edit blocks',len(format_edits),'edits',sum(len(x['edits']) for x in format_edits))
print('production prefix and instance_replay declarations byte-identical; imports token-identical; no new pub')
print('literal aggregate',hashlib.sha256('\n'.join(r['literal_sha256'] for r in sorted(hashrows,key=lambda r:r['name'])).encode()).hexdigest())
