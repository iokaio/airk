// SPDX-License-Identifier: Apache-2.0
import {browserHarness} from '../../packages/ui/test/browserHarness.mjs';
const started=performance.now();
const browser=await browserHarness();
try { console.log(JSON.stringify({stage:'actual_browser_started',observation:browser.observation,value:await browser.evaluate('1+1')})); }
finally { await browser.close(); console.log(JSON.stringify({stage:'actual_browser_closed',observation:browser.observation,elapsed_ms:performance.now()-started})); }
