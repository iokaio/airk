// SPDX-License-Identifier: Apache-2.0
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import {ca} from './httpTlsFixture';

/** Historical disposal metadata deliberately contains no live selection/grant. */
export function predecessorHttpFixture(control:string,predecessor:string,id='29'.repeat(16),ownership:'active'|'expired'|'revoked'='expired'){
    const endpoint={schema:1,installation:'aa'.repeat(32),user:'fixture',namespace:'34'.repeat(16),generation:1,service_instance:'12'.repeat(16),executable_sha256:'bb'.repeat(32),url:'https://127.0.0.1:57421/mcp',ca_sha256:'cc'.repeat(32),certificate_sha256:'dd'.repeat(32),state:'stopping',predecessor_digest:null as string|null};
    const credential_sha256=createHash('sha256').update(`airk1.${id}.`+'a'.repeat(64)).digest('hex'),ca_certificate_file_sha256=createHash('sha256').update(ca).digest('hex');
    const snapshot={schema:1,purpose:'predecessor_cleanup',predecessor,control_dir:control,namespace:endpoint.namespace,endpoint,ca_certificate_file_sha256,complete:true,owners:[{connection_id:id,ownership,credential_sha256}],failure:null};
    return {schema:1,purpose:'predecessor_cleanup',active:predecessor,predecessor,operation_id:null as string|null,control_dir:control,namespace:endpoint.namespace,connection_id:id,status:'owned',ownership,endpoint,ca_certificate_file:path.join(control,'http-1','ca.pem'),credential_file:path.join(control,'http-1',`credential-${id}.token`),credential_sha256,ca_certificate_file_sha256,qualification:{live_authority:false,source:'current_predecessor_metadata',snapshot_complete:true,credential_digest_algorithm:'sha256_full_airk1_token',current_namespace:endpoint.namespace,current_namespace_ownership:'same_predecessor',current_namespace_failure:null},snapshot};
}
