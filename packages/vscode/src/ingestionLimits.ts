// SPDX-License-Identifier: Apache-2.0
export interface ScanLimits {
    file_bytes: number; corpus_bytes: number; extracted_bytes: number;
    files: number; discovery_entries: number; read_bytes: number;
}
export interface LimitsDescription {
    mode: string; detected_memory_bytes: number | null;
    automatic: ScanLimits; effective: ScanLimits; maximum: ScanLimits;
}
type LimitKey = keyof ScanLimits;
export const largerLimitsExplanation = 'Larger limits can increase indexing time, memory use and disk usage. Disk quotas and per-file extraction limits still apply; the previous index is retained if indexing fails.';

export function aboveNormal(proposed: ScanLimits, automatic: ScanLimits): boolean {
    return (Object.keys(automatic) as LimitKey[]).some(key => proposed[key] > automatic[key]);
}

function exhaustedLimit(error: unknown): { key: LimitKey; label: string; unit: number } | undefined {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes('extracted corpus exceeds configured byte limit')) { return { key: 'extracted_bytes', label: 'extracted text per collection', unit: 1048576 }; }
    if (message.includes('corpus exceeds configured byte limit')) { return { key: 'corpus_bytes', label: 'eligible source per collection', unit: 1048576 }; }
    if (message.includes('corpus exceeds configured file count')) { return { key: 'files', label: 'indexed files', unit: 1 }; }
    if (message.includes('workspace discovery entry budget exhausted') || message.includes('previous-path reconciliation budget exhausted')) { return { key: 'discovery_entries', label: 'discovery entries', unit: 1 }; }
    if (message.includes('workspace raw source read budget exhausted')) { return { key: 'read_bytes', label: 'raw reads per pass', unit: 1048576 }; }
    return undefined;
}

export interface LimitApprovalPort {
    describe(): Promise<LimitsDescription>;
    review(message: string): Promise<boolean>;
    input(prompt: string, value: string, validate: (value: string) => string | undefined): Promise<string | undefined>;
    confirm(message: string): Promise<boolean>;
    save(proposed: ScanLimits, expected: ScanLimits): Promise<void>;
}

/** Approval changes one budget only, compares the reviewed policy atomically,
 * and never bypasses native validation, format bounds or storage admission. */
export async function authorizeLimitIncrease(port: LimitApprovalPort, collectionName: string, error: unknown): Promise<boolean> {
    const exhausted = exhaustedLimit(error);
    if (!exhausted) { return false; }
    const { key, label, unit } = exhausted;
    const { effective, automatic, maximum } = await port.describe();
    const current = effective[key], ceiling = maximum[key];
    if (!Number.isSafeInteger(current) || current < 1 || !Number.isSafeInteger(ceiling) || current >= ceiling) { return false; }
    const suffix = unit === 1 ? '' : ' MiB';
    const format = (bytes: number) => `${bytes / unit}${suffix}`;
    if (!await port.review(`${collectionName} reached its ${label} limit (${format(current)}). Review a larger saved limit and retry indexing?`)) { return false; }
    const validate = (value: string): string | undefined => {
        const amount = Number(value) * unit;
        return Number.isSafeInteger(amount) && amount > current && amount <= ceiling
            ? undefined : `Enter more than ${current / unit} and at most ${ceiling / unit}${suffix}.`;
    };
    const value = await port.input(`New ${label} limit${unit === 1 ? '' : ' in MiB'}; normal ${format(automatic[key])}, maximum ${format(ceiling)}.`,
        String(Math.min(Math.ceil(current * 2 / unit) * unit, ceiling) / unit), validate);
    if (value === undefined) { return false; }
    const invalid = validate(value); if (invalid) { throw new Error(invalid); }
    const amount = Number(value) * unit;
    const proposed = { ...effective, [key]: amount };
    const explanation = amount > automatic[key] ? 'This exceeds the normal limit for this machine. ' : '';
    if (!await port.confirm(`Save ${format(amount)} for ${label} in ${collectionName} and retry once? ${explanation}${largerLimitsExplanation}`)) { return false; }
    await port.save(proposed, effective);
    return true;
}

/** A second failure propagates: no repeated prompts or unlimited retry loop. */
export async function indexWithLimitApproval<T>(run: () => Promise<T>, approve: (error: unknown) => Promise<boolean>): Promise<T> {
    try { return await run(); }
    catch (error) {
        if (!await approve(error)) { throw error; }
        return run();
    }
}
