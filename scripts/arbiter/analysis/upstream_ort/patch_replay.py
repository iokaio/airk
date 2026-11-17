# SPDX-License-Identifier: Apache-2.0
"""Pure bounded unified-diff replay. No extraction, subprocess or fuzzy context."""
import re
MAX_BYTES=16 << 20
HEADER=re.compile(rb'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(?:[^\n]*)\n?$')

def sections(patch):
    if len(patch)>MAX_BYTES:raise ValueError('patch exceeds bound')
    lines=patch.replace(b'\r\n',b'\n').splitlines(keepends=True)
    result={};i=0
    while i<len(lines):
        if not lines[i].startswith(b'--- '):
            if lines[i][:1] in (b' ',b'-',b'+') or lines[i].startswith(b'@@'):raise ValueError('orphan patch operation')
            i+=1;continue
        old=lines[i][4:].split(b'\t')[0].strip();i+=1
        if i==len(lines) or not lines[i].startswith(b'+++ '):raise ValueError('missing new path')
        new=lines[i][4:].split(b'\t')[0].strip();i+=1
        if old.startswith(b'a/'):old=old[2:]
        if new.startswith(b'b/'):new=new[2:]
        if old!=new or any(x in (b'',b'.',b'..') for x in new.split(b'/')) or new.startswith(b'/') or b'\\' in new or b':' in new:
            raise ValueError('only same-path modifications supported')
        name=new.decode('utf-8');hunks=[]
        while i<len(lines) and lines[i].startswith(b'@@ '):
            match=HEADER.fullmatch(lines[i])
            if not match:raise ValueError('unsupported hunk header')
            start=int(match[1]);old_count=int(match[2] or b'1');new_count=int(match[4] or b'1');i+=1;ops=[];oc=nc=0
            while i<len(lines) and (oc<old_count or nc<new_count):
                line=lines[i];i+=1
                if line[:1] not in (b' ',b'-',b'+'):raise ValueError('unsupported hunk body')
                kind=line[:1];body=line[1:]
                if i<len(lines) and lines[i].startswith(b'\\ No newline at end of file'):
                    body=body.removesuffix(b'\n');i+=1
                oc+=kind!=b'+';nc+=kind!=b'-';ops.append((kind,body))
            if (oc,nc)!=(old_count,new_count):raise ValueError('hunk count differs')
            hunks.append((start,old_count,int(match[3]),new_count,ops))
        if not hunks or name in result:raise ValueError('empty or duplicate file patch')
        result[name]=hunks
    if not result:raise ValueError('no supported file patches')
    return result

def apply(source,hunks):
    if len(source)>MAX_BYTES:raise ValueError('source exceeds bound')
    lines=source.splitlines(keepends=True);out=[];cursor=0
    for start,count,new_start,new_count,ops in hunks:
        index=start-1 if count else start
        if index<cursor or index>len(lines):raise ValueError('overlapping/outside hunk')
        out.extend(lines[cursor:index]);cursor=index
        expected_new_index=new_start-1 if new_count else new_start
        if expected_new_index!=len(out):raise ValueError('new hunk position differs')
        for kind,body in ops:
            if kind!=b'+':
                if cursor>=len(lines) or lines[cursor]!=body:raise ValueError('exact patch context differs')
                cursor+=1
            if kind!=b'-':out.append(body)
    out.extend(lines[cursor:]);result=b''.join(out)
    if len(result)>MAX_BYTES:raise ValueError('patched output exceeds bound')
    return result
