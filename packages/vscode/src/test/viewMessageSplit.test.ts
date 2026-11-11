// SPDX-License-Identifier: Apache-2.0
// Split plan C9: every webview message type belongs to exactly one side of the
// reviewed split (shared UI, VS Code shell only, admin pages only), and every
// capability it needs is a reviewed capability flag. The fixtures are the
// independent expectation; this test fails when a message is added or removed
// without classifying it.
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {WEBVIEW_MESSAGES} from '../explorerContracts';

const fixtures=path.resolve(__dirname,'../../../../fixtures/split/ui');
type Side='ui'|'editor'|'admin';
const split=JSON.parse(fs.readFileSync(path.join(fixtures,'view_messages.json'),'utf8')) as {
    schema:string;counts:Record<Side,number>;messages:{type:string;side:Side;capability:string|null}[];
};
const capabilities=JSON.parse(fs.readFileSync(path.join(fixtures,'capabilities.json'),'utf8')) as {
    schema:string;both_shells:string[];editor_only:string[];admin_only:string[];
};

function messageTypes():string[]{
    const value=WEBVIEW_MESSAGES as unknown;
    if(Array.isArray(value))return value as string[];
    if(value instanceof Set)return [...value] as string[];
    return Object.keys(value as Record<string,unknown>);
}

test('every webview message type is classified exactly once',()=>{
    assert.equal(split.schema,'airk.split-fixture.view-messages/1');
    const types=split.messages.map(entry=>entry.type);
    assert.ok(types.length>0,'zero classified messages is a failure');
    assert.equal(new Set(types).size,types.length,'a message type is classified twice');
    assert.deepEqual([...types].sort(),[...messageTypes()].sort());
});

test('side counts match the reviewed totals',()=>{
    const observed:Record<Side,number>={ui:0,editor:0,admin:0};
    for(const entry of split.messages){
        assert.ok(entry.side in observed,`unknown side ${entry.side}`);
        observed[entry.side]++;
    }
    assert.deepEqual(observed,split.counts);
});

test('capabilities are reviewed flags available on the message side',()=>{
    assert.equal(capabilities.schema,'airk.split-fixture.ui-capabilities/1');
    const both=new Set(capabilities.both_shells),editor=new Set(capabilities.editor_only);
    for(const entry of split.messages){
        if(entry.capability===null)continue;
        if(entry.side==='ui')assert.ok(both.has(entry.capability),`${entry.type}: ${entry.capability} must exist in both shells`);
        else if(entry.side==='editor')assert.ok(editor.has(entry.capability)||both.has(entry.capability),`${entry.type}: unknown capability ${entry.capability}`);
        else assert.fail(`${entry.type}: admin messages need no editor capability`);
    }
});

test('management messages never stay in the editor',()=>{
    for(const entry of split.messages){
        if(/^(manage|usage)\./.test(entry.type))assert.equal(entry.side,'admin',entry.type);
        if(entry.type.startsWith('indexing.')&&!entry.type.startsWith('indexing.scope.'))assert.equal(entry.side,'admin',entry.type);
    }
});
