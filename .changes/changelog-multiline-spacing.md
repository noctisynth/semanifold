---
semifold: patch:fix
semifold-changelog: patch:fix
---

Render multiline changeset entries as valid nested Markdown paragraphs

Blank lines in a changeset summary now separate content without producing whitespace-only
paragraphs. Every non-empty continuation line is emitted after a blank line with four spaces of
indentation, and regression coverage uses the same front matter and prose shape as real changesets.
