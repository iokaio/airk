# SPDX-License-Identifier: Apache-2.0
"""Render the six explicit, print-sized SVG layouts with ImageMagick 7.

Python 3.11+ standard library only; no network, model, product build or live
state. Layout coordinates and body fonts are fixed for landscape Letter pages.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
from xml.sax.saxutils import escape

sys.dont_write_bytecode = True
HERE=Path(__file__).resolve().parent
INK='#172b46'; MUTED='#50647b'; BLUE='#2563a6'; TEAL='#087f82'; AMBER='#a66312'


class Diagram:
    def __init__(self,number,title,subtitle):
        self.parts=['<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1050" viewBox="0 0 1600 1050">','<rect width="1600" height="1050" fill="white"/>','<rect width="1600" height="10" fill="#172b46"/>']
        self.text(60,49,f'AIRK / ARCHITECTURE / {number:02}',20,TEAL,True)
        self.text(60,96,title,32,INK,True)
        self.text(60,136,subtitle,22,MUTED)
        self.parts.append('<path d="M60 1000 H1540" stroke="#d7e2eb"/>')
        self.text(60,1030,'1.9.0 unpublished source architecture | Component diagrams are not release or model qualification.',16,MUTED)
    def text(self,x,y,text,size=22,color=INK,bold=False,anchor='start'):
        self.parts.append(f'<text x="{x}" y="{y}" font-family="Segoe UI, DejaVu Sans, sans-serif" font-size="{size}" fill="{color}" font-weight="{600 if bold else 400}" text-anchor="{anchor}">{escape(text)}</text>')
    def box(self,x,y,w,h,title,lines=(),color=BLUE,fill='#eef5fc'):
        self.parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{fill}" stroke="{color}" stroke-width="1.7"/>')
        self.parts.append(f'<path d="M{x+18} {y+16} V{y+h-16}" stroke="{color}" stroke-width="4"/>')
        self.text(x+35,y+34,title,26,INK,True)
        for index,line in enumerate(lines):self.text(x+35,y+63+index*27,line)
    def group(self,x,y,w,h,title,fill='#f8fafc',color='#b8c9d8'):
        self.parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18" fill="{fill}" stroke="{color}"/>')
        self.text(x+25,y+35,title,22,MUTED,True)
    def arrow(self,points,label=None,at=None,both=False,dashed=False,color='#60758b'):
        path='M'+' L'.join(f'{x},{y}' for x,y in points)
        self.parts.append(f'<path d="{path}" fill="none" stroke="{color}" stroke-width="2.1"'+(' stroke-dasharray="7 6"' if dashed else '')+'/>')
        ends=[(points[-2],points[-1])]+([(points[1],points[0])] if both else [])
        for previous,tip in ends:
            dx,dy=tip[0]-previous[0],tip[1]-previous[1];length=math.hypot(dx,dy);ux,uy=dx/length,dy/length
            base=(tip[0]-ux*11,tip[1]-uy*11)
            vertices=[tip,(base[0]-uy*5,base[1]+ux*5),(base[0]+uy*5,base[1]-ux*5)]
            self.parts.append('<polygon points="'+' '.join(f'{x:g},{y:g}' for x,y in vertices)+f'" fill="{color}"/>')
        if label:
            width=len(label)*11+18
            self.parts.append(f'<rect x="{at[0]-width/2}" y="{at[1]-21}" width="{width}" height="30" fill="white"/>')
            self.text(*at,label,22,color,anchor='middle')
    def svg(self):return '\n'.join(self.parts+['</svg>'])+'\n'


def update_dependencies():
    root=HERE.parents[1];rows=[]
    for manifest in sorted((root/'crates').glob('*/Cargo.toml')):
        data=tomllib.loads(manifest.read_text(encoding='utf-8'));dependencies=set(data.get('dependencies',{}))
        for target in data.get('target',{}).values():dependencies.update(target.get('dependencies',{}))
        rows.append({'crate':data['package']['name'],'manifest':manifest.relative_to(root).as_posix(),'direct_airk_dependencies':sorted(dep for dep in dependencies if dep.startswith('airk-') or dep=='airkctl')})
    inventory={'scope':'union of normal optional and target-specific direct AIRK dependencies; no dev/build or Cargo feature resolution','crates':rows}
    (HERE/'native-dependencies.json').write_text(json.dumps(inventory,indent=2)+'\n',encoding='utf-8')
    lines=['| Crate | Direct production AIRK dependencies |','|---|---|']
    for row in rows:lines.append(f"| [{row['crate']}](../../{row['manifest']}) | "+(', '.join(f'`{name}`' for name in row['direct_airk_dependencies']) or 'None')+' |')
    document=HERE/'airk-architecture.md';left,tail=document.read_text(encoding='utf-8').split('<!-- native-dependencies:start -->');_,right=tail.split('<!-- native-dependencies:end -->')
    document.write_text(left+'<!-- native-dependencies:start -->\n'+'\n'.join(lines)+'\n<!-- native-dependencies:end -->'+right,encoding='utf-8')


def main():
    from diagram_layouts import figures
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--magick',default=shutil.which('magick'));args=parser.parse_args()
    if not args.magick:parser.error('ImageMagick 7 is required')
    update_dependencies();output=HERE/'images';sources=output/'sources';sources.mkdir(parents=True,exist_ok=True)
    version=subprocess.run([args.magick,'-version'],check=True,capture_output=True,text=True).stdout.splitlines()[0];entries=[]
    for name,diagram in figures(Diagram):
        svg=sources/(name+'.svg');png=output/(name+'.png');svg.write_text(diagram.svg(),encoding='utf-8',newline='\n')
        subprocess.run([args.magick,'-background','white','-density','192',str(svg),'-strip','-define','png:exclude-chunks=date,time',str(png)],check=True)
        dimensions=subprocess.run([args.magick,'identify','-format','%wx%h',str(png)],check=True,capture_output=True,text=True).stdout
        assert dimensions=='3200x2100',dimensions
        entries.append({'png':png.relative_to(HERE).as_posix(),'svg':svg.relative_to(HERE).as_posix(),'dimensions':dimensions,'png_sha256':hashlib.sha256(png.read_bytes()).hexdigest(),'svg_sha256':hashlib.sha256(svg.read_bytes()).hexdigest()});print(name,dimensions)
    manifest={'renderer':version,'font':'Segoe UI (fallback: DejaVu Sans, sans-serif)','body_font_pixels':44,'command':'python docs/architecture/render_diagrams.py','scale':'192dpi, 2x CSS pixels; fixed landscape layout','images':entries}
    (output/'render-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':main()
