// SPDX-License-Identifier: Apache-2.0
const {buildSync}=require('../../../packages/vscode/node_modules/esbuild');
const fs=require('fs');
const dir='artifacts/local/1.8.10-indexing-20261107';
buildSync({stdin:{contents:`import {render} from 'preact'; import {useState} from 'preact/hooks'; import {IndexingScreen} from './src/explorerWebview/indexing';
function Fixture(){const [tab,setTab]=useState('library');const [state,setState]=useState(window.fixture);window.updateFixture=setState;return <main style="padding:28px;max-width:1160px;margin:auto"><h1>Indexing</h1><div class="tabs">{['window','library','projects'].map(t=><button class={tab===t?'selected':''} onClick={()=>setTab(t)}>{t==='window'?'This window':t==='library'?'Index library':'Projects'}</button>)}</div><IndexingScreen state={state} route={{workspace:'indexing',tab}} go={r=>setTab(r.tab)} publications={[]} inspect={()=>{}} runRecipe={()=>{}}/></main>}; render(<Fixture/>,document.getElementById('root'));`,resolveDir:'packages/vscode',loader:'tsx'},bundle:true,write:true,outfile:dir+'/preview.js',platform:'browser',jsx:'automatic',jsxImportSource:'preact'});
