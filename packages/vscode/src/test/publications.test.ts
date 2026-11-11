// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { comparePublication, publicationRelationLabel, type PublicationObservation } from '../publications';
const incarnation='ab'.repeat(16),digest='cd'.repeat(32);
const token=(generation:string,revision=digest,owner=incarnation)=>`airk-r1:${owner}:${generation}:${revision}`;
const observation=(publication:unknown,authority='host-authority',collection='one'):PublicationObservation=>({authority,collection,publication});

test('publication comparison orders only an established authority, collection and incarnation',()=>{
    const first=observation(token('9')),second=observation({token:token('10'),incarnation,generation:10,collection:'one',revision:digest});
    assert.equal(comparePublication(first,second),'older');assert.equal(comparePublication(second,first),'newer');
    assert.equal(comparePublication(first,structuredClone(first)),'equal');
    assert.equal(comparePublication(first,observation(token('9','ef'.repeat(32)))),'inconsistent');
    for(const foreign of [observation(token('10'),'other'),observation(token('10'),'host-authority','two'),observation(token('10',digest,'ef'.repeat(16)))])assert.equal(comparePublication(first,foreign),'different');
    assert.equal(comparePublication(observation(token('18446744073709551614')),observation(token('18446744073709551615'))),'older');
    assert.equal(comparePublication(observation(token('9007199254740992')),observation(token('9007199254740993'))),'older');
    assert.match(publicationRelationLabel('older')!,/does not establish ancestry or live-source freshness/);
    assert.equal(publicationRelationLabel('unknown'),undefined);assert.equal(publicationRelationLabel('equal'),undefined);
});

test('bounded canonical tokens and consistent envelopes are required before any ordering',()=>{
    const valid=observation(token('7'));
    for(const malformed of [undefined,null,{},'',token('07'),token('-1'),token('1.0'),token('18446744073709551616'),token('0'.repeat(100000)),token('7').toUpperCase(),token('7')+'x',token('7')+'\n',
        {token:token('7'),generation:8},{token:token('7'),generation:'07'},{token:token('7'),generation:null},
        {token:token('9007199254740993'),generation:9007199254740993},
        {token:token('7'),incarnation:'ef'.repeat(16)},{token:token('7'),revision:'ef'.repeat(32)},{token:token('7'),collection:'two'}]){
        assert.equal(comparePublication(observation(malformed),valid),'unknown');
        assert.equal(comparePublication(valid,observation(malformed)),'unknown');
    }
    assert.equal(comparePublication(undefined,valid),'unknown');assert.equal(comparePublication(valid),'unknown');
    assert.equal(comparePublication(observation(token('7'),''),valid),'unknown');
    assert.equal(comparePublication(observation({token:token('18446744073709551615'),generation:'18446744073709551615'}),observation(token('18446744073709551615'))),'equal');
    assert.equal(comparePublication(observation({token:token('0'),generation:0}),observation(token('0'))),'equal');
    const before=structuredClone(valid);comparePublication(valid,valid);assert.deepEqual(valid,before);
});
