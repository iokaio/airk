// SPDX-License-Identifier: Apache-2.0
import { render } from 'preact';
import { App } from './app';
import './styles.css';

const root = document.getElementById('root');
if (root) { render(<App />, root); }
