from argparse import Namespace
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import candidate

spec = importlib.util.spec_from_file_location('rspec_adapter', Path(__file__).parent / 'adapters/rspec.py')
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class AdapterTest(unittest.TestCase):
    def exercise(self, variant):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            wasm = root / 'input.wasm'
            wasm.write_bytes(candidate.COMPONENT_HEADER)
            args = Namespace(wasm=wasm, config=root/'config.json', output=root/'bundle',
                producer='example', source_reference='urn:source', revision='42',
                build_reference='build:1', test_reference='test:1')
            def command(argv, **kwargs):
                if argv[2] == 'ruby':
                    Path(argv[-1]).write_bytes(candidate.encode({'wit': '4.1.0', 'scope': 'partial', 'operations': []}))
                else:
                    self.assertEqual(kwargs['env']['CONNECTOR_WASM_PATH'], str(wasm.resolve()))
                    if variant == 'exit_failure':
                        raise subprocess.CalledProcessError(1, argv)
                    summary = {'example_count': 1, 'failure_count': 0, 'pending_count': 0, 'errors_outside_of_examples_count': 0}
                    if variant == 'pending':
                        summary['pending_count'] = 1
                    if variant == 'missing_failure_proof':
                        del summary['failure_count']
                    if variant == 'changed_bytes':
                        wasm.write_bytes(candidate.COMPONENT_HEADER + b'x')
                    Path(argv[-1]).write_bytes(candidate.encode({'summary': summary, 'examples': [{'status': 'passed'}]}))
            with patch.object(adapter.subprocess, 'run', side_effect=command):
                if variant == 'pass':
                    adapter.run(args)
                    result = candidate.validate(args.output, '42', expected_wit='4.1.0')
                    self.assertFalse(result['origin_verified'])
                else:
                    with self.assertRaises((ValueError, KeyError, subprocess.CalledProcessError)):
                        adapter.run(args)
                    self.assertFalse(args.output.exists())

    def test_result_and_byte_guards(self):
        for variant in ['pass', 'exit_failure', 'pending', 'missing_failure_proof', 'changed_bytes']:
            with self.subTest(variant=variant):
                self.exercise(variant)
