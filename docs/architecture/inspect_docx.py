# SPDX-License-Identifier: Apache-2.0
"""Check the finished Word package without launching Office or reading state."""
import hashlib
import json
from pathlib import Path
import struct
from xml.etree import ElementTree as E
from zipfile import ZipFile

HERE=Path(__file__).resolve().parent
N={'w':'http://schemas.openxmlformats.org/wordprocessingml/2006/main','wp':'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing','a':'http://schemas.openxmlformats.org/drawingml/2006/main'}
def main():
    with ZipFile(HERE/'airk-architecture-landscape.docx') as z:
        document=E.fromstring(z.read('word/document.xml'));section=document.find('.//w:sectPr',N)
        size=section.find('w:pgSz',N);margins=section.find('w:pgMar',N)
        val=lambda node,name:node.get('{'+N['w']+'}'+name)
        assert (val(size,'w'),val(size,'h'),val(size,'orient'))==('15840','12240','landscape')
        assert [val(margins,key) for key in ['top','bottom','left','right']]==['648','648','720','720']
        images=z.namelist();media=[name for name in images if name.startswith('word/media/')]
        expected=json.loads((HERE/'images/render-manifest.json').read_text())['images']
        hashes={hashlib.sha256(z.read(name)).hexdigest() for name in media}
        assert len(media)==6 and hashes=={row['png_sha256'] for row in expected}
        for name in media:assert struct.unpack('>II',z.read(name)[16:24])==(3200,2100)
        extents=document.findall('.//wp:extent',N)
        assert len(extents)==6 and all(node.attrib=={'cx':'9144000','cy':'6000750'} for node in extents)
        rels=E.fromstring(z.read('word/_rels/document.xml.rels'))
        assert all(rel.get('TargetMode')!='External' for rel in rels if rel.get('Type','').endswith('/image'))
        assert all(rel.get('Target','').startswith(('https://','mailto:','#')) for rel in rels if rel.get('Type','').endswith('/hyperlink'))
        body=document.find('w:body',N);figures=[p for p in body if p.find('.//wp:inline',N) is not None]
        for image in figures:
            index=list(body).index(image)
            assert body[index-1].find('w:pPr/w:pageBreakBefore',N) is not None
            assert body[index+1].find('w:pPr/w:pStyle',N).get('{'+N['w']+'}val')=='ImageCaption'
            assert body[index+2].find('w:pPr/w:pageBreakBefore',N) is not None
        for row in document.findall('.//w:tr',N):assert row.find('w:trPr/w:cantSplit',N) is not None
        result={'checks':'PASS','embedded_images':6,'exact_png_hashes':True,'geometry':'Letter landscape; 0.5in sides, 0.45in top/bottom','image_geometry_inches':[10,6.5625],'external_images':0,'figure_page_boundaries':6,'body_terms_present':all(term in ''.join(document.itertext()) for term in ['The 19 crate owners','Installation, updates and offline operation','Terms used in this guide','same OS user'])}
        assert result['body_terms_present']
        print(json.dumps(result,indent=2))
if __name__=='__main__':main()
