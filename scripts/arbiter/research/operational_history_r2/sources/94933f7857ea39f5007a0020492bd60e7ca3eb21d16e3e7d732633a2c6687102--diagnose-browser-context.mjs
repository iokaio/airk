// SPDX-License-Identifier: Apache-2.0
// Minimal owned blank-page CDP probe; no product, source corpus or credentials.
import { browserHarness } from '../../packages/ui/test/browserHarness.mjs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';
const destination = resolve(process.argv[2]);
const receiptRoot = resolve('artifacts/advanced-split/receipts') + sep;
if (!destination.startsWith(receiptRoot)) throw new Error('Receipt outside campaign');
const result = { started_utc: new Date().toISOString(), scope: 'Blank-page environment diagnostic only; no product DOM',
  identity_sid: execFileSync('whoami', ['/user'], { encoding: 'utf8' }).match(/S-1-[0-9-]+/)?.[0],
  harness_sha256: createHash('sha256').update(await readFile('packages/ui/test/browserHarness.mjs')).digest('hex') };
let browser;
try {
  browser = await browserHarness();
  result.page = await browser.evaluate('({bodyReady:!!document.body,url:document.URL})');
} catch (error) {
  result.error = String(error);
} finally {
  try { await browser?.close(); } catch (error) { result.close_error = String(error); }
  result.ownership = browser?.observation;
  result.finished_utc = new Date().toISOString();
  await writeFile(destination, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
}
console.log(JSON.stringify(result));
process.exitCode = result.error || result.close_error || result.page?.bodyReady !== true ? 1 : 0;
