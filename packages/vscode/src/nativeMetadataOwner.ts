// SPDX-License-Identifier: Apache-2.0
// The Node host injects its standard SHA implementation into the single pure owner.
import {createHash} from 'node:crypto';
import {createNativeMetadataCodec} from './nativeMetadataCodec';
export const nativeMetadataOwner=createNativeMetadataCodec(bytes=>createHash('sha256').update(bytes).digest('hex'));
