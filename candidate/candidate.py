#!/usr/bin/env python3
"""Offline unsigned candidate envelope v2. Never executes WASM or authorizes publication."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil

VERSION = '0.1.0'
FORMAT = 'integrationer.verified-wasm-candidate.v2'
LIMITS = {'connector.wasm': 16 * 1024 * 1024, 'contract.json': 1024 * 1024,
          'tests.json': 1024 * 1024, 'manifest.json': 64 * 1024}
COMPONENT_HEADER = b'\x00asm\x0d\x00\x01\x00'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path, maximum):
    require(not path.is_symlink() and path.is_file(), 'regular_file_required')
    require(path.stat().st_size <= maximum, 'size_limit')
    with path.open('rb') as stream:
        data = stream.read(maximum + 1)
    require(len(data) <= maximum, 'size_limit')
    return data


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate_json_key')
        result[key] = value
    return result


def decode(data):
    def invalid(value):
        raise ValueError('non_finite_json')
    result = json.loads(data.decode('utf-8'), object_pairs_hook=pairs, parse_constant=invalid)
    def walk(value, depth=0):
        require(depth <= 32, 'json_depth_limit')
        if isinstance(value, str):
            value.encode('utf-8')  # Reject lone surrogate escapes.
        if isinstance(value, dict):
            for key, child in value.items():
                walk(key, depth + 1)
                walk(child, depth + 1)
        elif isinstance(value, list):
            for child in value:
                walk(child, depth + 1)
        elif isinstance(value, float):
            import math
            require(math.isfinite(value), 'non_finite_json')
    walk(result)
    return result


def encode(value):
    # Writer convenience only: verification hashes original bytes, not this serialization.
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, indent=2) + '\n').encode('utf-8')


def keys(value, expected):
    require(isinstance(value, dict) and set(value) == set(expected.split()), 'unexpected_fields')


def text(value):
    require(isinstance(value, str) and 0 < len(value) <= 512 and not any(ord(c) < 32 for c in value), 'invalid_text')


def source(value):
    keys(value, 'reference revision')
    for item in value.values():
        text(item)


def validate(bundle, expected_revision, expected_wasm=None, expected_wit=None):
    require(not bundle.is_symlink() and bundle.is_dir(), 'directory_required')
    require({p.name for p in bundle.iterdir()} == set(LIMITS), 'unexpected_files')
    data = {name: read(bundle / name, limit) for name, limit in LIMITS.items()}
    require(data['connector.wasm'].startswith(COMPONENT_HEADER), 'component_header_required')
    manifest, contract, tests = (decode(data[name]) for name in ('manifest.json', 'contract.json', 'tests.json'))
    keys(manifest, 'format profile producer source build_reference artifact contract tests tool origin_verified publication_authorized published')
    require(manifest['format'] == FORMAT, 'unsupported_format')
    require(manifest['profile'] in ('connector', 'conformance_fixture'), 'unsupported_profile')
    for flag in ('origin_verified', 'publication_authorized', 'published'):
        require(manifest[flag] is False, 'authority_claim_forbidden')
    text(manifest['producer'])
    text(manifest['tool'])
    text(manifest['build_reference'])
    source(manifest['source'])
    require(manifest['source']['revision'] == expected_revision, 'revision_mismatch')
    for field, filename in (('artifact', 'connector.wasm'), ('contract', 'contract.json'), ('tests', 'tests.json')):
        entry = manifest[field]
        keys(entry, 'file sha256 size wit' if field == 'artifact' else 'file sha256 size')
        require(entry['file'] == filename, 'unexpected_filename')
        require(type(entry['size']) is int and entry['size'] == len(data[filename]), 'size_mismatch')
        require(entry['sha256'] == sha(data[filename]), 'digest_mismatch')
    wit = manifest['artifact']['wit']
    require(isinstance(wit, str) and re.fullmatch(r'\d+\.\d+\.\d+', wit), 'invalid_wit')
    require(expected_wasm is None or manifest['artifact']['sha256'] == expected_wasm, 'wasm_mismatch')
    require(expected_wit is None or wit == expected_wit, 'wit_mismatch')
    keys(contract, 'wit scope operations')
    require(contract['wit'] == wit and contract['scope'] in ('complete', 'partial'), 'contract_mismatch')
    require(isinstance(contract['operations'], list) and len(contract['operations']) <= 1000, 'operations_limit')
    seen = set()
    for operation in contract['operations']:
        keys(operation, 'kind name input_schema output_schema')
        require(operation['kind'] in ('action', 'trigger'), 'operation_kind')
        text(operation['name'])
        key = (operation['kind'], operation['name'])
        require(key not in seen, 'duplicate_operation')
        seen.add(key)
        require(all(isinstance(operation[k], (dict, bool)) for k in ('input_schema', 'output_schema')), 'invalid_schema_shape')
    keys(tests, 'source build_reference test_reference artifact_sha256 contract_sha256 wit status checks details')
    require(tests['source'] == manifest['source'] and tests['build_reference'] == manifest['build_reference'], 'tests_source_mismatch')
    text(tests['test_reference'])
    require(tests['artifact_sha256'] == manifest['artifact']['sha256'] and tests['contract_sha256'] == manifest['contract']['sha256'] and tests['wit'] == wit, 'tests_binding_mismatch')
    require(tests['status'] == 'passed', 'tests_failed')
    require(isinstance(tests['checks'], list) and 0 < len(tests['checks']) <= 1000, 'checks_required')
    names = set()
    for check in tests['checks']:
        keys(check, 'name status')
        text(check['name'])
        require(check['name'] not in names, 'duplicate_check')
        names.add(check['name'])
        require(check['status'] == 'passed', 'check_failed')
    require(isinstance(tests['details'], dict), 'details_object_required')
    return manifest


def package(wasm, contract_path, tests_path, output, producer, source_identity, build_reference, profile='connector'):
    # Inputs are explicit; there is no CI environment, git, registry, URL fetch or runtime dependency.
    raw = {'connector.wasm': read(wasm, LIMITS['connector.wasm']),
           'contract.json': read(contract_path, LIMITS['contract.json']),
           'tests.json': read(tests_path, LIMITS['tests.json'])}
    contract = decode(raw['contract.json'])
    manifest = dict(format=FORMAT, profile=profile, producer=producer, source=source_identity,
                    build_reference=build_reference, tool='base-connector-tools/candidate@' + VERSION,
                    origin_verified=False, publication_authorized=False, published=False)
    for field, filename in [('artifact', 'connector.wasm'), ('contract', 'contract.json'), ('tests', 'tests.json')]:
        manifest[field] = {'file': filename, 'sha256': sha(raw[filename]), 'size': len(raw[filename])}
    manifest['artifact']['wit'] = contract['wit']
    output.mkdir()  # Never overwrite or reuse a previous successful bundle.
    try:
        for filename, content in raw.items():
            (output / filename).write_bytes(content)
        (output / 'manifest.json').write_bytes(encode(manifest))
        validate(output, source_identity['revision'])
    except Exception:
        shutil.rmtree(output)  # Only this newly-created directory, never caller-owned content.
        raise
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    pack = commands.add_parser('package')
    for flag in ('wasm', 'contract', 'tests', 'output'):
        pack.add_argument('--' + flag, type=Path, required=True)
    for flag in ('producer', 'source-reference', 'revision', 'build-reference'):
        pack.add_argument('--' + flag, required=True)
    pack.add_argument('--profile', choices=['connector', 'conformance_fixture'], default='connector')
    check = commands.add_parser('verify')
    check.add_argument('bundle', type=Path)
    check.add_argument('--expected-revision', required=True)
    check.add_argument('--expected-wasm')
    check.add_argument('--expected-wit')
    check.add_argument('--expected-manifest')
    args = parser.parse_args()
    if args.command == 'package':
        result = package(args.wasm, args.contract, args.tests, args.output, args.producer,
                         {'reference': args.source_reference, 'revision': args.revision}, args.build_reference, args.profile)
    else:
        if args.expected_manifest is not None:
            require(sha(read(args.bundle / 'manifest.json', LIMITS['manifest.json'])) == args.expected_manifest, 'manifest_mismatch')
        result = validate(args.bundle, args.expected_revision, args.expected_wasm, args.expected_wit)
    print(json.dumps({'format': result['format'], 'sha256': result['artifact']['sha256'],
                      'origin_verified': False, 'publication_authorized': False}))


if __name__ == '__main__':
    main()
