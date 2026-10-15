// Original receipt shader 🌱.
struct Receipt { amount: f32, }
@group(0) @binding(1) var<uniform> receipt: Receipt;
@compute @workgroup_size(1) fn record() { let amount = receipt.amount; }
