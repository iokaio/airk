// SPDX-License-Identifier: Apache-2.0
/** Node compatibility facade over the one shared lossless metadata owner. */
import {SharedCapabilityDisplayCacheBase} from './nativeMetadataCodec';
import {combinedReadDetails} from './workbenchRender';
import {nativeMetadataOwner} from './nativeMetadataOwner';
export const sharedCapabilityDigest=nativeMetadataOwner.sharedCapabilityDigest;
export const framedNativeMetadataDigest=nativeMetadataOwner.framedNativeMetadataDigest;
export const verifySharedCapabilityFields=nativeMetadataOwner.verifySharedCapabilityFields;
export class SharedCapabilityDisplayCache extends SharedCapabilityDisplayCacheBase {
    constructor(){super(nativeMetadataOwner,combinedReadDetails);}
}
