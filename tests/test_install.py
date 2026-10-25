import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("qs_install", Path(__file__).resolve().parents[1] / "scripts/install.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qs install ")
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.binary = self.home / "source"
        self.binary.write_bytes(b"#!/bin/sh\nprintf 'qs fixture\\n'\n")
        self.binary.chmod(0o755)
        self.rc = self.home / ".zshrc"
        self.rc.write_bytes(b"# personal settings\nexport PRESERVED='yes'\n")
        self.rc.chmod(0o600)

    def install(self, prefix=None):
        return installer.install(self.binary, prefix or self.home / ".softwares", self.rc, self.home)

    def test_installs_runs_and_is_idempotent(self):
        original = self.rc.read_bytes()
        executable, backup = self.install()
        self.assertEqual(backup.read_bytes(), original)
        self.assertEqual(self.rc.stat().st_mode & 0o777, 0o600)
        self.assertEqual(executable.read_bytes(), self.binary.read_bytes())
        self.assertEqual(subprocess.check_output([executable]), b"qs fixture\n")
        content = self.rc.read_bytes()
        _, second_backup = self.install()
        self.assertIsNone(second_backup)
        self.assertEqual(self.rc.read_bytes(), content)
        self.assertEqual(content.count(installer.START), 1)
        shell = shutil.which("zsh") or "/bin/sh"
        env = dict(os.environ, HOME=str(self.home), PATH="/usr/bin:/bin")
        output = subprocess.check_output([shell, "-c", '. "$HOME/.zshrc"; . "$HOME/.zshrc"; command -v qs; printf "%s" "$PATH"'], env=env)
        self.assertTrue(output.startswith(str(executable).encode() + b"\n"))
        self.assertEqual(output.split(b"\n", 1)[1].count(str(executable.parent).encode()), 1)

    def test_prefix_changes_replace_managed_block_and_preserve_suffix(self):
        self.install()
        with self.rc.open("ab") as stream:
            stream.write(b"# user suffix\n")
        prefix = self.home / "custom $dollar 'quote' dir"
        executable, _ = self.install(prefix)
        self.assertTrue(self.rc.read_bytes().endswith(b"# user suffix\n"))
        shell = shutil.which("zsh") or "/bin/sh"
        result = subprocess.check_output([shell, "-c", '. "$HOME/.zshrc"; command -v qs'], env=dict(os.environ, HOME=str(self.home), PATH="/usr/bin:/bin"))
        self.assertEqual(result.strip(), str(executable).encode())

    def test_symlinked_zshrc_is_preserved(self):
        target = self.home / "real config"
        self.rc.rename(target)
        self.rc.symlink_to(target)
        self.install()
        self.assertTrue(self.rc.is_symlink())
        self.assertIn(installer.START, target.read_bytes())

    def test_invalid_markers_do_not_change_binary_or_rc(self):
        self.rc.write_bytes(installer.START + b"\n")
        with self.assertRaises(ValueError):
            self.install()
        self.assertEqual(self.rc.read_bytes(), installer.START + b"\n")
        self.assertFalse((self.home / ".softwares/bin/qs").exists())


if __name__ == "__main__":
    unittest.main()
