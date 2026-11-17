# SPDX-License-Identifier: Apache-2.0
"""Mechanical projection of existing Rust-written JSON, never a Profile serializer."""
import json
WS=b' \t\r\n'
def compact_literal(raw):
 json.loads(raw)
 result=bytearray();quoted=False;escape=False
 for byte in raw:
  if quoted:
   result.append(byte)
   if escape:escape=False
   elif byte==92:escape=True
   elif byte==34:quoted=False
  elif byte==34:quoted=True;result.append(byte)
  elif byte not in WS:result.append(byte)
 if quoted or escape:raise ValueError('unterminated string')
 return bytes(result)
def array_member(raw,name):
 json.loads(raw)
 def skip(i):
  while i<len(raw)and raw[i]in WS:i+=1
  return i
 def string_end(i):
  i+=1;escape=False
  while i<len(raw):
   byte=raw[i];i+=1
   if escape:escape=False
   elif byte==92:escape=True
   elif byte==34:return i
  raise ValueError('unterminated string')
 def value_end(i):
  if raw[i]==34:return string_end(i)
  if raw[i]in(91,123):
   stack=[raw[i]];i+=1
   while stack:
    if raw[i]==34:i=string_end(i);continue
    if raw[i]in(91,123):stack.append(raw[i])
    elif raw[i]in(93,125):
     expected=91 if raw[i]==93 else 123
     if stack.pop()!=expected:raise ValueError('crossed JSON container')
    i+=1
   return i
  while i<len(raw)and raw[i]not in WS+b',}':i+=1
  return i
 i=skip(0)
 if raw[i]!=123:raise ValueError('root object required')
 i=skip(i+1);found=None;keys=set()
 while raw[i]!=125:
  if raw[i]!=34:raise ValueError('object key required')
  end=string_end(i);key=json.loads(raw[i:end]);i=skip(end)
  if key in keys:raise ValueError('duplicate root key')
  keys.add(key)
  if raw[i]!=58:raise ValueError('colon required')
  start=skip(i+1);end=value_end(start)
  if key==name:
   if raw[start]!=91:raise ValueError('actual native array required')
   found=raw[start:end]
  i=skip(end)
  if raw[i]==44:i=skip(i+1)
  elif raw[i]!=125:raise ValueError('object delimiter required')
 if found is None:raise ValueError('native array absent')
 return compact_literal(found)
