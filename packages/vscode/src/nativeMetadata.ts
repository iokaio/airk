// SPDX-License-Identifier: Apache-2.0
/** Node API compatibility facade; all validation/materialization lives in the shared pure owner. */
import {NativeMetadataProjectionCacheBase} from './nativeMetadataCodec';
import {nativeMetadataOwner} from './nativeMetadataOwner';
export {NATIVE_HEADER_REFERENCE} from './nativeMetadataCodec';
export const nativeHeaderTemplateDigest=nativeMetadataOwner.nativeHeaderTemplateDigest;
export const hasNativeMetadataReferences=nativeMetadataOwner.hasNativeMetadataReferences;
export const materializeNativeMetadata=nativeMetadataOwner.materializeNativeMetadata;
export class NativeMetadataProjectionCache extends NativeMetadataProjectionCacheBase {
    constructor(){super(nativeMetadataOwner);}
}
