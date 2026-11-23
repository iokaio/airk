// SPDX-License-Identifier: Apache-2.0
// The Answer /3 default budget comes from the Rust-emitted semantic cases
// (airk-core answer_loop); the bundlers inline only the field named here.
import { default_budget } from '../../../fixtures/split/arbiter/answer_loop_cases.json';
import type { BudgetV3 } from '../../contracts-gen/arbiter';

export const ANSWER_DEFAULT_BUDGET: Readonly<BudgetV3> = Object.freeze({ ...(default_budget satisfies BudgetV3) });
