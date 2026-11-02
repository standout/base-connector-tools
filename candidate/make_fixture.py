"""External producer conformance fixture: envelope tests, NOT an executable connector demo."""
from pathlib import Path
import argparse
import tempfile
import candidate


def make(output):
    # A valid empty component header. No Standout interface implementation is claimed as tested.
    # WIT is an unverified producer declaration, just as all offline envelopes' WIT fields are.
    identity = {'reference': 'urn:example:external-source', 'revision': 'revision-42'}
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        wasm = root / 'fixture.wasm'
        wasm.write_bytes(candidate.COMPONENT_HEADER)
        contract = root / 'contract.json'
        contract.write_bytes(candidate.encode({'wit': '5.0.0', 'scope': 'partial', 'operations': []}))
        tests = root / 'tests.json'
        tests.write_bytes(candidate.encode({'source': identity, 'build_reference': 'urn:example:build:12',
            'test_reference': 'urn:example:test:12', 'artifact_sha256': candidate.sha(wasm.read_bytes()),
            'contract_sha256': candidate.sha(contract.read_bytes()), 'wit': '5.0.0', 'status': 'passed',
            'checks': [{'name': 'envelope-fixture-shape-only', 'status': 'passed'}],
            'details': {'executable_connector_tested': False}}))
        candidate.package(wasm, contract, tests, output, 'urn:example:external-producer', identity,
                          'urn:example:build:12', 'conformance_fixture')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    make(parser.parse_args().output)
