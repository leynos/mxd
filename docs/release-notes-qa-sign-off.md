# Release-note quality assurance (QA) sign-off checklist

Use this checklist when preparing release notes for compatibility-related
changes. The checklist ensures roadmap acceptance criteria that require QA
sign-off references are auditable from repository history.

## Required compatibility references

- Link the release notes to `docs/internal-compatibility-matrix.md`.
- Confirm the matrix row(s) for affected client profiles were reviewed.
- Confirm test evidence listed in the matrix still matches current behaviour.
- Record any newly discovered deviations in both release notes and the matrix.

## Required QA sign-off statement

Include a line in release notes matching this format:

`QA sign-off: compatibility matrix reviewed at docs/internal-compatibility-matrix.md`

If the release introduces a new compatibility quirk, append:

`Deviation recorded in docs/internal-compatibility-matrix.md`

## Reviewer prompts

- Are supported clients and required toggles unchanged from the matrix?
- Are any behaviour changes still marked `Partial` or `Planned` in the matrix?
- Do release notes avoid claiming support that is not present in matrix status?
