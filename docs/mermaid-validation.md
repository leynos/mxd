# Mermaid Diagram Validation

Mermaid diagrams are rendered client-side, so a small syntax error can leave a broken image in the documentation. To prevent this, every Markdown document that includes a `mermaid` code block must be checked before merging.

## Requirements

- **Node.js** must be available in your PATH.
- The Mermaid CLI package:

```bash
npm install -g @mermaid-js/mermaid-cli
```

## Running the validator

From the repository root run:

```bash
./scripts/validate_mermaid.py path/to/file.md
```

If no file arguments are given the script scans all `*.md` files under `docs/`.
It extracts each ```mermaid` block and attempts to render it using `mmdc`.
Any syntax errors will cause the script to exit with a non-zero status and print which diagram failed along with the CLI error output.

If `npx` or `mmdc` cannot be found the validator will print a message prompting you to install Node.js and `@mermaid-js/mermaid-cli`.

Include this step in your workflow whenever you edit Markdown documentation containing Mermaid diagrams.
