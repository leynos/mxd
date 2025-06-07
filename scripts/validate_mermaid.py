#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path
import tempfile
import os
import json
import shutil
from typing import List

def get_mmdc_cmd(mmd: Path, svg: Path, cfg_path: Path) -> List[str]:
    """Return the command to run mermaid-cli."""
    cli = "mmdc" if shutil.which("mmdc") else "npx"
    cmd = [cli]
    if cli == "npx":
        cmd += ["--yes", "@mermaid-js/mermaid-cli", "mmdc"]
    cmd += ["-p", str(cfg_path), "-i", str(mmd), "-o", str(svg)]
    return cmd


def render_block(block: str, tmpdir: Path, cfg_path: Path, path: Path, idx: int) -> bool:
    mmd = tmpdir / f"{path.stem}_{idx}.mmd"
    svg = mmd.with_suffix(".svg")

    mmd.write_text(block)
    cmd = get_mmdc_cmd(mmd, svg, cfg_path)
    cli = cmd[0]

        proc = subprocess.run(cmd, capture_output=True, text=True)
            f"Error: '{cli}' not found. Node.js with npx and @mermaid-js/mermaid-cli is required.",
        ok = True
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            for idx, block in enumerate(blocks, 1):
                if not render_block(block, tmp_path, cfg_path, path, idx):
                    ok = False
        return ok
    finally:
        try:
            os.remove(cfg_path)
        except OSError:
            pass
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
