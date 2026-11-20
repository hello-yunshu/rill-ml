import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from check_runtime_semver import validate_doc


class HistoricalRustdoc(unittest.TestCase):
    def test_reject_wrong_version_or_missing_private_items(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'runtime.json'
            for data in [{'crate_version': '1.5.3'},
                         {'crate_version': '1.5.3', 'includes_private': False},
                         {'crate_version': '1.5.0', 'includes_private': True}]:
                path.write_text(json.dumps(data))
                with self.assertRaises(ValueError):validate_doc(path, '1.5.3')
            path.write_text(json.dumps({'crate_version': '1.5.3', 'includes_private': True}))
            self.assertEqual(len(validate_doc(path, '1.5.3')), 64)
