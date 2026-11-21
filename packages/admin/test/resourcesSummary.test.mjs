// SPDX-License-Identifier: Apache-2.0
// UI projection on explicitly synthetic native ResourceSnapshot fields; no hardware qualification.
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const built=await build({absWorkingDir:root,stdin:{contents:"export {resourceSummaryOf,gpuAllowedOf,gpuResetPendingOf} from './src/pages';",resolveDir:root,sourcefile:'resources-summary-test.ts'},bundle:true,write:false,format:'esm',platform:'node',jsx:'automatic',jsxImportSource:'preact'});
const {resourceSummaryOf,gpuAllowedOf,gpuResetPendingOf}=await import(`data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text).toString('base64')}`);
const gib=1024**3;
test('resource summary keeps observed memory separate from policy reservation and query capacity',()=>{
 const rows=Object.fromEntries(resourceSummaryOf({status:'resources',observation:{configuration:{query_concurrency:16},logical_cpus:8,resident_workers:1,resident_limit:4,observed_rss_bytes:2*gib,available_memory_bytes:8*gib,detected_memory_bytes:16*gib,memory_reserved_bytes:3*gib,memory_limit_bytes:6*gib,query_headroom_bytes:gib,running_jobs:2,waiting_projects:3,telemetry_complete:true,pressure:false,provider_drain_error:false}}));
 assert.equal(rows['Concurrent evidence requests'],'16');assert.equal(rows['CPU and workers'],'8 logical CPUs; 1 of 4 resident worker slots');
 assert.equal(rows['Service memory in use'],'2.0 GiB');assert.equal(rows['System memory available'],'8.0 GiB available; 16.0 GiB detected total');
 assert.equal(rows['Configured memory reservation'],'3.0 GiB reserved of 6.0 GiB; 1.0 GiB query headroom');
 assert.equal(rows.Resources,'Current resource observations available');
});
test('missing, invalid and unsafe measurements remain unavailable rather than guessed or zero-filled',()=>{
 assert.deepEqual(resourceSummaryOf({status:'health'}),[]);
 const rows=Object.fromEntries(resourceSummaryOf({status:'resources',observation:{configuration:{query_concurrency:4.5},logical_cpus:-1,observed_rss_bytes:Number.MAX_SAFE_INTEGER+1}}));
 assert.equal(rows['Concurrent evidence requests'],'Unavailable');assert.equal(rows['Service memory in use'],'Unavailable');assert.match(rows['CPU and workers'],/^Unavailable logical CPUs/);assert.equal(rows.Resources,'Resource observations incomplete or unavailable');
});
test('GPU permission requires the persisted native value from a successful resources read',()=>{
 const result={status:'resources',configuration_error:false,observation:{configuration:{allow_gpu:false}}};
 assert.equal(gpuAllowedOf(result),false);
 assert.equal(gpuAllowedOf({...result,observation:{configuration:{allow_gpu:true}}}),true);
 assert.equal(gpuAllowedOf({...result,configuration_error:true}),null);
 assert.equal(gpuAllowedOf({...result,observation:{configuration:{}}}),null);
 assert.equal(gpuAllowedOf({...result,status:'refused'}),null);
});
test('the GPU reset notice needs the pending record on a current resources read with GPU use off',()=>{
 const result={status:'resources',configuration_error:false,observation:{configuration:{allow_gpu:false,gpu_opt_in:'default_reset'}}};
 assert.equal(gpuResetPendingOf(result),true);
 assert.equal(gpuAllowedOf(result),false,'the reset state still reads as off');
 assert.equal(gpuResetPendingOf({...result,observation:{configuration:{allow_gpu:false}}}),false);
 assert.equal(gpuResetPendingOf({...result,observation:{configuration:{allow_gpu:true,gpu_opt_in:'user_choice'}}}),false);
 assert.equal(gpuResetPendingOf({...result,observation:{configuration:{allow_gpu:true,gpu_opt_in:'default_reset'}}}),false);
 assert.equal(gpuResetPendingOf({...result,observation:{configuration:{allow_gpu:false,gpu_opt_in:'future_value'}}}),false);
 assert.equal(gpuResetPendingOf({...result,observation:{configuration:{gpu_opt_in:'default_reset'}}}),false);
 assert.equal(gpuResetPendingOf({...result,configuration_error:true}),false);
 assert.equal(gpuResetPendingOf({...result,status:'refused'}),false);
 assert.equal(gpuResetPendingOf(undefined),false);
});
test('provider failures and memory pressure remain explicit even with complete telemetry',()=>{
 const snapshot={telemetry_complete:true,pressure:true,provider_drain_error:false};
 assert.equal(Object.fromEntries(resourceSummaryOf({status:'resources',observation:snapshot})).Resources,'Memory pressure: background work may wait');
 snapshot.provider_drain_error=true;assert.equal(Object.fromEntries(resourceSummaryOf({status:'resources',observation:snapshot})).Resources,'Provider recovery needs attention');
});
