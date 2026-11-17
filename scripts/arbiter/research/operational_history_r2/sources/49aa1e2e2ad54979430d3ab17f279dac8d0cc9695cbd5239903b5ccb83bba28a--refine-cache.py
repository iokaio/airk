# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f17-draft/display-updates/source/packages/vscode/src/workbenchRender.ts')
s=p.read_text(encoding='utf-8');s=s.replace('export class CurrentTurnProjections {','export const DISPLAY_PROJECTION_TURNS=200;\nexport class CurrentTurnProjections {')
s=s.replace('        const current=new Set(turns);','        const current=new Set(turns.slice(-DISPLAY_PROJECTION_TURNS));')
s=s.replace('        const cards:Record<string,EvidenceCard[]>={},pages:Record<string,Continuation[]>={};','        const cards:Array<[string,EvidenceCard[]]>=[],pages:Array<[string,Continuation[]]>=[];')
s=s.replace('                this.retained.set(turn,value);','                if(current.has(turn))this.retained.set(turn,value);')
s=s.replace('            cards[turn.id]=value.cards;pages[turn.id]=value.continuations;','            cards.push([turn.id,value.cards]);pages.push([turn.id,value.continuations]);')
s=s.replace('        return {cards,continuations:pages};','        return {cards:Object.fromEntries(cards),continuations:Object.fromEntries(pages)};')
p.write_text(s,encoding='utf-8',newline='\n')
