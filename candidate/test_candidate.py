import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import candidate
from make_fixture import make

ROOT = Path(__file__).parent


class Conformance(unittest.TestCase):
    def test_shared_vectors(self):
        for case in json.loads((ROOT / 'vectors/cases.json').read_text()):
            with self.subTest(case=case['name']), tempfile.TemporaryDirectory() as folder:
                bundle = Path(folder) / 'bundle'
                shutil.copytree(ROOT / 'vectors/valid-external', bundle)
                for edit in case['edits']:
                    path = bundle / edit['file']
                    if edit['op'] == 'delete':
                        path.unlink()
                    elif edit['op'] == 'append':
                        with path.open('ab') as stream:
                            stream.write(edit['text'].encode())
                    elif edit['op'] == 'replace':
                        path.write_text(edit['text'])
                    elif edit['op'] == 'symlink':
                        path.unlink()
                        path.symlink_to(edit['target'])
                    elif edit['op'] == 'set':
                        document = json.loads(path.read_text())
                        parent = document
                        for key in edit['path'][:-1]:
                            parent = parent[key]
                        parent[edit['path'][-1]] = edit['value']
                        path.write_bytes(candidate.encode(document))
                    elif edit['op'] == 'rebind':
                        manifest_path = bundle / 'manifest.json'
                        manifest = json.loads(manifest_path.read_text())
                        manifest[edit['field']]['sha256'] = candidate.sha(path.read_bytes())
                        manifest[edit['field']]['size'] = path.stat().st_size
                        manifest_path.write_bytes(candidate.encode(manifest))
                if case['accept']:
                    result = candidate.validate(bundle, case.get('expected_revision', 'revision-42'), expected_wit=case.get('expected_wit', '5.0.0'))
                    self.assertFalse(result['origin_verified'])
                    self.assertFalse(result['publication_authorized'])
                else:
                    with self.assertRaises((ValueError, KeyError)):
                        candidate.validate(bundle, case.get('expected_revision', 'revision-42'), expected_wit=case.get('expected_wit', '5.0.0'))

    def test_external_cli_without_ci_or_standout_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            bundle = Path(folder) / 'bundle'
            subprocess.run([sys.executable, str(ROOT / 'make_fixture.py'), str(bundle)], env={}, check=True)
            result = subprocess.run([sys.executable, str(ROOT / 'candidate.py'), 'verify', str(bundle), '--expected-revision', 'revision-42'], env={}, check=True, capture_output=True, text=True)
            response = json.loads(result.stdout)
            self.assertFalse(response['publication_authorized'])
            self.assertFalse(response['executable'])
            self.assertEqual(response['profile'], 'conformance_fixture')

    def test_size_and_json_limits(self):
        with tempfile.TemporaryDirectory() as folder:
            bundle = Path(folder) / 'bundle'
            make(bundle)
            (bundle / 'contract.json').write_bytes(b' ' * (candidate.LIMITS['contract.json'] + 1))
            with self.assertRaisesRegex(ValueError, 'size_limit'):
                candidate.validate(bundle, 'revision-42')
        for data in [b'{"x":NaN}', b'{"x":1e999}', b'"\\ud800"', b'[' * 34 + b'0' + b']' * 34, b'\xff', b'9' * 129]:
            with self.assertRaises((ValueError, UnicodeError)):
                candidate.decode(data)

    def test_raw_json_whitespace_is_significant(self):
        with tempfile.TemporaryDirectory() as folder:
            bundle = Path(folder) / 'bundle'
            make(bundle)
            with (bundle / 'contract.json').open('ab') as stream:
                stream.write(b'\n')
            with self.assertRaises(ValueError):
                candidate.validate(bundle, 'revision-42')

    def test_existing_output_is_preserved(self):
        with tempfile.TemporaryDirectory() as folder:
            bundle = Path(folder) / 'bundle'
            make(bundle)
            before = (bundle / 'manifest.json').read_bytes()
            with self.assertRaises(FileExistsError):
                make(bundle)
            self.assertEqual(before, (bundle / 'manifest.json').read_bytes())


if __name__ == '__main__':
    unittest.main()
