<!--
Sync Impact Report
Version change: 1.0.0 -> 1.1.0
Modified principles:
- III. Boundary-Oriented Rust Design -> III. Boundary-Oriented Rust Design
- V. Performance Budget Discipline -> V. Performance and Allocation Budget Discipline
Added sections:
- None
Removed sections:
- None
Templates requiring updates:
- ✅ .specify/templates/plan-template.md
- ✅ .specify/templates/spec-template.md
- ✅ .specify/templates/tasks-template.md
- ⚠ pending .specify/templates/commands/*.md (directory not present in this repository)
Follow-up TODOs:
- None
-->
# CPU Game Constitution

## Core Principles

### I. Server-Authoritative Simulation
All gameplay state that can affect fairness, scoring, combat, movement, or
spawn outcomes MUST be decided by the authoritative simulation, not by the
renderer or an individual client. Client code MAY predict or interpolate for
responsiveness, but the server-side tick remains the source of truth and MUST
be able to overwrite divergent local state cleanly. Rationale: the project is a
multiplayer shooter, so trust boundaries must protect competitive integrity
before convenience.

### II. Deterministic Gameplay Boundaries
Gameplay rules MUST be expressed as deterministic, side-effect-contained logic
that can be executed from explicit inputs and a known world snapshot. Changes to
movement, weapons, hit detection, spawning, or bots MUST document what inputs
they consume, which state they mutate, and how they behave under replay,
rollback, or packet delay. Rationale: deterministic slices are the only cheap
way to debug desyncs and keep local simulation, bots, and authoritative updates
aligned.

### III. Boundary-Oriented Rust Design
New work MUST preserve clear boundaries between engine logic, networking,
presentation, and platform wiring. Core rules belong in reusable Rust modules 
with narrow data contracts; rendering code MUST consume prepared state
instead of hiding game decisions inside GPU or windowing layers. Unsafe code,
global mutable state, and feature leakage across crate boundaries MUST be
justified in the plan's Complexity Tracking section. Logic that can be modeled
as pure transforms over explicit inputs SHOULD prefer that functional style,
with mutation isolated to narrow commit points or boundary adapters when it
improves readability and testability. Rationale: the project already separates
engine-core, presentation, and app layers, and that separation is required to
keep iteration fast while making state transitions easy to inspect.

### IV. Tests Before Trust
Every change to authoritative simulation, serialization, presentation contracts,
or bug-prone math MUST add or update automated tests at the cheapest meaningful
level. Pure simulation behavior SHOULD be covered with unit or property tests;
cross-boundary behavior such as client/server state flow or render request
validation MUST use focused integration tests. Manual playtesting is required
before merge for player-facing behavior, but it never replaces executable tests.
Rationale: multiplayer and rendering regressions are expensive to diagnose after
the fact and need fast local checks.

### V. Performance and Allocation Budget Discipline
The game MUST protect a smooth play loop on target desktop hardware by treating
frame time, simulation tick cost, and memory churn as first-class constraints.
Features that add per-frame allocation, extra full-screen passes, redundant data
copies, unbounded entity scans, or avoidable alloc/drop churn in hot paths MUST
include a concrete budget and a cheaper alternative considered in planning. Hot
loops SHOULD reuse buffers, caches, and scratch state where practical rather
than recreating owned data each tick or frame. Any change that risks frame
pacing, authoritative tick cadence, or presenter throughput MUST be measured
before it is accepted. Rationale: a shooter that is correct but stutters or
misses ticks is not shippable.

## Engineering Constraints

The implementation stack is Rust 2024 with wgpu, winit, image, glam, and local
workspace crates for engine and presentation. New gameplay systems MUST prefer
data-oriented structs and explicit message types over hidden callbacks. Public
contracts between crates or networking layers MUST remain serializable or be
designed so serialization can be added without redesign. Where it clarifies the
code, transformation-heavy logic SHOULD be expressed with pure helpers,
iterators, and other functional patterns before state is committed to shared
structures. Logs and diagnostics for simulation, networking, and presentation
failures SHOULD be structured enough to reconstruct the failing tick, player,
boundary crossed, and any measured hot-path allocation regressions.

## Delivery Workflow

Every feature spec MUST identify the authoritative owner of the behavior, the
inputs consumed per tick or event, the tests needed to prove correctness, and
the performance risks if the change lands. Implementation plans MUST fail the
Constitution Check if they do not define determinism expectations, boundary
ownership, validation coverage, hot-path allocation strategy, and a measurement
strategy for risky render or simulation work. Task breakdowns MUST schedule the
narrow validation work close to the implementation that introduces risk rather
than deferring all verification to a final polish pass.

## Governance

This constitution overrides informal project habits for all future planning,
specification, and implementation work. Amendments require a written rationale,
an explicit semantic version bump, and updates to any affected templates or
workflow guidance in the same change. Versioning policy is mandatory: MAJOR for
removing or redefining a core principle in a backward-incompatible way, MINOR
for adding a principle or materially expanding governance, and PATCH for
clarifications that do not change engineering obligations. Every review and plan
approval MUST include a compliance check against these principles, and any
approved exception MUST be recorded in the relevant plan or task artifact.

**Version**: 1.1.0 | **Ratified**: 2026-05-06 | **Last Amended**: 2026-05-13
