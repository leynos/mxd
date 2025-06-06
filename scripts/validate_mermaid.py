#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path
import tempfile
import os
import json

RE = re.compile(r"```mermaid\n(.*?)\n```", re.DOTALL)


def check_file(path: Path) -> bool:
    text = path.read_text()
    blocks = RE.findall(text)
    if not blocks:
        return True
    ok = True
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as cfh:
        json.dump({"args": ["--no-sandbox"]}, cfh)
        cfh.flush()
        cfg_path = cfh.name
    for idx, block in enumerate(blocks, 1):
        with tempfile.NamedTemporaryFile("w", suffix=".mmd", delete=False) as fh:
            fh.write(block)
            fh.flush()
            temp = fh.name
        try:
            proc = subprocess.run(
                [
                    "npx",
                    "-y",
                    "mmdc",
                    "-p",
                    cfg_path,
                    "-i",
                    temp,
                    "-o",
                    temp + ".svg",
                ],
                capture_output=True,
                text=True,
            )
        except FileNotFoundError:
            print(
                "Error: 'npx' or the mermaid CLI is not installed.\n"
                "Install Node.js and @mermaid-js/mermaid-cli to enable diagram validation.",
                file=sys.stderr,
            )
            return False
        if proc.returncode != 0:
            print(f"{path}: diagram {idx} failed to render", file=sys.stderr)
            if proc.stderr:
                print(proc.stderr, file=sys.stderr)
            ok = False
        for ext in ("", ".svg"):
            try:
                os.remove(temp + ext)
            except OSError:
                pass
    try:
        os.remove(cfg_path)
    except OSError:
        pass
    return ok


def main(paths):
    ok = True
    for p in paths:
        if not check_file(p):
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    doc_paths = [Path(p) for p in sys.argv[1:]] or list(Path("docs").glob("*.md"))
    sys.exit(main(doc_paths))
