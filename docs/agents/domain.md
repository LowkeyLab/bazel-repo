# Domain docs

This repository uses a multi-context layout.

## Before exploring

Read the root GLOSSARY-MAP.md when present, then the glossaries relevant
to the task. The map identifies domain contexts and their documentation
paths; a context may span multiple frontend and backend projects.

Read relevant system-wide ADRs in docs/adr/ and context-specific ADRs
at the locations identified by the map.

If these documents are absent, proceed silently. The domain-modeling
skill creates them lazily as terminology and decisions are resolved.

## Layout

- GLOSSARY-MAP.md: root index of contexts and documentation paths.
- docs/adr/: system-wide architectural decisions.
- <context-root>/GLOSSARY.md: context terminology.
- <context-root>/docs/adr/: context-specific architectural decisions.

Use existing project directories for context roots where appropriate.
Record actual paths in the map rather than assuming a src/ layout.

## Vocabulary

Use glossary terms in issue titles, proposals, hypotheses, and tests.
When a needed concept is missing, reconsider the terminology or note
the gap for domain-modeling.

## ADR conflicts

Explicitly identify any proposal that contradicts an existing ADR,
cite the ADR, and explain why the decision should be reconsidered.
