# Historical recipe group

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

See the parent history README and source-map.json. Descendants preserve distinct
source versions only; no model/data payload or fresh-run qualification is included.
