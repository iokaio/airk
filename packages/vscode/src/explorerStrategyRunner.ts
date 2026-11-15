// SPDX-License-Identifier: Apache-2.0
// Node host adapter for the single browser-safe bounded strategy runner.
import {createHash} from 'node:crypto';
import {runStrategy as runShared, type RunDependencies as SharedDependencies, type RunRequest} from './explorerStrategyRunnerCore';
export {checkRequest, type RunRequest} from './explorerStrategyRunnerCore';
export type RunDependencies=Omit<SharedDependencies,'sha256'>;
export const runStrategy=(request:RunRequest,deps:RunDependencies)=>runShared(request,{...deps,sha256:value=>createHash('sha256').update(value).digest('hex')});
