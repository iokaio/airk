// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';

export const maximumPolicyBytes = 131072;
export async function withPolicySnapshot<T>(file: string, action: (snapshot: string) => Promise<T>, label = 'Extraction policy', maximumBytes = maximumPolicyBytes): Promise<T> {
    if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 1 || maximumBytes > maximumPolicyBytes) { throw new Error('Invalid JSON input bound.'); }
    if (!(await fs.lstat(file)).isFile()) { throw new Error(`${label} must be a regular unlinked file.`); }
    const input = await fs.open(file, 'r');
    let bytes: Buffer;
    try {
        const metadata = await input.stat();
        if (!metadata.isFile() || metadata.size > maximumBytes) { throw new Error(`${label} must be a regular file of at most ${maximumBytes / 1024} KiB.`); }
        const buffer = Buffer.alloc(maximumBytes + 1);
        let length = 0;
        while (length < buffer.length) {
            const read = await input.read(buffer, length, buffer.length - length, null);
            if (!read.bytesRead) { break; }
            length += read.bytesRead;
        }
        if (length > maximumBytes) { throw new Error(`${label} exceeds ${maximumBytes / 1024} KiB.`); }
        bytes = buffer.subarray(0, length);
        JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    } finally { await input.close(); }
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-extraction-policy-'));
    const snapshot = path.join(directory, 'policy.json');
    try {
        await fs.writeFile(snapshot, bytes, { flag: 'wx', mode: 0o600 });
        return await action(snapshot);
    } finally {
        await fs.rm(snapshot, { force: true });
        await fs.rmdir(directory);
    }
}
