#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path
import tempfile
import os

RE = re.compile(r"```mermaid\n(.*?)\n```", re.DOTALL)


def check_file(path: Path) -> bool:
    text = path.read_text()
    blocks = RE.findall(text)
    if not blocks:
        return True
    ok = True
    for idx, block in enumerate(blocks, 1):
        with tempfile.NamedTemporaryFile("w", suffix=".mmd", delete=False) as fh:
            fh.write(block)
            temp = fh.name
        try:
            subprocess.run(
                ["npx", "-y", "mmdc", "-i", temp, "-o", temp + ".svg"],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except subprocess.CalledProcessError:
            print(f"{path}: diagram {idx} failed to render", file=sys.stderr)
            ok = False
        finally:
            for ext in ("", ".svg"):
                try:
                    os.remove(temp + ext)
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
