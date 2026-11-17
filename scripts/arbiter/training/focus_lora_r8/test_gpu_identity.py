# SPDX-License-Identifier: Apache-2.0
import unittest
from gpu_identity import canonical_uuid,describe_gpu,check_gpu_identity

BARE='08e2b3f6-6a39-3456-77cb-a57287a702ec'
EXPECTED={'uuid':'GPU-'+BARE,'name':'NVIDIA RTX A6000','compute_capability':[8,6]}
RESERVE=8589934592


class GPUIdentityControls(unittest.TestCase):
    def test_literal_PyTorch_and_NVML_are_same128bit_identity(self):
        for raw in [BARE,'GPU-'+BARE,BARE.upper(),'GPU-'+BARE.upper()]:
            self.assertEqual(canonical_uuid(raw),BARE)
            actual=describe_gpu('NVIDIA RTX A6000',raw,[8,6],49886003200,51527024640)
            self.assertEqual(actual['uuid_raw'],raw);self.assertEqual(actual['uuid_canonical'],BARE)
            self.assertTrue(check_gpu_identity(EXPECTED,actual,RESERVE))
    def test_MIG_unknown_prefix_malformed_and_distinct_bytes_refuse(self):
        for raw in ['MIG-'+BARE,'gpu-'+BARE,'GPU-GPU-'+BARE,' '+BARE,BARE+'\n',BARE.replace('-',''),'not-a-uuid',None]:
            actual=describe_gpu(EXPECTED['name'],raw,[8,6],RESERVE,RESERVE)
            self.assertIsNone(actual['uuid_canonical'])
            with self.assertRaises(ValueError):check_gpu_identity(EXPECTED,actual,RESERVE)
        actual=describe_gpu(EXPECTED['name'],'18e2b3f6-6a39-3456-77cb-a57287a702ec',[8,6],RESERVE,RESERVE)
        with self.assertRaises(ValueError):check_gpu_identity(EXPECTED,actual,RESERVE)
    def test_name_capability_and_memory_are_independent_guards(self):
        for name,cap,free,total in [('Other GPU',[8,6],RESERVE,RESERVE),(EXPECTED['name'],[8,9],RESERVE,RESERVE),(EXPECTED['name'],[8,True],RESERVE,RESERVE),(EXPECTED['name'],[8,6],RESERVE-1,RESERVE),(EXPECTED['name'],[8,6],RESERVE+1,RESERVE)]:
            actual=describe_gpu(name,BARE,cap,free,total)
            with self.assertRaises(ValueError):check_gpu_identity(EXPECTED,actual,RESERVE)
    def test_invalid_metadata_remains_diagnostic_before_refusal(self):
        actual=describe_gpu('actual name','MIG-unknown',[0,0],1,2)
        self.assertEqual(actual,{'name':'actual name','uuid_raw':'MIG-unknown','uuid_canonical':None,'compute_capability':[0,0],'free_bytes':1,'total_bytes':2})
        with self.assertRaises(ValueError):check_gpu_identity(EXPECTED,actual,RESERVE)

if __name__=='__main__':unittest.main(verbosity=2)
