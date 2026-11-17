# SPDX-License-Identifier: Apache-2.0
"""Private tokenizer admission against WHOLE sealed Rust-emitted tensors.
Uses tokenizer's own padding on a new Encoding; never edits/crops native arrays.
"""
import hashlib,json
NAMES=('input_ids','attention_mask','token_type_ids')
def native_unit_admission(tokenizer,text,native,admission):
 if admission!='embeddable' or not isinstance(native,dict) or set(native)!=set(NAMES):raise ValueError('native embeddable tensor fields required')
 width=None
 for name in NAMES:
  rows=native[name]
  if not isinstance(rows,list) or len(rows)!=1 or not isinstance(rows[0],list) or not rows[0] or any(type(v)is not int or v<0 for v in rows[0]):raise ValueError('exact one-row integer native tensors required')
  if width is None:width=len(rows[0])
  if len(rows[0])!=width:raise ValueError('native tensor widths disagree')
 if width>256:raise ValueError('native input exceeds unchanged256context')
 encoding=tokenizer.encode(text);raw_count=len(encoding.ids)
 if not 2<=raw_count<=256 or raw_count>width:raise ValueError('uncropped tokenizer input cannot fit exact native tensor')
 pad_id=tokenizer.token_to_id('[PAD]')
 if pad_id is None:raise ValueError('owning tokenizer PAD missing')
 encoding.pad(width,pad_id=pad_id,pad_type_id=0,pad_token='[PAD]',direction='right')
 expected={'input_ids':encoding.ids,'attention_mask':encoding.attention_mask,'token_type_ids':encoding.type_ids}
 for name in NAMES:
  if expected[name]!=native[name][0]:raise ValueError('whole native tensor differs after tokenizer-owned padding: '+name)
 return {'raw_tokens':raw_count,'native_width':width,'padding_tokens':width-raw_count,'whole_tensors_match':True,'native_tensor_sha256':hashlib.sha256(json.dumps(native,separators=(',',':')).encode()).hexdigest()}
