<!--
SYNC IMPACT REPORT
==================
Version change: 1.0.0 → 1.1.0
Bump rationale: MINOR — new principle added (IV. Design Fidelity to the Prototype) plus a
  corresponding Quality Gate and workflow item.

Modified principles: (none)
Added principles:
  - IV. Design Fidelity to the Prototype
Added sections: (none — Quality Gates and Development Workflow extended in place)
Removed sections: (none)

Templates requiring updates:
  - ✅ .specify/templates/plan-template.md — Constitution Check gate is generic; gates
       derived from this file at plan time (now includes the design-fidelity gate for
       UI-touching features).
  - ✅ .specify/templates/tasks-template.md — no structural change; UI features gain a
       validate-against-prototype step via the gate.
  - ✅ .specify/templates/spec-template.md — no change required.
  - ✅ README.md — references constitution generically; no change required.

Follow-up TODOs: (none)

Previous version (1.0.0) sync report: initial ratification — principles I–III, Quality
Gates, Development Workflow.
-->

# daedalus Constitution

## Core Principles

### I. Red/Green Test-Driven Development (NON-NEGOTIABLE)

Every behavioral change MUST follow the red-green-refactor cycle:

- Write a failing test that expresses the desired behavior FIRST.
- Run the test and confirm it FAILS for the expected reason (red).
- Write the minimum implementation to make the test PASS (green).
- Refactor only while tests stay green.

A change that adds or alters observable behavior without a preceding failing test is a
constitution violation. Bug fixes MUST begin with a regression test that reproduces the
bug (fails) before the fix is applied. Pure refactors (no behavioral change) are exempt
from the new-test requirement but MUST keep the existing suite green.

**Rationale**: Tests written after the fact tend to encode the implementation rather than
the requirement, and rarely demonstrate that they can fail. Writing the test first proves
the test is meaningful and that the code is driven by intent, not the reverse.

### II. Strict Linting — Warnings Are Errors

Each language in the codebase MUST have a linter and formatter configured, and they MUST
run in CI with warnings treated as errors:

- The build/CI fails on ANY lint warning or formatting deviation in every relevant
  language (e.g. `-Werror`-equivalent, `ruff`/`mypy --strict`, `clippy -D warnings`,
  `eslint --max-warnings 0`, etc.).
- Warnings MUST be fixed at the source, not suppressed. Per-line or per-file silencers
  (`# noqa`, `eslint-disable`, `#[allow(...)]`, `// nolint`, pragma suppressions) are
  prohibited unless suppression is genuinely the only correct option — in which case the
  silencer MUST be narrowly scoped and carry an inline comment justifying it.
- Global relaxation of lint rules to clear a backlog is forbidden; rules are tightened
  over time, never loosened to pass.

**Rationale**: A warning is a defect the tooling already found for free. Treating warnings
as errors keeps the signal at zero-noise; blanket silencers hide real problems and erode
the value of every other warning.

### III. Locally Testable Runtime Environment

Every feature MUST be runnable and verifiable on a developer's (or agent's) local machine
before it is considered complete:

- The repository MUST provide a documented, reproducible way to build, run, and exercise
  the change locally (script, devcontainer, compose file, Makefile target, or quickstart).
- Tests and linters MUST be runnable locally with a single, documented command — not only
  in CI.
- External dependencies MUST have a local substitute (fake, fixture, container, or
  emulator) so the change can be exercised without privileged or remote access.
- When a change cannot yet be exercised locally, providing that capability is part of the
  task's scope, not a follow-up.

**Rationale**: Fast, local feedback is what makes red/green TDD and strict linting
practical. An agent or developer who cannot run the code locally cannot honestly verify it
works, and CI becomes the only — far slower — feedback loop.

### IV. Design Fidelity to the Prototype

The HTML-based design prototypes in `design/` (the runnable `design/prototype/` sources and
their exports) are the **visual and interaction source of truth** for the operator UI.
Any design for production UI code MUST be validated against them:

- Before implementing a screen, component, or state, the implementation design (view
  model, layout, tokens, behavior) MUST be checked against the corresponding prototype
  screen/state — including the cross-cutting states (empty, loading, error, degraded,
  blocked-on-operator) and both themes and platform skins, as catalogued in
  `design/README.md` and reproduced via `design/storyboards.md`.
- Deviations from the prototype MUST be deliberate and recorded (in the plan or the
  design/README mapping), never silent drift; unresolved conflicts escalate to a design
  update first, not an implementation-side improvisation.
- New UI work with no prototype counterpart MUST get a prototype (or an explicit recorded
  exemption) before production implementation begins.
- Verification of a UI change includes comparing the running native UI against the
  prototype rendering of the same screen and state.

**Rationale**: The prototype is where design decisions are made, reviewed, and kept
coherent (tokens, status palette, accessibility). Validating implementation designs
against it keeps the native GPUI port faithful and prevents the design system from
forking between artifact and product.

## Quality Gates

These gates are enforced on every change before merge:

- **Tests green**: the full suite passes locally and in CI; new behavior is covered by a
  test that was demonstrated to fail first.
- **Lint clean**: every configured linter/formatter passes with zero warnings; any
  suppression is narrowly scoped and justified inline.
- **Locally exercised**: the change has been run and observed locally via the documented
  runtime path; the quickstart/run instructions are updated when they change.
- **No silent scope-narrowing**: skipped tests, disabled lint rules, or stubbed runtime
  paths are called out explicitly in the change description, never slipped in quietly.
- **Design validated against the prototype**: for UI-touching changes, the implementation
  design was checked against the `design/` HTML prototype for the same screens and states,
  and any deviation is recorded (Principle IV).

## Development Workflow

- Spec-driven flow: features progress spec → plan → tasks → implementation under
  `.specify/`. Each stage's Constitution Check MUST pass before advancing.
- The `/speckit.plan` Constitution Check gate MUST verify the three core principles are
  satisfiable for the feature (test strategy, lint coverage for each language touched, and
  a local runtime path) before Phase 0 research and again after Phase 1 design.
- Task lists MUST include, where applicable: linter/formatter configuration, a local
  runtime/quickstart path, and tests written before their implementation tasks.
- UI-touching plans and tasks MUST name the prototype screens/states they implement and
  include a validate-against-prototype step (Principle IV).
- Code review MUST confirm constitution compliance; a reviewer rejects changes that add
  behavior without a prior failing test, carry unjustified lint suppressions, cannot be
  run locally, or implement UI that was not validated against the design prototype.

## Governance

This constitution supersedes other development practices in this repository. When a
practice and this document conflict, this document wins.

- **Amendments**: proposed via pull request that edits this file, states the rationale,
  and updates the version and dates below. Amendments require maintainer approval.
- **Versioning policy** (semantic):
  - MAJOR — backward-incompatible governance changes, or removal/redefinition of a
    principle.
  - MINOR — a new principle or section, or materially expanded guidance.
  - PATCH — clarifications, wording, and non-semantic refinements.
- **Compliance review**: every PR and review MUST verify compliance with the Core
  Principles and Quality Gates. Justified, non-negotiable exceptions MUST be documented in
  the change (e.g. the plan's Complexity Tracking table) with the simpler alternative and
  why it was rejected.
- **Runtime guidance**: agent and contributor runtime guidance lives in `README.md` and
  feature `quickstart.md` files; these MUST stay consistent with Principle III.

**Version**: 1.1.0 | **Ratified**: 2026-06-06 | **Last Amended**: 2026-07-06
