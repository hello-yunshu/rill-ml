import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("baseline", Path(__file__).resolve().parents[1] / "generate_api_baseline.py")
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)

class ApiBaseline(unittest.TestCase):
    def test_read_reexport_spelling_only(self):
        old = "pub fn load<R: alloc::io::read::Read + core::io::seek::Seek>(R)"
        new = "pub fn load<R: std::io::Read + core::io::seek::Seek>(R)"
        self.assertEqual(baseline.canonical_api(old), baseline.canonical_api(new))
        for changed in [new.replace("Seek", "Write"), new.replace("load", "save"), new + " -> bool", new.replace("std::io::Read", "user::Read")]:
            self.assertNotEqual(baseline.canonical_api(old), baseline.canonical_api(changed))
