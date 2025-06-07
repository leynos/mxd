# Mermaid Diagram Validation

Mermaid diagrams are rendered client-side, so a small syntax error can leave a broken image in the documentation. To prevent this, every Markdown document that includes a `mermaid` code block must be checked before merging.

## Requirements

- **Node.js** must be available in your `PATH`.
- The validator uses `npx` to run the [`@mermaid-js/mermaid-cli`](https://github.com/mermaid-js/mermaid-cli) package. Installing it globally speeds things up:

```bash
npm install -g @mermaid-js/mermaid-cli
```

## Running the validator

From the repository root run the script with one or more Markdown paths:

```bash
./scripts/validate_mermaid.py docs/chat-schema.md docs/news-schema.md
```

Shell expansion lets you validate everything in `docs/` at once:

```bash
./scripts/validate_mermaid.py docs/*.md
```

The script extracts each ```mermaid` block and attempts to render it using
`mmdc`.
Any syntax errors will cause the script to exit with a non-zero status. The
failing diagram's line and a pointer to the error location are printed to help
you fix the issue.

If the required tools are missing the validator explains that Node.js and `@mermaid-js/mermaid-cli` must be installed.

Include this step in your workflow whenever you edit Markdown documentation containing Mermaid diagrams.
