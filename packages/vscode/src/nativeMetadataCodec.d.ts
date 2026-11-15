// SPDX-License-Identifier: Apache-2.0
// Generated from packages/ui/src/nativeMetadataCodec.ts; source-sha256:83aac071d032cdd1bb3bee48e55bc1b35784dffee90a7891e2bb5edf835eada2
/** Single browser-safe lossless native metadata owner. Hashing is injected; raw native responses remain authoritative. */
export type Value = Record<string, unknown>;
export type CapabilityFieldDisplay = {
    state: 'verified';
    values: Record<string, Value>;
} | {
    state: 'unavailable';
    error: string;
};
export type ProjectionTurn = {
    id: string;
    envelope?: Value;
};
export declare const WORKBENCH_MAX_STEP_REPORTS = 72;
export declare const DISPLAY_PROJECTION_TURNS = 200;
export declare const NATIVE_HEADER_REFERENCE = "$airk_shared_native_header_fields";
export declare function readResultBody(raw: unknown): Value;
export declare const SHARED_CAPABILITY_REFERENCE = "$airk_shared_capability_fields";
export declare function shareableCapabilityField(key: string): boolean;
/** This merge is display-only and accepts only a separately host-verified table.
 * No reference grants read authority or replaces the complete original result. */
export declare function materializeCapabilityMember(raw: unknown, display?: CapabilityFieldDisplay): Value;
/** Exactly the prior native framed digest input; this function performs no cryptography. */
export declare function framedNativeMetadataBytes(domain: string, value: Value): Uint8Array;
export declare function createNativeMetadataCodec(sha256: (framedBytes: Uint8Array) => string): {
    framedNativeMetadataDigest: (domain: string, value: Value) => string;
    sharedCapabilityDigest: (fields: Value) => string;
    verifySharedCapabilityFields: (combined: Value) => CapabilityFieldDisplay | undefined;
    nativeHeaderTemplateDigest: (template: Value) => string;
    hasNativeMetadataReferences: (value: unknown) => boolean;
    materializeNativeMetadata: (raw: Value) => Value;
};
export type NativeMetadataCodec = ReturnType<typeof createNativeMetadataCodec>;
export declare class NativeMetadataProjectionCacheBase {
    private readonly codec;
    constructor(codec: NativeMetadataCodec);
    private views;
    remember(raw: Value, payload: Value): void;
    view(raw: Value): {
        payload?: Value;
        error?: string;
    };
    clear(): void;
}
export declare class SharedCapabilityDisplayCacheBase {
    private readonly codec;
    private readonly combinedReadDetails;
    constructor(codec: NativeMetadataCodec, combinedReadDetails: (value: Value) => Array<{
        step: string;
        result: Value;
    }>);
    private readonly retained;
    project(turns: ProjectionTurn[]): Record<string, Record<string, CapabilityFieldDisplay>>;
    clear(): void;
}
/** Async WebCrypto shell over the exact same synchronous validator, with no alternate acceptance logic. */
export declare function createBrowserNativeMetadataCodec(subtle?: Pick<SubtleCrypto, 'digest'>): {
    codec: {
        hasNativeMetadataReferences: (value: unknown) => boolean;
        materializeNativeMetadata: (raw: Value) => Value;
    };
    materializeNativeMetadata: (raw: Value, checkCurrent?: () => void) => Promise<Value>;
};
