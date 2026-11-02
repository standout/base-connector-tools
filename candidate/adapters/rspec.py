#!/usr/bin/env python3
"""Optional RSpec/AppBridge adapter. No build, CI variables or publication operations."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import candidate


def run(args):
    wasm = args.wasm.resolve()
    original = candidate.sha(candidate.read(wasm, candidate.LIMITS['connector.wasm']))
    identity = {'reference': args.source_reference, 'revision': args.revision}
    with tempfile.TemporaryDirectory(prefix='candidate-rspec-') as folder:
        temp = Path(folder)
        contract_path = temp / 'contract.json'
        subprocess.run(['bundle', 'exec', 'ruby', str(Path(__file__).with_name('export_contract.rb')),
                        str(wasm), str(args.config), str(contract_path)], check=True)
        report_path = temp / 'rspec.json'
        subprocess.run(['bundle', 'exec', 'rspec', '--format', 'documentation', '--format', 'json', '--out', str(report_path)],
                       env=dict(os.environ, CONNECTOR_WASM_PATH=str(wasm)), check=True)
        report = candidate.decode(candidate.read(report_path, candidate.LIMITS['tests.json']))
        summary = report['summary']
        candidate.require(type(summary['example_count']) is int and summary['example_count'] == len(report['examples']) > 0 and all(type(summary[k]) is int and summary[k] == 0 for k in
                          ['failure_count', 'pending_count', 'errors_outside_of_examples_count']), 'rspec_not_passed')
        candidate.require(all(example['status'] == 'passed' for example in report['examples']), 'rspec_example_failed')
        candidate.require(original == candidate.sha(candidate.read(wasm, candidate.LIMITS['connector.wasm'])), 'wasm_changed_during_tests')
        contract_raw = contract_path.read_bytes()
        tests = {'source': identity, 'build_reference': args.build_reference, 'test_reference': args.test_reference,
                 'artifact_sha256': original, 'contract_sha256': candidate.sha(contract_raw),
                 'wit': candidate.decode(contract_raw)['wit'], 'status': 'passed',
                 'checks': [{'name': 'rspec-domain-suite', 'status': 'passed'}, {'name': 'appbridge-contract-extraction', 'status': 'passed'}],
                 'details': {'runner': 'RSpec', 'report': report}}
        tests_path = temp / 'tests.json'
        tests_path.write_bytes(candidate.encode(tests))
        return candidate.package(wasm, contract_path, tests_path, args.output, args.producer, identity, args.build_reference)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('wasm', 'config', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    for flag in ('producer', 'source-reference', 'revision', 'build-reference', 'test-reference'):
        parser.add_argument('--' + flag, required=True)
    run(parser.parse_args())
