// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';

/** A definition list of observations; values are rendered as text, never HTML. */
export function Facts(props: { rows: [string, ComponentChildren][] }) {
    return <dl class="facts">{props.rows.map(([name, value]) => [<dt key={`${name}-t`}>{name}</dt>, <dd key={`${name}-d`}>{value}</dd>])}</dl>;
}
