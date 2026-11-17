# SPDX-License-Identifier: Apache-2.0
"""Independent BNSH to BSNH-flat sentinel control; no model or graph execution."""
import unittest

def repack(heads,tokens,width):
    source=[[[100*h+10*t+d for d in range(width)] for t in range(tokens)] for h in range(heads)]
    # Transpose head/token, then flatten only the last two axes.
    output=[[source[h][t][d] for h in range(heads) for d in range(width)] for t in range(tokens)]
    restored=[[[output[t][h*width+d] for d in range(width)] for t in range(tokens)] for h in range(heads)]
    return source,output,restored
class Layout(unittest.TestCase):
    def test_explicit_transpose_recovers_every_qkv_axis(self):
        for heads in [2,4]:
            source,output,restored=repack(heads,3,2)
            self.assertEqual(source,restored)
            self.assertEqual(output[1],[10,11,110,111] if heads==2 else [10,11,110,111,210,211,310,311])
    def test_direct_reshape_is_not_equivalent(self):
        source,output,_=repack(2,3,2)
        flat=[value for head in source for row in head for value in row]
        direct=[flat[i:i+4] for i in range(0,len(flat),4)]
        self.assertNotEqual(direct,output)
if __name__=='__main__':unittest.main()
