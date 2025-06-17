# Mermaid Diagram Validation

Mermaid diagrams are rendered client-side, so a small syntax error can leave a
broken image in the documentation. To prevent this, every Markdown document that
includes a `mermaid` code block must be checked before merging.

## Requirements

- Install the `nixie` CLI and ensure it is available on your `PATH`.

## Running the validator

From the repository root run `nixie` with one or more Markdown paths:

```bash
nixie docs/chat-schema.md docs/news-schema.md
```

You can pass directories and the validator will search for Markdown files
recursively. Shell expansion lets you validate everything in `docs/` at once:

```bash
nixie docs/*.md
```

For machines with many cores, increasing the concurrency can speed up
validation. By default the script uses the number of CPU cores detected on your
system. Use the `--concurrency` flag to override the number of diagrams rendered
in parallel:

```bash
nixie --concurrency 8 docs/*.md
```

The script extracts each
\`\`\`mermaid`block and attempts to render it using`mmdc\`. Any syntax errors
will cause the script to exit with a non-zero status. The failing diagram's line
and a pointer to the error location are printed to help you fix the issue.

If the required tools are missing the validator explains that Node.js and
`@mermaid-js/mermaid-cli` must be installed.

Include this step in your workflow whenever you edit Markdown documentation
containing Mermaid diagrams.
