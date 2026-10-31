# Verify Windows Assistance IPC access control across user accounts

Draft for private security review under `SECURITY.md`. This is a bounded investigation, not a demonstrated authentication bypass.

The extension's Windows Assistance IPC path relies on Node's named-pipe behavior and a random bearer credential. The review did not establish the effective pipe and descriptor-file access controls under a second Windows account.

## Version and evidence

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. [The Assistance host](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/explorerAssistanceHost.ts#L69) chooses a random pipe name, publishes a credential in a local descriptor and rotates that credential. Its Node `net.createServer`/`listen` call does not specify an explicit Windows security descriptor in this adapter. Native Rust IPC has separate controls and is outside this finding.

No cross-account connection, descriptor disclosure or unauthorized action was reproduced. Random endpoints and credential validation are material mitigations; lack of an explicit ACL in this TypeScript file does not prove that Windows grants another user access.

## Investigation completion criteria

- **AC1:** On a supported Windows installation, record effective pipe and descriptor-file ACLs using two disposable accounts and fictional workspace data.
- **AC2:** Test separately whether the second account can discover/connect to the pipe, read the descriptor, and obtain any authenticated response or action. Test absent, invalid and expired credentials without exposing real tokens.
- **AC3:** Record bounded behavior for unauthenticated connections and credential rotation; distinguish availability observations from authorization failures.
- **AC4:** Produce a source-linked conclusion. If current OS controls and authentication meet the intended single-user boundary, retain that evidence; otherwise propose a focused remediation with a regression test. An inconclusive result must identify the missing evidence rather than assert a bypass or a pass.
