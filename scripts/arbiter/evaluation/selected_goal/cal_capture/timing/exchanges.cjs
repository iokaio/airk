// SPDX-License-Identifier: Apache-2.0
'use strict';
const crypto = require('node:crypto');
const {isDeepStrictEqual: equal} = require('node:util');

const ARMS = [['full','local4','local4','full'], ['local4','full','full','local4']];
function need(ok, reason) { if (!ok) throw Error(reason); }
function hash(text) { return crypto.createHash('sha256').update(text).digest('hex'); }
function stamp() { return {unix_ms:Date.now(), monotonic_ns:process.hrtime.bigint().toString()}; }

function timing(plan, pass, pid, ids, serviceInstance, cliPid, emit, apply, clock=stamp) {
    need(plan.schema === 'airk.private.cal-phase12-affinity-timing/1' && Number.isInteger(pass) && ARMS[pass] &&
        Number.isSafeInteger(pid) && pid > 0 && ids.length === 4 && new Set(ids).size === 4 &&
        equal(plan.question_ids,ids), 'foreign original timing plan or questions');
    let next = 0, question, grant, active = false, calls = 0;
    function begin(id) {
        need(question === undefined && ids[next] === id, 'timing question reused or out of order');
        const ordinal = 96 + next, arm = ARMS[pass][next];
        const result = apply(ordinal,arm);
        need(result.success === true && result.pid === pid && result.pass_index === pass &&
            result.ordinal === ordinal && result.arm === arm &&
            /^0x[0-9a-f]+$/i.test(result.full_mask_hex) && /^0x[0-9a-f]+$/i.test(result.observed_mask_hex) &&
            (arm === 'full' ? BigInt(result.full_mask_hex) === BigInt(result.observed_mask_hex) :
             BigInt(result.observed_mask_hex) > 0n && (BigInt(result.full_mask_hex) & BigInt(result.observed_mask_hex)) ===
             BigInt(result.observed_mask_hex) && BigInt(result.observed_mask_hex).toString(2).replace(/0/g,'').length === 4),
             'native affinity attestation differs');
        question = {question_id:id,ordinal,arm,pass_index:pass,service_instance:serviceInstance,service_pid:pid};
        calls = 0;
        emit({kind:'timing_assignment',...question,attestation:result});
        ++next;
    }
    async function invoke(session,argv,options) {
        need(question && grant && !active && options.deadlineMs === 125000 &&
            argv[0] === 'tools' && argv[1] === 'call', 'ungranted, concurrent or unbounded measuring invocation');
        active = true;
        const base = {...question,scope:grant,exchange:++calls,tool:argv[2],argv_sha256:hash(JSON.stringify(argv))};
        const start = clock();
        let end,result,error;
        try {
            result = await session.invoke(argv,options);
            end = clock();
            return result;
        } catch(e) {
            end = clock(); error = e;
            throw e;
        } finally {
            active = false;
            const cli = cliPid();
            emit({kind:'timing_exchange',...base,start,end,cli_pid:cli,native_delivery:!!result,
                exit_code:result?.exitCode ?? null,class:result?.class ?? null,
                stdout_sha256:result ? hash(result.stdout) : null,stderr_sha256:result ? hash(result.stderr) : null,
                error:error ? String(error.message ?? error) : null});
        }
    }
    return {begin,invoke,
        bound(result) {
            need(question && !grant && typeof result.scope_identity === 'string', 'missing original W4 grant');
            grant = result.scope_identity;
        },
        complete(evidence) {
            need(question?.question_id === evidence.id && Array.isArray(evidence.probes) &&
                calls === 2 + evidence.probes.length, 'original delivered menu differs');
        },
        end(id) {
            need(question?.question_id === id && !active && grant && calls >= 2, 'unfinished timing question');
            emit({kind:'timing_question_completed',...question,scope:grant,exchanges:calls});
            question = undefined; grant = undefined;
        },
        finish() { need(next === 4 && question === undefined && !active, 'incomplete timing phase'); }
    };
}
module.exports = {timing,ARMS};
