"""Only synthetic child processes; never invokes baw-rs or opens a wallet."""
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

DRIVER = pathlib.Path(__file__).resolve().parents[1] / "examples" / "fast_driver.py"


class DriverTests(unittest.TestCase):
    def run_driver(self, rows, execute=True):
        with tempfile.TemporaryDirectory() as tmp:
            binary = pathlib.Path(tmp) / "fake-resident"
            starts = pathlib.Path(tmp) / "starts"
            binary.write_text(
                f"#!{sys.executable}\n"
                "import json,sys\n"
                f"with open({str(starts)!r},'a') as f: f.write('start\\n')\n"
                "for line in sys.stdin:\n"
                " r=json.loads(line)\n"
                " print(json.dumps({'success':r['confirm'],'id':r['operationId']}),flush=True)\n"
            )
            binary.chmod(0o700)
            cmd = [sys.executable, str(DRIVER), "--binary", str(binary),
                   "--wallet-dir", tmp, "--instance-id-file", str(pathlib.Path(tmp) / "unused")]
            if execute:
                cmd.append("--execute")
            result = subprocess.run(cmd, input="".join(json.dumps(r) + "\n" for r in rows),
                                    text=True, capture_output=True, timeout=10, check=False)
            return result, starts.read_text().splitlines() if starts.exists() else []

    def test_persistent_child_checks_every_response(self):
        result, starts = self.run_driver([{"operationId": "a", "confirm": True},
                                         {"operationId": "b", "confirm": True}])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(starts, ["start"])
        self.assertEqual([json.loads(line)["id"] for line in result.stdout.splitlines()], ["a", "b"])

    def test_no_execute_no_process(self):
        result, starts = self.run_driver([], execute=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(starts, [])

    def test_failure_stops_without_retry_or_next_order(self):
        result, starts = self.run_driver([{"operationId": "a", "confirm": False},
                                         {"operationId": "b", "confirm": True}])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(starts, ["start"])
        self.assertEqual(len(result.stdout.splitlines()), 1)
        self.assertFalse(json.loads(result.stdout)["success"])


if __name__ == "__main__":
    unittest.main()
