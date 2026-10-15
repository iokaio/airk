// Original fixture: receipts 🌱, no GPU execution.
namespace receipts { struct Ledger { int amount; }; }
__global__ void record(float *values) { values[0] = 1.0f; }
__global__ __launch_bounds__(32) void bounded(float *values) { values[0] = 2.0f; }
void submit(float *values) { record<<<1, 32>>>(values); }
