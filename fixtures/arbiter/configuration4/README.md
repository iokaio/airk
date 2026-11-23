# Retained `airk.arbiter.configuration/4` documents (LH)

Written on 23 November 2026 by the code as it was before Large left the
release catalog (branch `loop/lh` on `feat/arbiter-final` `9d63df97`, after
LH commit 2), by the ignored emitter
`service_runtime::arbiter::device_management::retained_tests::emit_retained_configuration4`
(`airk-service`, all features), into a fresh folder, then copied here
unchanged. They let the `configuration/4` -> `/5` migration be tested
against bytes an installation of that build could hold.

| File | What | SHA-256 |
| --- | --- | --- |
| `binding.json` | The compiled `DeviceCatalogBinding` (`device-catalog/1` with `focus`, `context_compact`, `context_large` slots; source digest of `release-catalog.json`) | d5cb439171b7db7a5da056b817d17af419fbd89ad61741f181af148529f23136 |
| `catalog.json` | The compiled `DeviceCatalog` itself | 5adb423b62240cb8cbbca4a7ea29b66cc751562c767403706a52b2a13d8d9235 |
| `disabled.json` | Both stages disabled (revision 1) | 3fdf6238488e06b7e9723d96d2efb9060a4a9234e11ed8af9bf874936948c3e5 |
| `compact-enabled.json` | Context enabled with `context_compact`, one enrollment (revision 2) | e341c87f4068afcfb18a66272c6cecd1a7265050d63ccfa20c985c0983071ff3 |
| `large-enabled.json` | Context enabled with `context_large`, one enrollment (revision 2) | 785238b59d45e6b04b0476bd983d8eb0e53e86af836360577368caffc22124e1 |
| `device-status.json` | `DeviceStatus` of a real owner with an empty store under that binding | e48caa928005657aa35c4d2d0fda701c73d4ace241c7f13552bab3186fef283f |

Installation `01…01`, source catalog incarnation `02…02` and the
enrollment index `idx_1f…1f` are test values. Never edit these files; a
later state contract adds new fixtures beside them.