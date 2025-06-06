#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path
import tempfile
import os
import json
from typing import List

RE = re.compile(r"```mermaid\s*\n(.*?)```", re.DOTALL)


def parse_blocks(text: str) -> List[str]:
    """Return all mermaid code blocks found in the text."""
    return RE.findall(text)


def create_puppeteer_config() -> Path:
    """Write a minimal Puppeteer config disabling sandboxing."""
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
        json.dump({"args": ["--no-sandbox"]}, fh)
        fh.flush()
        return Path(fh.name)


def render_block(block: str, cfg_path: Path, path: Path, idx: int) -> bool:
    """Render a single mermaid block using the CLI."""
    with tempfile.NamedTemporaryFile("w", suffix=".mmd", delete=False) as fh:
        fh.write(block)
        fh.flush()
        temp = Path(fh.name)

    try:
        proc = subprocess.run(
            [
                "npx",
                "-y",
                "mmdc",
                "-p",
                str(cfg_path),
                "-i",
                str(temp),
                "-o",
                str(temp) + ".svg",
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

    success = proc.returncode == 0
    if not success:
        print(f"{path}: diagram {idx} failed to render", file=sys.stderr)
        # Surface the CLI error output to help diagnose syntax problems
        print(proc.stderr, file=sys.stderr)

    for ext in ("", ".svg"):
        try:
            os.remove(str(temp) + ext)
        except OSError:
            pass

    return success


def check_file(path: Path) -> bool:
    blocks = parse_blocks(path.read_text())
    if not blocks:
        return True

    cfg_path = create_puppeteer_config()
    ok = True

    for idx, block in enumerate(blocks, 1):
        if not render_block(block, cfg_path, path, idx):
            ok = False

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
