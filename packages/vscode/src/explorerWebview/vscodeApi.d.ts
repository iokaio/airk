// SPDX-License-Identifier: Apache-2.0
/** The webview host API injected by VS Code. Only typed messages cross this boundary. */
interface VsCodeWebviewApi {
    postMessage(message: unknown): void;
    getState(): unknown;
    setState(state: unknown): void;
}
declare function acquireVsCodeApi(): VsCodeWebviewApi;
