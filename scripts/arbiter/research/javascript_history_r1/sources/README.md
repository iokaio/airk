# Retained standalone JavaScript templates

Hash-prefixed .js.in files preserve specific historical editor driver/test or
help-search versions. Parent source-map.json records original paths, byte hashes
and any literal SPDX prefix. These files are not mounted, imported or executed.

Driver sources implement extension activation and assert installation/update or
Explorer behavior. They require the original VS Code/test extension environment,
private run paths and matching native tools. Help-search manipulates the static
preview DOM. Capture and syntax checks grant no runtime or release qualification.

The separate source_policy/source-map.json preserves31 aliases of one historical
sourcePolicy.js implementation (b39d1a6a...). Its validatePolicyChangePreview
variant was not found in current or reachable Git source; related current
validators live in packages/vscode/src/indexingCatalog.ts. This module imports
only AIRK indexingCatalog/publications, contains no bundled vendor code, and is
retained as an inert historical template. The original55 driver/test/search
templates remain separately counted. Capture uses -SourcePolicySupplement.
