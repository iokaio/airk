# SPDX-License-Identifier: Apache-2.0
"""Compose only complete byte-preserved Rust-emitted descriptor rows; no DTO mirror."""
from pathlib import Path
import hashlib,json
HERE=Path(__file__).resolve().parent
MODELS=HERE.parent
SELECTION=[('candidate-u8','visible-prefix-mining-r1-u8'),('candidate-s8','visible-prefix-mining-r1-s8'),('control-shipped-u8','original-shipped-u8-ort130-evaluation'),('control-mixed-u8','original-mixed-u8-ort130-evaluation'),('diagnostic-shipped-s8','original-shipped-s8-ort130-evaluation'),('diagnostic-mixed-s8','original-mixed-s8-ort130-evaluation')]
INPUTS=[(MODELS/'candidate-native-emission-r20/master.json','0b58b97ee107c9c2c3e4adfefa7b6bf215a9c2d59a1966167b81e3f88224f718'),(MODELS/'native-encoder-qualification-input-r1.json','3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81')]
def sha(raw):return hashlib.sha256(raw).hexdigest()
def row_slices(raw):
    text=raw.decode('utf-8');decoder=json.JSONDecoder();position=0
    while text[position].isspace():position+=1
    if text[position]!='[':raise ValueError('owning Rust descriptor array required')
    position+=1;result=[]
    while True:
        while text[position].isspace():position+=1
        if text[position]==']':break
        value,end=decoder.raw_decode(text,position)
        result.append((value,text[position:end].encode('utf-8')));position=end
        while text[position].isspace():position+=1
        if text[position]==',':position+=1
        elif text[position]!=']':raise ValueError('descriptor syntax differs')
    return result
def main():
    rows={}
    for path,pin in INPUTS:
        raw=path.read_bytes()
        if sha(raw)!=pin:raise ValueError('original Rust descriptor source changed')
        for row,exact in row_slices(raw):
            if row['family'] in rows:raise ValueError('duplicate descriptor family')
            rows[row['family']]=(row,exact,path,pin)
    output=HERE/'native-six-master.json';proof=HERE/'native-six-composition-proof.json'
    if output.exists() or proof.exists():raise ValueError('never overwrite descriptor composition')
    chosen=[rows[family] for role,family in SELECTION]
    final=b'[\n'+b',\n'.join(row[1] for row in chosen)+b'\n]\n'
    parsed=json.loads(final)
    if parsed!=[row[0] for row in chosen]:raise ValueError('parsed descriptor equality differs')
    output.write_bytes(final)
    proof.write_text(json.dumps({'schema':'airk.private-native-descriptor-row-composition-proof/1','output_sha256':sha(final),'owning_Rust_type':'evaluation_tests::EncoderInput; final_family accepts exact six closed roles','scope':'Whole original Rust-emitted row bytes copied without key/value edits; no Python native type/schema/field mirror introduced; only list membership/order composes approved six roles.','rows':[{'role':role,'family':family,'source':str(row[2]),'source_sha256':row[3],'exact_row_sha256':sha(row[1])} for (role,family),row in zip(SELECTION,chosen)],'native_execution':False,'FINAL':False},indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'native_six_master_sha256':sha(final),'byte_preserved_rows':6,'native_execution':False}))
if __name__=='__main__':main()
