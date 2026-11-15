# SPDX-License-Identifier: Apache-2.0
"""Build the self-contained Letter-landscape architecture document.

Requires existing Pandoc; finishing uses Python stdlib OOXML, not python-docx.
Run after render_diagrams.py. No network, product build or live service input.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from urllib.parse import unquote
from xml.etree import ElementTree as E
from zipfile import ZipFile,ZIP_DEFLATED

HERE=Path(__file__).resolve().parent
W='http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R='http://schemas.openxmlformats.org/officeDocument/2006/relationships'
WP='http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing'
A='http://schemas.openxmlformats.org/drawingml/2006/main'
REL='http://schemas.openxmlformats.org/package/2006/relationships'
CT='http://schemas.openxmlformats.org/package/2006/content-types'
NS={'w':W,'r':R,'wp':WP,'a':A}
for prefix,uri in NS.items():E.register_namespace(prefix,uri)

def element(parent,name,**attrs):
    return E.SubElement(parent,'{'+W+'}'+name,{'{'+W+'}'+k:str(v) for k,v in attrs.items()})
def props(node,name='pPr'):
    found=node.find('w:'+name,NS)
    if found is None:found=E.Element('{'+W+'}'+name);node.insert(0,found)
    return found
def xml(node):return E.tostring(node,encoding='utf-8',xml_declaration=True)
def text(node):return ''.join(node.itertext())
def paragraph(content,style=None):
    p=E.Element('{'+W+'}p')
    if style:element(props(p),'pStyle',val=style)
    element(element(p,'r'),'t').text=content
    return p
def set_spacing(p,before=0,after=0):element(props(p),'spacing',before=before,after=after)

def main():
    pandoc=shutil.which('pandoc')
    if not pandoc:raise SystemExit('Pandoc must be installed explicitly; no dependency is downloaded.')
    output=HERE/'airk-architecture-landscape.docx'
    with tempfile.TemporaryDirectory(prefix='airk-word-',dir=HERE) as temp:
        raw=Path(temp)/'base.docx'
        command=[pandoc,str(HERE/'airk-architecture.md'),'--from=gfm','--to=docx','--standalone','--resource-path='+str(HERE),'--output='+str(raw)]
        subprocess.run(command,check=True)
        with ZipFile(raw) as archive:files={name:archive.read(name) for name in archive.namelist()}
    document=E.fromstring(files['word/document.xml']);body=document.find('w:body',NS)
    section=body.find('w:sectPr',NS)
    for name in ['pgSz','pgMar']:
        for old in section.findall('w:'+name,NS):section.remove(old)
    element(section,'pgSz',w=15840,h=12240,orient='landscape')
    element(section,'pgMar',top=648,bottom=648,left=720,right=720,header=250,footer=250,gutter=0)
    element(section,'pgNumType',start=1)
    captions=['Product interaction and complete native evidence.','Matched executables, protocol planes and in-process service owners.','Selected native dependency direction; full owner and edge inventories follow.','Read-only source capture, derived publication and owned maintenance.','Original global budgets, optional local advice and guarded final delivery.','Separate grants and broad same-OS-user choice when explicitly enabled.']
    images=[p for p in body if p.find('.//wp:inline',NS) is not None]
    assert len(images)==6
    for number,p in enumerate(images,1):
        index=list(body).index(p);heading=body[index-1]
        assert heading.find('w:pPr/w:pStyle',NS) is not None
        element(props(heading),'pageBreakBefore');element(props(heading),'keepNext');set_spacing(heading,0,100)
        for r in heading.findall('.//w:r',NS):element(props(r,'rPr'),'sz',val=32)
        pr=props(p);element(pr,'jc',val='center');element(pr,'keepNext');set_spacing(p,0,0)
        # 10.0 x 6.5625 inches: exact 3200:2100 aspect, never stretched.
        for extent in p.findall('.//wp:extent',NS)+p.findall('.//a:xfrm/a:ext',NS):extent.set('cx','9144000');extent.set('cy','6000750')
        for inline in p.findall('.//wp:inline',NS):
            for key in ['distT','distB','distL','distR']:inline.set(key,'0')
        caption=paragraph(f'Figure {number}. {captions[number-1]}','ImageCaption');set_spacing(caption,60,0)
        element(props(caption),'jc',val='center');body.insert(index+1,caption)
        following=body[index+2]
        if text(following)=='Open full-size PNG':body.remove(following)
        following=body[list(body).index(caption)+1]
        assert following.tag=='{'+W+'}p'
        element(props(following),'pageBreakBefore')
    styles=E.fromstring(files['word/styles.xml'])
    for style in styles.findall('w:style',NS):
        sid=style.get('{'+W+'}styleId','');rp=props(style,'rPr')
        element(rp,'rFonts',ascii='Calibri',hAnsi='Calibri',eastAsia='Calibri',cs='Calibri')
        size=22
        if sid.startswith('Heading'):size={'Heading1':42,'Heading2':32,'Heading3':26}.get(sid,24);element(rp,'color',val='172B46')
        if sid in ['ImageCaption','Table','TableCaption','SourceCode','VerbatimChar']:size=20
        if sid=='Hyperlink':element(rp,'color',val='2563A6')
        for old in rp.findall('w:sz',NS)+rp.findall('w:szCs',NS):rp.remove(old)
        element(rp,'sz',val=size);element(rp,'szCs',val=size)
        if style.get('{'+W+'}type')=='paragraph':
            pp=props(style);element(pp,'widowControl');element(pp,'spacing',after=100,line=250,lineRule='auto')
            if sid.startswith('Heading'):element(pp,'keepNext')
    # Prevent split rows and repeat the header; body paragraphs stay at 11 pt.
    for table in document.findall('.//w:tbl',NS):
        tp=props(table,'tblPr');element(tp,'tblW',w=14400,type='dxa');element(tp,'tblLayout',type='fixed')
        grid=table.find('w:tblGrid',NS)
        if grid is not None:
            columns=list(grid);widths=[3000,11400] if len(columns)==2 else [14400//len(columns)]*len(columns)
            for column,width in zip(columns,widths):column.set('{'+W+'}w',str(width))
        for index,row in enumerate(table.findall('w:tr',NS)):
            trp=props(row,'trPr');element(trp,'cantSplit')
            if index==0:element(trp,'tblHeader')
            for cell_index,cell in enumerate(row.findall('w:tc',NS)):
                tcp=props(cell,'tcPr');element(tcp,'vAlign',val='top')
                if grid is not None:element(tcp,'tcW',w=widths[cell_index],type='dxa')
                if index==0:element(tcp,'shd',fill='EEF5FC')
                for para in cell.findall('.//w:p',NS):
                    set_spacing(para,30,55)
                    for run in para.findall('.//w:r',NS):element(props(run,'rPr'),'sz',val=20)
    rels=E.fromstring(files['word/_rels/document.xml.rels'])
    for relation in rels:
        target=relation.get('Target','')
        if relation.get('Type')==R+'/hyperlink' and not target.startswith(('https:','http:','mailto:','#')):
            try:
                resolved=(HERE/unquote(target.split('#')[0])).resolve().relative_to(HERE.parents[1]).as_posix()
            except ValueError:
                continue
            anchor='#'+target.split('#',1)[1] if '#' in target else ''
            mode='tree' if (HERE/unquote(target.split('#')[0])).is_dir() else 'blob'
            relation.set('Target','https://github.com/iokaio/airk/'+mode+'/feat/advanced-split/'+resolved+anchor)
    for kind in ['header','footer']:
        rid='rIdArchitecture'+kind.title();E.SubElement(rels,'{'+REL+'}Relationship',{'Id':rid,'Type':R+'/'+kind,'Target':kind+'-architecture.xml'})
        reference=E.Element('{'+W+'}'+kind+'Reference',{'{'+W+'}type':'default','{'+R+'}id':rid});section.insert(0,reference)
        root=E.Element('{'+W+'}'+('hdr' if kind=='header' else 'ftr'));p=element(root,'p');pr=props(p)
        if kind=='header':
            element(element(p,'r'),'t').text='AIRK ARCHITECTURE  /  1.9.0 SOURCE'
            element(element(pr,'pBdr'),'bottom',val='single',sz=4,color='D7E2EB',space=4)
        else:
            element(pr,'jc',val='right');element(element(p,'r'),'t').text='Unpublished source  |  Page '
            for code,trailing in [('PAGE',' of '),('NUMPAGES','')]:
                fld=element(p,'fldSimple',instr=code);element(element(fld,'r'),'t').text='1'
                element(element(p,'r'),'t').text=trailing
        for run in p.findall('.//w:r',NS):
            rp=props(run,'rPr');element(rp,'sz',val=18);element(rp,'color',val='50647B')
        files['word/'+kind+'-architecture.xml']=xml(root)
    types=E.fromstring(files['[Content_Types].xml'])
    for kind in ['header','footer']:E.SubElement(types,'{'+CT+'}Override',{'PartName':'/word/'+kind+'-architecture.xml','ContentType':'application/vnd.openxmlformats-officedocument.wordprocessingml.'+kind+'+xml'})
    settings=E.fromstring(files['word/settings.xml']);element(settings,'updateFields',val='true')
    files.update({'word/document.xml':xml(document),'word/styles.xml':xml(styles),'word/_rels/document.xml.rels':xml(rels),'[Content_Types].xml':xml(types),'word/settings.xml':xml(settings)})
    with ZipFile(output,'w',ZIP_DEFLATED) as archive:
        for name,data in files.items():archive.writestr(name,data)
    receipt={'filename':output.name,'writer':'Pandoc + Python standard-library OOXML finishing','pandoc':subprocess.run([pandoc,'--version'],check=True,capture_output=True,text=True).stdout.splitlines()[0],'page':{'paper':'US Letter','orientation':'landscape','width_inches':11,'height_inches':8.5,'left_right_margin_inches':0.5,'top_bottom_margin_inches':0.45,'body_points':11},'figures':{'count':6,'pixel_dimensions':[3200,2100],'body_pixel_font':44,'width_inches':10,'height_inches':6.5625,'own_pages':True,'aspect_preserved':True},'docx_sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256((HERE/'airk-architecture.md').read_bytes()).hexdigest(),'png_sha256':[hashlib.sha256((HERE/row['png']).read_bytes()).hexdigest() for row in json.loads((HERE/'images/render-manifest.json').read_text())['images']]}
    (HERE/'word-layout-manifest.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(output);print(receipt['docx_sha256'])

if __name__=='__main__':main()
