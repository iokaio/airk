// SPDX-License-Identifier: Apache-2.0
// Current browser guided destinations; this catalog grants no native authority.
import {GUIDED_EVIDENCE_ROUTES,isGuidedEvidenceRoute} from './guidedEvidence';
import {BROWSER_REVIEW_ROUTES,isBrowserReviewRoute} from './ReviewScreen';
import {BROWSER_STRATEGY_ROUTES,isBrowserStrategyRoute} from './StrategiesScreen';
export const BROWSER_EVIDENCE_ROUTES=[...GUIDED_EVIDENCE_ROUTES,...BROWSER_REVIEW_ROUTES,...BROWSER_STRATEGY_ROUTES] as const;
export type BrowserEvidenceRoute=typeof BROWSER_EVIDENCE_ROUTES[number];
export const isBrowserEvidenceRoute=(v:unknown):v is BrowserEvidenceRoute=>isGuidedEvidenceRoute(v)||isBrowserReviewRoute(v)||isBrowserStrategyRoute(v);
