// SPDX-License-Identifier: Apache-2.0
import { IndexProgress, parseIndexProgress } from './native';

export interface ProgressObservation { event: IndexProgress; started: number; updated: number }
/** Collection-bound display state; foreign workers cannot refresh its idle timer. */
export class IndexProgressTracker {
    private observation?: ProgressObservation;
    constructor(private readonly collection: string, private readonly publish: (value: ProgressObservation) => void,
        private readonly now: () => number = Date.now) {}
    start(): void {
        if (this.observation) { return; }
        const now = this.now();
        this.observation = { event: { collection: this.collection, phase: 'scan_started', revision: null,
            processed_artifacts: 0, committed_artifacts: 0, selected_artifacts: null, completed_artifacts: null }, started: now, updated: now };
        this.publish(this.observation);
    }
    line(line: string): boolean {
        const event = parseIndexProgress(line);
        if (!event || event.collection !== this.collection) { return false; }
        this.start();
        if (event.phase.startsWith('waiting_')) {
            this.observation = { ...this.observation!, event: { ...this.observation!.event, phase: event.phase } };
            this.publish(this.observation); return true;
        }
        this.observation = { event, started: this.observation!.started, updated: this.now() };
        this.publish(this.observation); return true;
    }
    waiting(): void {
        if (this.observation && !this.observation.event.phase.startsWith('waiting_')) { this.observation = { ...this.observation, event: { ...this.observation.event, phase: 'waiting' } }; this.publish(this.observation); }
    }
}
