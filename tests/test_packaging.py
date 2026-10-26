import os
import sys
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

try:
    import main
    PYSIDE_AVAILABLE = True
except ImportError:
    PYSIDE_AVAILABLE = False


@unittest.skipUnless(PYSIDE_AVAILABLE, "PySide6 not installed")
class AppBaseDirTests(unittest.TestCase):
    def test_uses_script_directory_when_running_from_source(self):
        self.assertFalse(getattr(sys, "frozen", False))
        expected = os.path.dirname(os.path.abspath(main.__file__))
        self.assertEqual(main.app_base_dir(), expected)

    def test_uses_executable_directory_when_frozen(self):
        had_frozen = hasattr(sys, "frozen")
        original_frozen = getattr(sys, "frozen", None)
        original_executable = sys.executable
        try:
            sys.frozen = True
            sys.executable = os.path.join(
                "C:\\", "Portable", "GameMagNet", "GameMagNet.exe"
            )
            self.assertEqual(
                main.app_base_dir(),
                os.path.join("C:\\", "Portable", "GameMagNet"),
            )
        finally:
            if had_frozen:
                sys.frozen = original_frozen
            else:
                del sys.frozen
            sys.executable = original_executable


class PackagingSpecTests(unittest.TestCase):
    """Guard against the spec drifting away from the packaged app."""

    def test_spec_targets_the_qml_app_and_bundles_resources(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        spec_path = os.path.join(root, "GameMagNet.spec")
        self.assertTrue(os.path.isfile(spec_path))
        with open(spec_path, encoding="utf-8") as handle:
            spec = handle.read()
        self.assertIn("main.py", spec)
        self.assertIn('ROOT / "qml"', spec)
        self.assertIn('ROOT / "assets"', spec)


if __name__ == "__main__":
    unittest.main()
