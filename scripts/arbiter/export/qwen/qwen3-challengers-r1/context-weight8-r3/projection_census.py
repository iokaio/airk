# SPDX-License-Identifier: Apache-2.0
"""Metadata-only complete projection census; never loads tensor values."""
def census(graph,expected_layers,quantized):
 initializers={tensor.name:tensor for tensor in graph.graph.initializer}
 constant=[];packed=[]
 for node in graph.graph.node:
  if node.op_type=='MatMul' and any(name in initializers for name in node.input):
   if len(node.input)!=2 or node.input[1] not in initializers or initializers[node.input[1]].data_type!=1:raise ValueError('unexpected constant projection parameterization')
   constant.append((node.name,node.input[1]))
  if node.op_type=='Gemm':raise ValueError('unreviewed Gemm projection')
  if node.op_type=='MatMulNBits':
   attrs={a.name:a.i for a in node.attribute if a.name in ['bits','block_size','accuracy_level']}
   if node.domain!='com.microsoft' or attrs.get('bits')!=8 or attrs.get('block_size')!=32 or attrs.get('accuracy_level',0)!=0 or len(node.input)!=3 or node.input[1] not in initializers or initializers[node.input[1]].data_type!=2:raise ValueError('unreviewed packed projection')
   packed.append(node.name)
 expected=expected_layers*7
 if quantized:
  if constant or len(packed)!=expected:raise ValueError('not all constant projections converted; leftover FP32 MatMul')
 else:
  if packed or len(constant)!=expected:raise ValueError('original projection census differs from full architecture')
 embeddings=[t for t in initializers.values() if list(t.dims)==[151936,2048 if expected_layers==28 else 2560]]
 if len(embeddings)!=1 or embeddings[0].data_type!=1:raise ValueError('trained embedding/dynamic-gather readout must remain original FP32')
 return {'layers':expected_layers,'projections_per_layer':7,'expected':expected,'constant_fp32_count':len(constant),'constant_projection_initializers':[item[1] for item in constant],'packed_weight8_count':len(packed),'dynamic_gather_head_and_embedding':'FP32 retained truthfully','embedding_initializer':embeddings[0].name}
