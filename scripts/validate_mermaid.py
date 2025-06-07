#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path
import tempfile
import os
import json
import shutil
from typing import List, Optional

RE = re.compile(r"```mermaid\n(.*?)\n```", re.DOTALL)


def parse_blocks(text: str) -> List[str]:
    """Return all mermaid code blocks found in the text."""
    return RE.findall(text)


from contextlib import contextmanager


@contextmanager
def create_puppeteer_config() -> Path:
    """Yield a Puppeteer config path and remove it on exit."""
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
        json.dump({"args": ["--no-sandbox"]}, fh)
        fh.flush()
        name = fh.name
    path = Path(name)
    try:
        yield path
    finally:
        try:
            os.remove(path)
        except OSError:
            pass


def get_mmdc_cmd(mmd: Path, svg: Path, cfg_path: Path) -> List[str]:
    """Return the command to run mermaid-cli."""
    cli = "mmdc" if shutil.which("mmdc") else "npx"
    cmd = [cli]
    if cli == "npx":
        cmd += ["--yes", "@mermaid-js/mermaid-cli", "mmdc"]
    cmd += ["-p", str(cfg_path), "-i", str(mmd), "-o", str(svg)]
    return cmd


def format_cli_error(stderr: str) -> str:
    """Extract a concise parse error message from mmdc output."""
    lines = stderr.splitlines()
    for i, line in enumerate(lines):
        m = re.search(r"Parse error on line (\d+):", line)
        if m and i + 2 < len(lines):
            snippet = lines[i + 1]
            pointer = lines[i + 2]
            detail = lines[i + 3] if i + 3 < len(lines) else ""
            return f"Parse error on line {m.group(1)}:\n{snippet}\n{pointer}\n{detail}"
    return stderr.strip()


def render_block(block: str, tmpdir: Path, cfg_path: Path, path: Path, idx: int) -> bool:
    """Render a single mermaid block using the CLI."""
    mmd = tmpdir / f"{path.stem}_{idx}.mmd"
    svg = mmd.with_suffix(".svg")

    mmd.write_text(block)

    cmd = get_mmdc_cmd(mmd, svg, cfg_path)
    cli = cmd[0]

    try:
        proc = subprocess.run(cmd, capture_output=True, text=True)
    except FileNotFoundError:
        print(
            f"Error: '{cli}' not found. Node.js with npx and @mermaid-js/mermaid-cli is required.",
            file=sys.stderr,
        )
        return False

    success = proc.returncode == 0
    if not success:
        print(f"{path}: diagram {idx} failed to render", file=sys.stderr)
        print(format_cli_error(proc.stderr), file=sys.stderr)

    return success


def check_file(path: Path) -> bool:
    blocks = parse_blocks(path.read_text())
    if not blocks:
        return True

    ok = True
    with create_puppeteer_config() as cfg_path:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            for idx, block in enumerate(blocks, 1):
                if not render_block(block, tmp_path, cfg_path, path, idx):
                    ok = False
    return ok


def main(paths):
    ok = True
    for p in paths:
        if not check_file(p):
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    args = sys.argv[1:]
    doc_paths = (
        [Path(p) for p in args]
        if args
        else list(Path("docs").glob("*.md"))
    )
    sys.exit(main(doc_paths))
