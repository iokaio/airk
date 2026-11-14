# SPDX-License-Identifier: Apache-2.0
"""Versioned Python publication contracts; no network, files, signing or execution.

C7 /1 remains owned by split_receipts unchanged. /2 adds portable publications
and their same-run inputs. GitHub /2 additionally describes the official
headless installer selection; digests remain in the complete assets map.
Live producer/run and platform signature qualification remain consumer duties.
"""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import re


def _load(name):
    spec=importlib.util.spec_from_file_location('publication_'+name.replace('-','_'),Path(__file__).with_name(name+'.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

legacy=_load('split_receipts')
standalone=legacy.standalone
CATALOG='airk-release-catalog/2'
RELEASE='airk-github-release/2'
SHA=re.compile(r'[0-9a-f]{64}')
GIT=re.compile(r'[0-9a-f]{40}')
POSITIVE=re.compile(r'[1-9][0-9]*')


def current(version,channel):
    return standalone.portable_required(version,channel)


def portable_assets(version):
    return {name for target in standalone.TARGETS for name in
            (standalone.portable_artifact(version,target),standalone.portable_manifest(version,target))}


def portable_inputs(commit,run_id):
    return {f'airk-portable-{target}-{commit}-{run_id}' for target in standalone.TARGETS}


def release_assets(version,channel):
    return legacy.release_assets(version,channel) | (portable_assets(version) if current(version,channel) else set())


def catalog_names(version,channel,commit,producer,release):
    artifacts,inputs,assets=legacy.catalog_names(version,channel,commit,producer,release)
    if current(version,channel):
        inputs |= portable_inputs(commit,release['run_id']);assets |= portable_assets(version)
    return artifacts,inputs,assets


def installer_rows(version,channel):
    return {target:{'artifact':standalone.artifact(version,target),'manifest':standalone.manifest(version,target),
                    'headless':standalone.headless()} for target in standalone.TARGETS} if standalone.required(version,channel) else {}


def validate_catalog(document):
    if not isinstance(document,dict) or document.get('schema')!=CATALOG:
        raise ValueError('Publication catalog schema differs')
    version,channel=document.get('package_version'),document.get('channel')
    if not isinstance(version,str) or not current(version,channel):
        raise ValueError('Portable catalog requires current publication version')
    projected=deepcopy(document);projected['schema']=legacy.RELEASE_CATALOG
    extra=portable_assets(version)
    if not isinstance(projected.get('assets'),dict) or not extra<=projected['assets'].keys():
        raise ValueError('Portable published asset set differs')
    for name in extra:
        value=projected['assets'].pop(name)
        if not isinstance(value,str) or not SHA.fullmatch(value):
            raise ValueError('Portable published digest differs')
    inputs=portable_inputs(document.get('commit'),document.get('release',{}).get('run_id'))
    values=projected.get('release_inputs')
    if not isinstance(values,list) or not inputs<=set(values) or len(values)!=len(set(values)):
        raise ValueError('Portable same-run inputs differ')
    projected['release_inputs']=[name for name in values if name not in inputs]
    legacy.validate_release_catalog(projected)
    return CATALOG


def validate_release(document):
    fields={'schema','tag','package_version','commit','tree','channel','run_id','attempt','assets','producer','installers'}
    if not isinstance(document,dict) or set(document)!=fields or document.get('schema')!=RELEASE:
        raise ValueError('Publication release schema or fields differ')
    version,channel=document['package_version'],document['channel']
    if not isinstance(version,str) or not current(version,channel):
        raise ValueError('Current publication requires portable contract')
    if not re.fullmatch(r'v'+re.escape(version)+(r'(?:-build\.[1-9][0-9]*)?' if channel=='preview' else ''),str(document['tag'])):
        raise ValueError('Publication release tag differs')
    for key in ('commit','tree'):
        if not isinstance(document[key],str) or not GIT.fullmatch(document[key]):
            raise ValueError('Publication source identity differs')
    for key in ('run_id','attempt'):
        if not isinstance(document[key],str) or not POSITIVE.fullmatch(document[key]):
            raise ValueError('Publication run identity differs')
    assets=document['assets']
    if (not isinstance(assets,dict) or set(assets)!=release_assets(version,channel)
            or any(not isinstance(value,str) or not SHA.fullmatch(value) for value in assets.values())):
        raise ValueError('Publication asset inventory or digest differs')
    if json.dumps(document['installers'],sort_keys=True,separators=(',',':')) != json.dumps(installer_rows(version,channel),sort_keys=True,separators=(',',':')):
        raise ValueError('Official installer capability or selection differs')
    producer=document['producer']
    if not isinstance(producer,dict) or producer.get('commit')!=document['commit']:
        raise ValueError('Publication producer source differs')
    return RELEASE
