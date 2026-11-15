// SPDX-License-Identifier: Apache-2.0
// Produce the Node runtime and its declarations from the single browser-safe metadata owner.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),ts=require('typescript'),esbuild=require('esbuild');
const root=path.resolve(__dirname,'..'),source=path.resolve(root,'../ui/src/nativeMetadataCodec.ts');
function outputs(){
 const sourceBytes=fs.readFileSync(source),seal=crypto.createHash('sha256').update(sourceBytes).digest('hex');
 const program=ts.createProgram([source],{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,strict:true,newLine:ts.NewLineKind.LineFeed,declaration:true,emitDeclarationOnly:true,noEmitOnError:true,types:[],lib:['lib.es2022.d.ts','lib.dom.d.ts']});
 const diagnostics=ts.getPreEmitDiagnostics(program);if(diagnostics.length)throw new Error(ts.formatDiagnosticsWithColorAndContext(diagnostics,{getCurrentDirectory:()=>root,getCanonicalFileName:file=>file,getNewLine:()=> '\n'}));
 let declaration;program.emit(undefined,(name,text)=>{if(name.endsWith('.d.ts')){if(declaration!==undefined)throw Error('Metadata owner emitted additional declarations');declaration=text;}});
 if(declaration===undefined)throw Error('Metadata owner emitted no declarations');
 const header=`// SPDX-License-Identifier: Apache-2.0\n// Generated from packages/ui/src/nativeMetadataCodec.ts; source-sha256:${seal}\n`;
 const built=esbuild.buildSync({absWorkingDir:root,entryPoints:[source],bundle:true,write:false,platform:'node',format:'cjs',target:'node20',metafile:true,banner:{js:header},legalComments:'none'});
 if(Object.keys(built.metafile.inputs).length!==1||built.outputFiles.length!==1||Object.values(built.metafile.outputs).some(output=>output.imports.length))throw Error('Pure metadata owner gained an external runtime dependency');
 return [[path.join(root,'src/nativeMetadataCodec.d.ts'),Buffer.from(header+declaration)],[path.join(root,'src/nativeMetadataCodec.js'),Buffer.from(built.outputFiles[0].contents)],[path.join(root,'out/nativeMetadataCodec.js'),Buffer.from(built.outputFiles[0].contents)]];
}
function build(check=false){for(const[file,bytes]of outputs()){if(check){if(!fs.existsSync(file)||!fs.readFileSync(file).equals(bytes))throw Error('Metadata owner runtime/declarations are stale: '+file);}else{fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,bytes);}}}
function verifyCaptured(bytes){if(bytes.length>4194304||!bytes.equals(outputs()[2][1]))throw Error('Captured metadata runtime differs from its exact source producer');}
function boundedStdin(){const parts=[];let size=0;while(true){const chunk=Buffer.alloc(Math.min(65536,4194305-size)),used=fs.readSync(0,chunk,0,chunk.length,null);if(!used)break;size+=used;if(size>4194304)throw Error('Captured metadata runtime exceeds bound');parts.push(chunk.subarray(0,used));}return Buffer.concat(parts);}
module.exports={outputs,verifyCaptured};if(require.main===module){if(process.argv.length===3&&process.argv[2]==='--verify-stdin'){const bytes=boundedStdin();verifyCaptured(bytes);process.stdout.write('verified\n');}else{if(process.argv.length>3||process.argv[2]&&process.argv[2]!=='--check')throw Error('Unknown metadata generation argument');build(process.argv[2]==='--check');}}
