# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import sys,re,json,subprocess,hashlib,datetime
root=Path.cwd().resolve()
def digest(b):return hashlib.sha256(b).hexdigest()
def security(paths):
    code="@($input | ConvertFrom-Json) | ForEach-Object { $a=Get-Acl -LiteralPath $_; [pscustomobject]@{path=$_;owner=$a.Owner;sddl=$a.Sddl} } | ConvertTo-Json -Compress"
    p=subprocess.run(['pwsh','-NoProfile','-Command',code],input=json.dumps([str(p) for p in paths]),text=True,capture_output=True,check=True)
    v=json.loads(p.stdout);return [v] if isinstance(v,dict) else v
patches=[Path(v) for v in sys.argv[1:]]
changes={}; inputs={}
for patch in patches:
    lines=patch.read_text(encoding='utf-8-sig').splitlines(); i=0
    while i<len(lines):
        if not lines[i].startswith('--- '):i+=1;continue
        oldname=lines[i][4:].split('\t')[0];newname=lines[i+1][4:].split('\t')[0];i+=2
        name=newname[2:] if newname.startswith('b/') else newname
        if name=='/dev/null':raise RuntimeError('Deletion requires separate review')
        dest=(root/name).resolve();assert dest.is_relative_to(root)
        base=changes.get(dest,dest.read_bytes() if dest.exists() else b'');inputs.setdefault(dest,dest.read_bytes() if dest.exists() else None)
        source=base.decode('utf-8-sig').splitlines(keepends=True);pos=0; result=[]
        while i<len(lines) and not lines[i].startswith(('--- ','diff --git ')):
            if not lines[i].startswith('@@ '):i+=1;continue
            m=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',lines[i]);assert m,lines[i]
            nominal=max(0,int(m[1])-1);i+=1; h=[]
            while i<len(lines) and not lines[i].startswith(('@@ ','diff --git ','--- ')):
                if lines[i].startswith((' ','+','-')):h.append(lines[i])
                elif lines[i].startswith('\\ No newline'):pass
                else:break
                i+=1
            before=[v[1:] for v in h if v[0] in ' -']; after=[v[1:] for v in h if v[0] in ' +']
            candidates=[j for j in range(pos,len(source)+1) if [v.rstrip('\r\n') for v in source[j:j+len(before)]]==before]
            if nominal in candidates:j=nominal
            elif len(candidates)==1:j=candidates[0]
            else:raise RuntimeError(f'{name}: ambiguous/unmatched hunk {nominal}, candidates {candidates[:8]}')
            result.extend(source[pos:j]); ending='\r\n' if any(x.endswith('\r\n') for x in source) else '\n';result.extend(v+ending for v in after);pos=j+len(before)
        result.extend(source[pos:]);changes[dest]=''.join(result).encode('utf-8')
existing=[p for p,b in inputs.items() if b is not None];pre=security(existing)
for p,b in inputs.items():assert (p.read_bytes() if p.exists() else None)==b,f'Concurrent change {p}'
for p,b in changes.items():p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b)
post=security(existing);assert pre==post,'Owner or ACL changed'
receipt={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'patches':[str(p) for p in patches],'ownership_and_acl_unchanged':True,'security':pre,'files':[{'path':str(p.relative_to(root)),'before_sha256':digest(inputs[p]) if inputs[p] is not None else None,'after_sha256':digest(b)} for p,b in changes.items()]}
receipt_path=root/'artifacts/v190/final-plan/exec-20261116T052942Z'/('applied-'+datetime.datetime.now().strftime('%H%M%S%f')+'.json');receipt_path.write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({'applied_files':len(changes),'receipt':str(receipt_path)}))
