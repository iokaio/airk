// SPDX-License-Identifier: Apache-2.0
import { render } from 'preact';
import './styles.css';

function App() {
    return <main class="airk-explorer"><p>AIRK Explorer</p></main>;
}

const root = document.getElementById('root');
if (root) { render(<App />, root); }
