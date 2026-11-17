# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f16b-draft/source/crates/airk-cli/src/unified_mcp/explorer_adapter.rs')
s=p.read_text(encoding='utf-8').replace('        Some(self.primary.as_ref())','        let original: &(dyn std::error::Error + \'static) = self.primary.as_ref();\n        original.source()',1)
s=s.replace('        assert_eq!(error.to_string(), "original reply timeout");','''        assert_eq!(error.to_string(), "original reply timeout");
        assert_eq!(format!("{error:#}"), "original reply timeout");
        let nested = settle_exchange(Err(anyhow::anyhow!("root read cause").context("original reply timeout")), Err(anyhow::anyhow!("cleanup write")), false).unwrap_err();
        assert_eq!(format!("{nested:#}"), "original reply timeout: root read cause");''')
s=s.replace('Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),','Err(anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied)).context("actual writer cause")),')
p.write_text(s,encoding='utf-8',newline='\n')
p=Path('artifacts/optimizations/f16b-draft/IMPLEMENTATION.txt');s=p.read_text(encoding='utf-8');s+='\nError wrapper source correction: secondary attachment preserves primary Display and underlying source chain without repeating its top message in alternate formatting. Plain and nested formatted-chain oracles are drafted. Like the reviewed shared-owner wrappers, attaching a secondary wrapper changes direct downcast identity of a top-only original error; nested typed sources remain exposed. No public ToolResult value is changed by this internal native transport error wrapper.\n';p.write_text(s,encoding='utf-8',newline='\n')
