// SPDX-License-Identifier: Apache-2.0
// The injected client probe precedes the ordinary installed-driver Stop owner.
exports.withFailureStop=async({run,stop,publishStop,retainFailure})=>{
    try{return await run();}
    catch(error){
        const evidence={client_error:String(error),stop_attempted:true,stop_receipt_published:false};
        try{
            // Exactly one mutation dispatch. An uncertain Stop or failed receipt
            // write remains incomplete; never restart or repeat it for evidence.
            const receipt=await stop();
            await publishStop(receipt);
            evidence.stop_receipt_published=true;
        }catch(cleanupError){evidence.cleanup_error=String(cleanupError);}
        try{await retainFailure(evidence);}
        catch(retentionError){throw new AggregateError([error,retentionError],'Client failed and cleanup evidence could not be retained');}
        throw error;
    }
};
