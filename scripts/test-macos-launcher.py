"""Exercise the packaged launcher without opening the GUI or touching user data."""
import concurrent.futures
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class LauncherTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="open-bim-launcher-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "User home with spaces"
        self.home.mkdir()
        self.project = self.home / "Open BIM проект.obim"
        mac = self.root / "Open BIM.app/Contents/MacOS"
        mac.mkdir(parents=True)
        resources = mac.parent / "Resources/examples"
        resources.mkdir(parents=True)
        self.example = resources / "steel-release.obim"
        self.example.write_bytes(b"packaged example")
        binary = mac / "open-bim-desktop"
        binary.write_text('#!/bin/sh\nprintf "%s\\0" "$@"\n')
        binary.chmod(0o755)
        source = Path(__file__).with_name("package-macos.sh").read_text()
        self.launcher = mac / "Open BIM"
        self.launcher.write_text(source.split("<<'LAUNCH'\n", 1)[1].split("\nLAUNCH", 1)[0])
        self.launcher.chmod(0o755)
        self.env = dict(os.environ, HOME=str(self.home), TMPDIR=str(self.root))

    def run_launcher(self, *args):
        return subprocess.run([str(self.launcher), *args], env=self.env, capture_output=True)

    def assert_project_args(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(result.stdout.decode().rstrip("\0").split("\0"),
                         ["--open", str(self.project), "--project-path", str(self.project)])

    def test_first_launch_opens_a_user_owned_copy(self):
        self.assert_project_args(self.run_launcher())
        self.assertEqual(self.project.read_bytes(), self.example.read_bytes())

    def test_existing_project_is_opened_without_replacement(self):
        self.project.write_bytes(b"my existing model")
        self.assert_project_args(self.run_launcher())
        self.assertEqual(self.project.read_bytes(), b"my existing model")
        self.assertEqual(self.example.read_bytes(), b"packaged example")

    def test_concurrent_first_launches_do_not_truncate_the_project(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: self.run_launcher(), range(2)))
        for result in results:
            self.assert_project_args(result)
        self.assertEqual(self.project.read_bytes(), b"packaged example")

    def test_explicit_arguments_are_preserved(self):
        args = ["--open", "/tmp/Моя модель.obim", "--project-path", "/tmp/Копия.obim"]
        result = self.run_launcher(*args)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.decode().rstrip("\0").split("\0"), args)
        self.assertFalse(self.project.exists())

    def test_directory_at_project_path_is_an_error(self):
        self.project.mkdir()
        self.assertNotEqual(self.run_launcher().returncode, 0)
        self.assertTrue(self.project.is_dir())

    def test_missing_example_leaves_no_partial_project(self):
        self.example.unlink()
        self.assertNotEqual(self.run_launcher().returncode, 0)
        self.assertFalse(self.project.exists())
        self.assertEqual(list(self.home.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
