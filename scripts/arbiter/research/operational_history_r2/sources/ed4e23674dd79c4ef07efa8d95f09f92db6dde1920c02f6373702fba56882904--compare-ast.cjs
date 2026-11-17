// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs'),crypto=require('node:crypto'),path=require('node:path');
const ts=require(path.resolve('packages/vscode/node_modules/typescript'));
const base='artifacts/advanced-split/receipts/metadata-owner-extraction';
const paths=['nativeMetadata.ts','sharedCapabilityFields.ts','workbenchRender.ts','sourceVerification.ts'];
const parse=s=>ts.createSourceFile('input.ts',s,ts.ScriptTarget.Latest,true);
function named(s){let map=new Map();function walk(n){if(ts.isFunctionDeclaration(n)&&n.name?.text==='createBrowserNativeMetadataCodec')return;if((ts.isFunctionDeclaration(n)||ts.isMethodDeclaration(n))&&n.name&&n.body){const prefix=ts.isMethodDeclaration(n)?n.parent.name?.text.replace(/Base$/,'')+'.':'';map.set(prefix+n.name.getText(),n);}ts.forEachChild(n,walk);}walk(parse(s));return map;}
const current=fs.readFileSync('packages/ui/src/nativeMetadataCodec.ts','utf8'),now=named(current);
function normal(n){let body=n.body.getText().replace(/this\.codec\./g,'').replace(/this\.combinedReadDetails/g,'combinedReadDetails');
 const f=parse('function x()'+body);
 const transformed=ts.transform(f,[ctx=>{function visit(n){if(ts.isCallExpression(n)&&n.expression.getText(f)==='Buffer.byteLength'&&n.arguments.length===2&&n.arguments[1].text==='utf8')return ts.factory.createPropertyAccessExpression(ts.factory.createCallExpression(ts.factory.createIdentifier('utf8'),undefined,[ts.visitNode(n.arguments[0],visit)]),'length');return ts.visitEachChild(n,visit,ctx);}return root=>ts.visitNode(root,visit);}]).transformed[0];
 const printed=ts.createPrinter({removeComments:true}).printFile(transformed),js=ts.transpileModule(printed,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 const scan=ts.createScanner(ts.ScriptTarget.Latest,true,ts.LanguageVariant.Standard,js),tokens=[];for(let t=scan.scan();t!==ts.SyntaxKind.EndOfFileToken;t=scan.scan())tokens.push([t,scan.getTokenText()]);return JSON.stringify(tokens);
}
const exclude=new Set(['framedNativeMetadataDigest','sharedCapabilityDigest','nativeHeaderTemplateDigest']);const rows=[];
for(const name of paths){for(const [key,node]of named(fs.readFileSync(base+'/'+name,'utf8'))){if(!now.has(key)||exclude.has(key))continue;const a=normal(node),b=normal(now.get(key));rows.push({source:name,name:key,equal:a===b,before_sha256:crypto.createHash('sha256').update(a).digest('hex'),after_sha256:crypto.createHash('sha256').update(b).digest('hex')});}}
const report={schema:'airk.private-metadata-structural-review/1',allowances:['erase TypeScript types and trivia via TypeScript compiler','Buffer.byteLength(value, utf8) => utf8(value).length','injected this.codec method prefix and combinedReadDetails callback'],excluded_digest_primitives:'Exact independent framed-byte construction and both native Rust goldens tested separately; injected hash is intentional change.',new_behavior:'Browser async digest, exact raw snapshot custody, retained original scope guard, bounded same-object proof reuse are new and separately tested.',source_sha256:crypto.createHash('sha256').update(current).digest('hex'),rows};
fs.writeFileSync(base+'/ast-receipt.json',JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({compared:rows.length,failed:rows.filter(r=>!r.equal)}));if(!rows.length||rows.some(r=>!r.equal))process.exitCode=1;
