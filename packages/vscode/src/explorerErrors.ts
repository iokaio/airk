// SPDX-License-Identifier: Apache-2.0
// The single Explorer error registry. Each code names the plane that emits it.
// Native ServiceError codes from the control plane pass through unchanged and are
// not listed here.

export type ErrorPlane = 'assistance' | 'host' | 'service' | 'webview';

export const EXPLORER_ERRORS = {
    // Explorer Assistance MCP server (drafts and binding).
    UI_NOT_ATTACHED: { plane: 'assistance', message: 'No trusted AIRK Explorer window is attached to receive a draft.' },
    HOST_UNAVAILABLE: { plane: 'assistance', message: 'The VS Code extension host is not reachable.' },
    WORKSPACE_UNTRUSTED: { plane: 'assistance', message: 'The workspace is not trusted.' },
    AMBIGUOUS_TARGET: { plane: 'assistance', message: 'More than one Explorer window matches; select a workspace explicitly.' },
    STALE_BINDING: { plane: 'assistance', message: 'The window, workspace or project binding changed since the request was prepared.' },
    SCHEMA_UNSUPPORTED: { plane: 'assistance', message: 'The request schema version is not supported by this extension.' },
    INVALID_SCHEMA: { plane: 'assistance', message: 'The request has unknown fields, invalid values or exceeds a limit.' },
    CAPABILITY_DISABLED: { plane: 'assistance', message: 'The requested view, template or mode is disabled or unavailable.' },
    EVIDENCE_MISMATCH: { plane: 'assistance', message: 'A cited publication or source hash does not match the current index.' },
    PROPOSAL_CONFLICT: { plane: 'assistance', message: 'The target draft changed, or the idempotency key was reused with a different body.' },
    PROPOSAL_EXPIRED: { plane: 'assistance', message: 'The proposal expired; prepare a new one.' },
    RATE_LIMITED: { plane: 'assistance', message: 'Too many proposals; retry later.' },
    POLICY_DENIED: { plane: 'assistance', message: 'The request would execute, mutate storage or reach outside the workspace.' },
    REJECTED_BY_USER: { plane: 'assistance', message: 'The user rejected the proposal.' },
    // Extension host.
    SERVICE_UNAVAILABLE: { plane: 'host', message: 'The AIRK service is not connected.' },
    SERVICE_INCOMPATIBLE: { plane: 'host', message: 'The AIRK runtime version does not match this extension.' },
    EMBEDDING_UNAVAILABLE: { plane: 'host', message: 'Local vector search is unavailable for this project.' },
    NO_PROJECT: { plane: 'host', message: 'No registered AIRK project is selected.' },
    INDEX_UNAVAILABLE: { plane: 'host', message: 'The selected project has no readable publication yet.' },
    REQUEST_SUPERSEDED: { plane: 'host', message: 'A newer request replaced this one.' },
    REQUEST_TIMEOUT: { plane: 'host', message: 'AIRK did not answer before the Explorer deadline.' },
    TOOL_ERROR: { plane: 'service', message: 'The AIRK tool reported an error.' },
    UNKNOWN_REFERENCE: { plane: 'host', message: 'The referenced result is no longer available; run the query again.' },
    // Webview message validation.
    INVALID_MESSAGE: { plane: 'webview', message: 'The Explorer message was not recognized.' },
} as const satisfies Record<string, { plane: ErrorPlane; message: string }>;

export type ExplorerErrorCode = keyof typeof EXPLORER_ERRORS;

export interface ExplorerError { readonly code: ExplorerErrorCode | string; readonly message: string }

export function isExplorerErrorCode(value: unknown): value is ExplorerErrorCode {
    return typeof value === 'string' && Object.hasOwn(EXPLORER_ERRORS, value);
}

/** A registry error, or a pass-through native service code with a bounded message. */
export function explorerError(code: ExplorerErrorCode | string, detail?: string): ExplorerError {
    const base = isExplorerErrorCode(code) ? EXPLORER_ERRORS[code].message : 'AIRK reported an error.';
    const safeCode = isExplorerErrorCode(code) || /^[a-z0-9_]{1,128}$/.test(code) ? code : 'TOOL_ERROR';
    return { code: safeCode, message: detail ? `${base} ${detail.slice(0, 600)}` : base };
}
