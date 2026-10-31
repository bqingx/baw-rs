#!/usr/bin/env python3
"""Generate skill CLI references from help only. Never load a session or call a backend."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

EXTRA_ROOTS = ("vaults", "openfour", "fast")


def collect(binary, extra_roots=EXTRA_ROOTS):
    # clap help can echo env defaults: do not expose wallet selectors or credentials.
    env = {k: os.environ[k] for k in ("PATH", "HOME", "TMPDIR", "LANG") if k in os.environ}
    env["LC_ALL"] = "C"
    helps = {}
    leaves = []

    def visit(path):
        output = subprocess.run([str(binary), *path, "--help"], env=env,
                                capture_output=True, text=True, check=False, timeout=10)
        text = output.stdout
        # This snapshot prints valid openfour help but exits 1 (separate parser bug).
        help_exit_quirk = bool(path and path[0] == "openfour" and output.returncode == 1
                               and "Usage:" in text and "error:" not in text.lower())
        if output.returncode and not help_exit_quirk:
            raise ValueError(f"help failed: {path}, exit {output.returncode}")
        if not text.strip() or "Usage:" not in text:
            raise ValueError(f"missing help: {path}")
        helps[" ".join(path)] = text
        names = []
        if "Commands:\n" in text:
            block = text.split("Commands:\n", 1)[1].split("\nOptions:", 1)[0]
            for line in block.splitlines():
                match = re.match(r"^  ([a-z][a-z0-9-]*)\s", line)
                if match and match[1] != "help":
                    names.append(match[1])
        if not names and path:
            leaves.append(" ".join(path))
        for name in names:
            visit((*path, name))

    visit(())
    # These parsers are intentionally outside the root wallet CLI.
    for name in extra_roots:
        if name not in helps:
            visit((name,))
    version = subprocess.run([str(binary), "--version"], env=env, check=True,
                             capture_output=True, text=True, timeout=10).stdout.strip()
    return helps, sorted(set(leaves)), version


def generate(binary, output):
    helps, leaves, version = collect(binary)
    ref = output / "references"
    ref.mkdir(parents=True, exist_ok=True)
    for family in sorted({key.split()[0] for key in helps if key}):
        entries = [(key, text) for key, text in helps.items()
                   if key == family or key.startswith(family + " ")]
        chunks = [f"# {family}：实际 CLI 参数\n\n由 `scripts/build_skill_reference.py` 从 `{version}` 的 `--help` 生成。\n"
                  "这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。\n"
                  "普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。\n"]
        for key, text in entries:
            chunks.append(f"\n## `{key}`\n\n```text\n{text.rstrip()}\n```\n")
        (ref / f"cli-{family}.md").write_text("".join(chunks))
    (ref / "cli-global.md").write_text("# 普通钱包 CLI 全局参数\n\n```text\n" + helps[""] + "```\n")
    manifest = {"version": version, "source": "actual --help; no wallet/backend calls",
                "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "leafCount": len(leaves), "leaves": leaves,
                "families": sorted({c.split()[0] for c in leaves})}
    (ref / "commands.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    (ref / "command-index.md").write_text(
        "# 完整命令索引\n\n包含根帮助之外的 `vaults`、`openfour`、`fast`。\n\n"
        + f"快照版本 `{version}`；叶子命令 **{len(leaves)}** 条。版本号相同不代表功能相同，须核对命令帮助或二进制哈希。\n\n"
        + "\n".join(f"- `{cmd}` → [参数](cli-{cmd.split()[0]}.md)" for cmd in leaves) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=Path("skills/baw-rs"))
    args = parser.parse_args()
    manifest = generate(args.binary.resolve(), args.output)
    print(json.dumps({"version": manifest["version"], "leaves": manifest["leafCount"],
                      "families": manifest["families"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
