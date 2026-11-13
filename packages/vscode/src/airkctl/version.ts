// SPDX-License-Identifier: Apache-2.0
//! The I2 version handshake (contracts §3.5, §3.6). The extension decides
//! compatibility from the I2 contract version and capabilities, never from
//! product-version ordering (plan §4.2). Semantic rules are hand-written and
//! tested against fixtures/split/i2/version.json and the §3.6 matrix rows.
import type { Installation, VersionDocument } from '../generated/i2';

export const VERSION_SCHEMA = 'airkctl-version/1';
export const PRODUCT = 'AIRK Service';
export const MAX_VERSION_DOCUMENT_BYTES = 64 * 1024;

/** What this extension build speaks and needs. */
export interface ExtensionRequirements {
    readonly supported: readonly number[];
    readonly required: readonly string[];
    /** Features that degrade with a reason when their capability is absent. */
    readonly optional: Readonly<Record<string, string>>;
}
export const EXTENSION_REQUIREMENTS: ExtensionRequirements = Object.freeze({
    supported: [1],
    required: ['session/2', 'tools/1'],
    optional: Object.freeze({
        'events/1': 'Live status and job updates',
        'window-scope/1': 'Choosing this window\'s search scope',
        'assistance/1': 'Explorer Assistance drafts',
        'handoff/1': 'Explorer handoffs',
        'agents/1': 'Agent connection management',
        'agents-entry-remove/1': 'Removing reviewed agent configuration entries',
        'admin-launch/1': 'Open AIRK Admin',
    }),
});

export class VersionDocumentInvalid extends Error {
    constructor(readonly code: string) { super(code); this.name = 'VersionDocumentInvalid'; }
}

const INSTALLATION_STATES = ['complete', 'incomplete', 'not_found', 'unverified'];
const strings = (value: unknown) => Array.isArray(value) && value.every(item => typeof item === 'string');
const u32s = (value: unknown) => Array.isArray(value) && value.every(item => Number.isSafeInteger(item) && item >= 0 && item <= 0xffff_ffff);
const optionalString = (value: unknown) => value === undefined || value === null || typeof value === 'string';

/** Parse `airkctl version --output json`. Unknown fields are ignored (forward
 * compatibility); a malformed or oversized document is refused. */
export function parseVersionDocument(stdout: string): VersionDocument {
    if (Buffer.byteLength(stdout, 'utf8') > MAX_VERSION_DOCUMENT_BYTES) { throw new VersionDocumentInvalid('version_document_oversize'); }
    let value: unknown;
    try { value = JSON.parse(stdout); } catch { throw new VersionDocumentInvalid('version_document_malformed'); }
    if (!value || typeof value !== 'object' || Array.isArray(value)) { throw new VersionDocumentInvalid('version_document_malformed'); }
    const doc = value as Record<string, unknown>;
    const installation = doc.installation as Record<string, unknown> | undefined;
    const valid = doc.schema === VERSION_SCHEMA && typeof doc.product === 'string' && typeof doc.version === 'string'
        && optionalString(doc.channel) && Number.isSafeInteger(doc.i2_contract) && u32s(doc.i2_contracts_supported)
        && Number.isSafeInteger(doc.i1_protocol) && strings(doc.capabilities)
        && (doc.executable_sha256 === undefined || doc.executable_sha256 === null || typeof doc.executable_sha256 === 'string' && /^[0-9a-f]{64}$/.test(doc.executable_sha256))
        && !!installation && typeof installation === 'object' && !Array.isArray(installation)
        && INSTALLATION_STATES.includes(installation.state as string)
        && optionalString(installation.airk_version) && optionalString(installation.install_id)
        && (installation.pair === undefined || installation.pair === null || installation.pair === 'exact' || installation.pair === 'mismatch');
    if (!valid) { throw new VersionDocumentInvalid(doc.schema !== VERSION_SCHEMA ? 'version_document_unknown_schema' : 'version_document_malformed'); }
    return doc as unknown as VersionDocument;
}

export type Compatibility =
    | {
        readonly compatible: true; readonly contract: number;
        /** Optional features turned off, with the reason shown to the user. */
        readonly disabled: readonly { readonly capability: string; readonly feature: string }[];
    }
    | {
        readonly compatible: false;
        readonly reason: 'unsupported_contract' | 'missing_capability' | 'installation_incomplete' | 'pair_mismatch' | 'installation_unverified' | 'not_installed';
        readonly missing?: readonly string[];
        readonly extensionContracts: readonly number[];
        readonly airkctl: { readonly version: string; readonly contracts: readonly number[] };
    };

/** Decide compatibility from the contract version, the pair state and
 * capabilities (contracts §3.6). Unknown capability strings are ignored. */
export function compatibility(doc: VersionDocument, requirements: ExtensionRequirements = EXTENSION_REQUIREMENTS): Compatibility {
    const refuse = (reason: Extract<Compatibility, { compatible: false }>['reason'], missing?: string[]): Compatibility => ({
        compatible: false, reason, ...(missing ? { missing } : {}), extensionContracts: [...requirements.supported],
        airkctl: { version: doc.version, contracts: [...new Set([doc.i2_contract, ...doc.i2_contracts_supported])] },
    });
    const installation: Installation = doc.installation;
    if (installation.state === 'not_found') { return refuse('not_installed'); }
    if (installation.state === 'unverified') { return refuse('installation_unverified'); }
    if (installation.state === 'incomplete') { return refuse('installation_incomplete'); }
    if (installation.pair === 'mismatch') { return refuse('pair_mismatch'); }
    const offered = new Set([doc.i2_contract, ...doc.i2_contracts_supported]);
    const contract = [...requirements.supported].filter(version => offered.has(version)).sort((a, b) => b - a)[0];
    if (contract === undefined) { return refuse('unsupported_contract'); }
    const capabilities = new Set(doc.capabilities);
    const missing = requirements.required.filter(capability => !capabilities.has(capability));
    if (missing.length) { return refuse('missing_capability', missing); }
    const disabled = Object.entries(requirements.optional).filter(([capability]) => !capabilities.has(capability))
        .map(([capability, feature]) => ({ capability, feature }));
    return { compatible: true, contract, disabled };
}
