# zobrist

Personal, ChessBase-like chess database in Rust. One maintainer, no other users, public only so CI is free. `ARCHITECTURE.md` is the map and its invariants are binding. The gate enforces the dependency direction and document paths. Review checks the invariants. Everything in the repository is in English, whatever language the conversation uses.

## Gate

`scripts/check.sh --all` is the whole gate, the same locally and in CI. Run it before you report any work as done and quote its last lines as evidence. Fix the cause, never the gate.

## Rules the gate cannot check

These are traps, not a map. The lints in `Cargo.toml` and `clippy.toml` own everything a lint can own. Do not repeat them here.

- No `utils`, `helpers`, or `common` modules. No abstraction with a single caller. Prefer adding to an existing file over creating a small new one. A new file is a new logical unit.
- Full words in names. No `q` for queue, no `cfg` for config.
- A bug fix starts with a failing test. Run it, see it fail, and only then edit the implementation. Never write the test and the fix in the same edit. After the fix, run it again.
- For a mechanical rewrite (rename, mass edit), run a tool (rustfmt, a script, `cargo fix`) and review the diff. Never type it by hand file by file.
- Past the boundary parsers, handle only the cases the spec names. For anything else, ask first.
- Tests are the specification. Call code the way users do and assert literal expected values.
- Comments only for a non-obvious why.
- Verify the base of every delegated worktree. In the brief, name the commit that the agent must start from, and have the agent print `git rev-parse HEAD` before it edits anything. Claude Code creates a worktree from the `main` that the remote had at the last fetch, not from your branch, and a stale base wastes the work.
- No time estimates anywhere.

## Workflow

- The backlog is GitHub issues: `idea` is a wish, `spec` is a step ready to build, `bug` is a defect. Issue forms in `.github/ISSUE_TEMPLATE/` define the fields, and GitHub renders each as a `### Label` heading. Read with `gh issue view N --json title,body,labels`. Create with `gh issue create --label spec --body-file FILE`, where FILE uses the form's labels as `###` headings in order.
- Spec before code. A `feat` or `fix` PR implements one issue and closes it. A PR of another type needs an issue only when one defines its scope.
- The body of a PR uses the sections in `.github/pull_request_template.md`. Keep `Why`, `Scope`, and `Verification`, and delete any other section that has nothing to say. The body becomes the squash commit body, so keep it under about 40 lines.
- `main` is the only long-lived branch. Branch from `main` and open the PR against it. When the checks are green and the Review section allows it, squash-merge the PR. Releases are tags on `main`.
- Small PRs, one concern each, Conventional Commit titles. If review follow-ups grow the diff by more than half or add new state, stop and ask.
- A decision that is hard to reverse gets an entry in the Decisions section of `ARCHITECTURE.md`. Update `ARCHITECTURE.md` when a crate or invariant changes. After a handful of feature PRs comes one that only simplifies.

## Review

Review exists to catch a defect before the merge. Zero comments is not the goal.

- A `feat`, `fix`, `refactor`, or `perf` PR gets one Codex round. When the checks are green, comment `@codex review` once. A PR of any other type merges on green checks without a round.
- Sort every finding. Fix a defect that the diff introduced. Dismiss style, taste, and anything the gate already checks, with a one-line reason in the thread. File a defect that the diff did not introduce as a `bug` issue. Ask the maintainer about a security or data-loss finding that you cannot settle. Resolve every thread before the merge.
- Request a second round only if the first round led to a code fix. Never request a third. If the second round finds another defect, stop and ask.
- A PR that touches the gate waits for the maintainer to merge it. The gate is `scripts/`, `.github/`, `clippy.toml`, `deny.toml`, `rust-toolchain.toml`, `.config/nextest.toml`, the lints in `Cargo.toml`, and this file. A PR that deletes a test or weakens an assertion waits too.

## Code Review Rules

These rules are for the reviewer. Report a defect that the diff introduces. Leave formatting, naming, and lint findings to the gate.

- The invariants in `ARCHITECTURE.md` are binding. A diff that breaks one is a defect.
- A diff that adds a crate, changes a dependency edge, or changes an invariant is a defect unless it also changes `ARCHITECTURE.md`.
- A diff that deletes a test, weakens an assertion, or loosens the gate is a defect unless the PR body gives the reason.

## Rules hygiene

Every agent session reads this file, so every line costs attention. Before you propose a rule, try to make the mistake impossible: by structure or types first, then by a check in the gate whose error names the fix, then by a test. A sentence here is the last resort.

A new rule must be non-obvious, concrete enough to act on, and backed by an incident. Two incidents are enough. One is enough when the mistake is silent and costly. Write the reason into the rule. Clarifying an existing rule is always welcome.

Delete a rule in the PR that makes a check enforce it. The PR that only simplifies also removes every rule that no longer changes what an agent does.

Propose a rule under a "Suggested AGENTS.md additions" heading in the PR body. Never edit this file during feature or fix work. A rule for one crate goes in that crate's own `AGENTS.md`. No architecture descriptions here.

## Boundaries

Agents create issues, branches, commits, and PRs against `main`, and squash-merge a PR when the Review section allows it. Agents never push to `main` directly, never enable auto-merge, never merge with `--admin`, never force-push, never change repository settings or rules, never create remote repos, and never touch releases or secrets. A decision that is hard to reverse gets a yes or no from the maintainer before the code. No attribution trailers.
