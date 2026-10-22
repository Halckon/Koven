"""Shared setup must refresh the runtime closure, not inherit runner image drift."""
from pathlib import Path
import shlex
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PreviewDependencySetup(unittest.TestCase):
    def test_llvm_install_explicitly_refreshes_existing_lzma_runtime(self):
        source = (ROOT / 'scripts/install_ci_llvm.sh').read_text().replace('\\\n', ' ')
        installs = [shlex.split(line) for line in source.splitlines()
                    if line.startswith('sudo apt-get install ')]
        self.assertEqual(len(installs), 1)
        self.assertIn('liblzma5', installs[0],
                      'LLVM dependency satisfaction alone retains different preinstalled liblzma builds')
        self.assertNotIn('--allow-unauthenticated', installs[0])

    def test_producer_and_consumer_share_the_same_setup(self):
        producer = (ROOT / '.github/actions/setup-llvm/action.yml').read_text()
        consumer = (ROOT / 'scripts/install_preview_dependencies.sh').read_text()
        self.assertIn('bash scripts/install_ci_llvm.sh', producer)
        self.assertIn('bash "$script_directory/install_ci_llvm.sh"', consumer)


if __name__ == '__main__':
    unittest.main()
