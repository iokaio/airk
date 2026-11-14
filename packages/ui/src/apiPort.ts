// SPDX-License-Identifier: Apache-2.0
// Presentation transport only. Native payloads retain the generated I3 envelope.
import type { ParsedResponse } from '../../admin/src/envelope';
import type { Operation } from '../../vscode/src/adminUiContract';
import type { MutationPreparation } from '../../contracts-gen/i3';

export type ApiResult = ParsedResponse | { kind: 'no_session' };
export type RetainedPreparation = { operation: Operation; body: unknown; acknowledgment: ApiResult; preparation: MutationPreparation };
/** A captured native UI owner. No cookie, CSRF, native caller/key or executable. */
export interface ApiPort {
  readonly kind: 'native';
  readonly serviceInstance: string;
  isCurrent(): boolean;
  readonly preparations?: readonly RetainedPreparation[];
  call(operation: Operation, body: unknown): Promise<ApiResult>;
  close(): Promise<void>;
}
