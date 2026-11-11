// SPDX-License-Identifier: Apache-2.0
//! Test-only stdio transport. Production never starts an MCP server over stdio.
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
export const stdioLaunch = (command: string, args: string[], options: { cwd: string; env: NodeJS.ProcessEnv }): ChildProcessWithoutNullStreams =>
    spawn(command, args, { ...options, windowsHide: true, stdio: 'pipe' });
