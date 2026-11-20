# Store examples

`registry_schema` prints compiled registry DDL from an empty, disposable namespace.

`observe_graph_candidate_reads` compares the canonical candidate-to-occurrence-pointer resolution with a single authenticated graph/binding lookup on an isolated, byte-verified clone of the CAL development index. Pass the clone’s collection directory, original source root, incarnation, generation and head digest. For the first 64 and 400 live graph IDs it times both paths in reversed order, then checks every ordered pointer stream by SHA-256, including source hashes and path ordinals. This measures pointer materialization at the supported native widths. It runs no embedding or Focus inference and does not reproduce the query’s ANN ranking or full service timing.
