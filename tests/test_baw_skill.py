#!/usr/bin/env python3
"""Validate the baw-rs skill using help and offline build only; never touch a wallet."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
SKILL = ROOT / "skills" / "baw-rs"
BINARY = None
spec = importlib.util.spec_from_file_location("skill_refs", ROOT / "scripts/build_skill_reference.py")
assert spec is not None and spec.loader is not None
refs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(refs)


class SkillTests(unittest.TestCase):
    def test_frontmatter_and_portability(self):
        text = (SKILL / "SKILL.md").read_text()
        self.assertTrue(text.startswith("---\n"))
        head, body = text[4:].split("\n---\n", 1)
        for key in ("name", "description", "version", "author", "license", "platforms", "metadata"):
            self.assertRegex(head, rf"(?m)^{key}:")
        match = re.search(r'^description:\s*"(.*)"$', head, re.M)
        self.assertIsNotNone(match)
        assert match is not None
        description = match[1]
        self.assertLessEqual(len(description), 60)
        self.assertTrue(description.endswith("."))
        self.assertIn("bqing", head)
        self.assertIn("linux", head)
        for title in ("When to Use", "Prerequisites", "How to Run", "Procedure", "Pitfalls", "Verification"):
            self.assertIn("## " + title, body)
        for path in SKILL.rglob("*"):
            if path.is_file():
                self.assertLess(path.stat().st_size, 100000)
                self.assertNotIn("/root/", path.read_text())
                self.assertNotIn("SYNTHETIC-MUST-NOT-LEAK", path.read_text())

    def test_all_local_markdown_links_resolve(self):
        for path in SKILL.rglob("*.md"):
            for target in re.findall(r"\]\(([^)]+)\)", path.read_text()):
                if "://" in target or target.startswith("#"):
                    continue
                relative = target.split("#", 1)[0]
                self.assertTrue((path.parent / relative).is_file(), f"{path.name}: {target}")
                self.assertTrue((path.parent / relative).resolve().is_relative_to(SKILL.resolve()))

    def test_complete_command_snapshot_matches_binary(self):
        _, leaves, version = refs.collect(BINARY)
        manifest = json.loads((SKILL / "references/commands.json").read_text())
        self.assertEqual(leaves, manifest["leaves"])
        self.assertEqual(len(leaves), manifest["leafCount"])
        self.assertEqual(version, manifest["version"])
        for family in ("wallets", "task", "openfour", "approvals", "sign-message"):
            self.assertIn(family, manifest["families"])
        # `vaults` is a hidden alias of `wallets`, so it is not a family in help.
        self.assertNotIn("vaults", manifest["families"])
        for family in ("fast", "prediction", "defi", "tracker", "signal", "leaderboard"):
            self.assertNotIn(family, manifest["families"])
        comparison = json.loads((SKILL / "references/official-comparison.json").read_text())
        self.assertEqual(len(leaves), comparison["ours"])
        self.assertEqual(
            comparison["officialCount"] + len(comparison["extras"]),
            len(leaves) + len(comparison["missing"]),
        )
        # The trim removed these official families on purpose; they must be reported as missing.
        for family in ("defi", "prediction", "tracker", "signal", "leaderboard",
                       "limit-order", "x402-payment"):
            self.assertTrue(
                any(leaf.split()[0] == family for leaf in comparison["missing"]), family
            )
            self.assertFalse(
                any(leaf.split()[0] == family for leaf in comparison["extras"]), family
            )

    def test_generator_suppresses_sensitive_environment_defaults(self):
        keys = ("BINANCE_INSTANCE_ID", "BINANCE_BAW_DIR")
        old = {key: os.environ.get(key) for key in keys}
        try:
            for key in keys:
                os.environ[key] = "SYNTHETIC-MUST-NOT-LEAK"
            helps, _, _ = refs.collect(BINARY)
            self.assertNotIn("SYNTHETIC-MUST-NOT-LEAK", "".join(helps.values()))
        finally:
            for key, value in old.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_documented_offline_encode_matches_documented_intent(self):
        intent = {"core": "0x2222222222222222222222222222222222222222",
                  "token": "0x3333333333333333333333333333333333333333",
                  "from": "0x1111111111111111111111111111111111111111",
                  "budget": "100000000000000", "minAmountOut": "1000000000000000000"}
        env = {key: os.environ[key] for key in ("PATH", "TMPDIR", "LANG") if key in os.environ}
        with tempfile.TemporaryDirectory() as home:
            env["HOME"] = home
            out = subprocess.run([str(BINARY), "openfour", "encode-buy", "--core", intent["core"],
                                  "--token", intent["token"], "--wallet", intent["from"],
                                  "--amount-wei", intent["budget"], "--min-out-raw",
                                  intent["minAmountOut"]],
                                 env=env, capture_output=True, text=True, timeout=10, check=True)
            row = json.loads(out.stdout)
            self.assertTrue(row["success"])
            self.assertEqual(row["data"]["value"], intent["budget"])
            self.assertEqual(row["data"]["from"], intent["from"])
            self.assertTrue(row["data"]["inputData"].startswith("0x"))
            self.assertEqual(list(Path(home).rglob("session.json")), [])

    def test_skill_explains_rust_specific_safety_and_modes(self):
        text = (SKILL / "SKILL.md").read_text()
        for token in ("--instance-id-file", "--operation-id", "--accept-preview-sha256", "serve",
                      "vaults", "openfour encode-buy", "--json", "BROADCASTED", "--acknowledge-token-risk"):
            self.assertIn(token, text)
        modes = (SKILL / "references/wallet-modes.md").read_text()
        self.assertIn("--warm", modes)
        self.assertNotIn("fast serve", modes)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args, remaining = parser.parse_known_args()
    BINARY = args.binary.resolve()
    unittest.main(argv=[__file__, *remaining])
