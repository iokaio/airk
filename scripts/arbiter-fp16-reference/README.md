# Independent Context numerical references

The CPU tools in this directory generate matching-recipe score vectors from the original model weights and native-compiled request inputs. They retain the original publisher FP32 scores separately. Each result identifies the input, model, math recipe and bounded worker that produced it. These files support the [native execution tools](../arbiter-fp16-execution/README.md).

For Large, `run_large_t32.py` verifies the conversion plan and model closure before invoking the selected CPU recipe. `large_body_reference.py` retains the intermediate half roundings. `large_weight_only_reference.py` uses FP32 inputs and outputs for the 252 dense projections with half-stored weights and compensated attention. `large_roundback_panel.py` isolates the first layer's measured projection boundaries on the original operands.

Use the pinned CPU Torch environment and the checked same-user runner. See the [reference contract](CONTRACT.md) for original inputs, budgets and supported modes. Keep model assets and score receipts in the private campaign directory.
