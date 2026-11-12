# Architecture decision records

One file per decision that is hard to reverse. Language, license, storage format, UI toolkit, a new dependency of `core`. Anything a single PR can undo without pain gets no ADR; the PR description is enough.

An ADR describes a moment, not the current state. It is written when the decision is made and never edited after acceptance. When the decision changes, a new record supersedes the old one, which stays in place with its status changed to `superseded by ADR-NNNN`. Research notes stay outside the repository; the record must be readable without following any link.

## Format

The MADR minimal template (https://adr.github.io/madr/) plus three of its optional sections. Twenty to forty lines.

```markdown
---
status: accepted
date: YYYY-MM-DD
---

# Short title in the imperative, as in "Use X for Y"

## Context and Problem Statement

The question, in two or three sentences, with the facts that force it.

## Considered Options

* Option A
* Option B

## Decision Outcome

Chosen option: "Option A", because <the reasons that decided it>. <One clause per rejected option on why it lost.>

### Consequences

* Good, because ...
* Bad, because ...

### Confirmation

The mechanism that keeps the decision enforced, usually a check in the gate.

## More Information

Revisit if <the condition under which the decision no longer holds>.

* Source with date
```

## Naming

Files are named NNNN-short-title.md, with a four-digit, zero-padded number that increases by one per record and is never reused. The directory listing is the index.
