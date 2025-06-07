#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import asyncio
import subprocess
import sys
import tempfile
from typing import List

RE = re.compile(
    r"^```\s*mermaid\s*\n(.*?)\n```[ \t]*$",
    re.DOTALL | re.MULTILINE,
)


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


async def render_block(
    block: str,
    tmpdir: Path,
    cfg_path: Path,
    path: Path,
    idx: int,
    semaphore: asyncio.Semaphore,
    timeout: float = 30.0,
) -> bool:
    """Render a single mermaid block using the CLI asynchronously."""
    mmd = tmpdir / f"{path.stem}_{idx}.mmd"
    svg = mmd.with_suffix(".svg")

    mmd.write_text(block)

    cmd = get_mmdc_cmd(mmd, svg, cfg_path)
    cli = cmd[0]

    async with semaphore:
        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
        except FileNotFoundError:
            print(
                f"Error: '{cli}' not found. Node.js with npx and @mermaid-js/mermaid-cli is required.",
                file=sys.stderr,
            )
            return False

        try:
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout)
        except asyncio.TimeoutError:
            proc.kill()
            await proc.wait()
            print(f"{path}: diagram {idx} timed out", file=sys.stderr)
            return False

    success = proc.returncode == 0
    if not success:
        print(f"{path}: diagram {idx} failed to render", file=sys.stderr)
        print(format_cli_error(stderr.decode('utf-8', errors='replace')), file=sys.stderr)

    return success


async def check_file(path: Path, cfg_path: Path, semaphore: asyncio.Semaphore) -> bool:
    blocks = parse_blocks(path.read_text(encoding="utf-8"))
    if not blocks:
        return True

    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_path = Path(tmpdir)
        tasks = [
            render_block(block, tmp_path, cfg_path, path, idx, semaphore)
            for idx, block in enumerate(blocks, 1)
        ]
        results = await asyncio.gather(*tasks, return_exceptions=True)
    return all(result is True for result in results)


async def main(paths, max_concurrent: int = 4):
    semaphore = asyncio.Semaphore(max_concurrent)
    with create_puppeteer_config() as cfg_path:
        tasks = [check_file(p, cfg_path, semaphore) for p in paths]
        results = await asyncio.gather(*tasks)
    return 0 if all(results) else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Validate Mermaid diagrams in Markdown files"
    )
    parser.add_argument(
        "paths",
        type=Path,
        nargs="+",
        help="Markdown files to validate",
    )
    parsed = parser.parse_args()
    sys.exit(asyncio.run(main(parsed.paths)))
