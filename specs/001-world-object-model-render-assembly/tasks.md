# Tasks: World Object Model And Render Assembly

**Input**: Design notes from `/objectplan.md`
**Prerequisites**: Current gameplay and rendering flow in `src/simulation.rs`, `src/app.rs`, `src/gpu_renderer.rs`, `src/renderer.rs`

**Tests**: No dedicated test-first work is included because the feature definition did not request TDD. Validation is handled by story checkpoints and final compile/smoke verification.

**Organization**: Tasks are grouped by user story so each slice can be implemented and verified independently.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Introduce the shared module seams and render-facing vocabulary needed by the rest of the migration.

- [ ] T001 Add the `render_assembly` module declaration and startup wiring in `src/main.rs`
- [ ] T002 [P] Define render-facing visual identifiers and animation metadata in `src/texture.rs`
- [ ] T003 [P] Replace the old sprite-only shared model surface with object/render shared types in `src/model.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Move the authoritative simulation from player-derived sprites to an object-backed world state.

**⚠️ CRITICAL**: No user story work should begin until this phase is complete.

- [ ] T004 Refactor `GameState` and `PlayerState` to use object-backed actor ownership in `src/simulation.rs`
- [ ] T005 Extend the simulation update loop with object allocation and shared spawn helpers in `src/simulation.rs`
- [ ] T006 [P] Create and remove player-controlled actor objects during client lifecycle events in `src/net/server.rs`
- [ ] T007 [P] Update client prediction reconciliation to replay object-backed state in `src/net/client.rs`
- [ ] T008 [P] Update waypoint bot steering to read its controlled actor object instead of player position fields in `src/net/bot.rs`

**Checkpoint**: The authoritative state owns world objects, players point at controlled objects, and networking/prediction still function.

---

## Phase 3: User Story 1 - Render actors from world objects (Priority: P1) 🎯 MVP

**Goal**: Preserve the current player-avatar rendering path while replacing derived sprites with a render assembly step over world objects.

**Independent Test**: Start the game, confirm the human and bot avatars still render, animate, and occlude correctly while the camera follows the human player's controlled object.

### Implementation for User Story 1

- [ ] T009 [US1] Define `RenderScene`, `RenderCamera`, and `RenderBillboard` assembly outputs in `src/render_assembly.rs`
- [ ] T010 [US1] Assemble actor billboards and viewer camera state from `GameState.objects` in `src/render_assembly.rs`
- [ ] T011 [US1] Replace direct `players` and `sprites` render reads with render-assembly output in `src/app.rs`
- [ ] T012 [P] [US1] Update the GPU scene renderer to consume `RenderCamera` and `RenderBillboard` data in `src/gpu_renderer.rs`
- [ ] T013 [P] [US1] Update the CPU raycast renderer to consume `RenderCamera` and `RenderBillboard` data in `src/renderer.rs`
- [ ] T014 [US1] Remove `Sprite` and the per-tick sprite rebuild path once render assembly owns billboard generation in `src/model.rs`
- [ ] T015 [US1] Remove sprite regeneration from authoritative simulation updates in `src/simulation.rs`

**Checkpoint**: Player avatars are world objects, rendering is driven by `RenderScene`, and no gameplay code rebuilds a sprite list from players.

---

## Phase 4: User Story 2 - Add static props and pickups as first-class objects (Priority: P2)

**Goal**: Allow non-player world objects to exist independently, render through the same assembly path, and affect movement or collection behavior.

**Independent Test**: Seed at least one static prop and one pickup, verify the prop blocks movement, the pickup renders independently of players, and the pickup disappears when collected.

### Implementation for User Story 2

- [ ] T016 [US2] Add `StaticProp` and `Pickup` object variants plus shared spawn helpers in `src/simulation.rs`
- [ ] T017 [P] [US2] Map static props and pickups to billboards and animation behavior in `src/render_assembly.rs`
- [ ] T018 [US2] Extend visual lookup for prop and pickup object types in `src/texture.rs`
- [ ] T019 [US2] Seed initial static props and pickups into the startup world state in `src/main.rs`
- [ ] T020 [US2] Apply movement blocking and pickup collection/removal rules for object interactions in `src/simulation.rs`

**Checkpoint**: Non-player objects can be spawned, rendered, collided with, and removed without any player-specific render code.

---

## Phase 5: User Story 3 - Add projectiles as actor-independent world objects (Priority: P3)

**Goal**: Represent transient moving objects in the same world model so projectiles can spawn, travel, collide, and render without special renderer paths.

**Independent Test**: Trigger projectile spawning in-game, confirm projectiles render while moving, despawn on impact or TTL expiry, and do not rely on player-derived sprite generation.

### Implementation for User Story 3

- [ ] T021 [US3] Add projectile state, TTL, damage, and shared projectile spawn helpers in `src/simulation.rs`
- [ ] T022 [US3] Add a fire intent to player input messages in `src/input.rs`
- [ ] T023 [US3] Emit projectile fire input from local controls in `src/app.rs`
- [ ] T024 [US3] Advance projectiles and resolve wall or actor collisions in `src/simulation.rs`
- [ ] T025 [P] [US3] Assemble projectile billboards and motion-facing behavior in `src/render_assembly.rs`
- [ ] T026 [P] [US3] Add projectile visual lookup coverage in `src/texture.rs`

**Checkpoint**: Projectiles are ordinary world objects with simulation and rendering handled by the shared object pipeline.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Remove obsolete paths, document the new architecture, and verify the migration end to end.

- [ ] T027 [P] Update the feature architecture notes to reflect the final object pipeline in `objectplan.md`
- [ ] T028 Clean up obsolete sprite-specific naming and dead code across `src/app.rs`, `src/gpu_renderer.rs`, `src/model.rs`, `src/renderer.rs`, and `src/simulation.rs`
- [ ] T029 Validate the end-to-end migration by fixing any remaining integration issues surfaced while building from `src/main.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup**: No dependencies.
- **Phase 2: Foundational**: Depends on Phase 1 and blocks every user story.
- **Phase 3: User Story 1**: Depends on Phase 2 and is the MVP.
- **Phase 4: User Story 2**: Depends on Phase 2 and should build on the render assembly seam from US1.
- **Phase 5: User Story 3**: Depends on Phase 2 and reuses the object pipeline built in US1.
- **Phase 6: Polish**: Depends on the selected user stories being complete.

### User Story Dependencies

- **US1**: No dependency on later stories; it establishes the render assembly seam.
- **US2**: Depends on US1's render assembly path to render non-player objects cleanly.
- **US3**: Depends on US1's object-backed rendering path and benefits from US2's object interaction patterns, but can start once US1 is stable.

### Within Each User Story

- Shared types and assembly outputs come before renderer cutovers.
- App integration happens after render assembly can build a complete scene.
- Simulation cleanup happens only after both renderers no longer depend on `Sprite`.
- Object interaction rules follow object variant and spawn helper work.

### Parallel Opportunities

- `T002` and `T003` can run in parallel after module wiring begins.
- `T006`, `T007`, and `T008` can run in parallel after the core `GameState` refactor lands.
- `T012` and `T013` can run in parallel once `RenderScene` is defined.
- `T017` and `T018` can run in parallel after `StaticProp` and `Pickup` variants exist.
- `T025` and `T026` can run in parallel after projectile simulation state exists.

---

## Parallel Example: User Story 1

```text
Task: T012 Update the GPU scene renderer to consume RenderCamera and RenderBillboard data in src/gpu_renderer.rs
Task: T013 Update the CPU raycast renderer to consume RenderCamera and RenderBillboard data in src/renderer.rs
```

## Parallel Example: User Story 2

```text
Task: T017 Map static props and pickups to billboards and animation behavior in src/render_assembly.rs
Task: T018 Extend visual lookup for prop and pickup object types in src/texture.rs
```

## Parallel Example: User Story 3

```text
Task: T025 Assemble projectile billboards and motion-facing behavior in src/render_assembly.rs
Task: T026 Add projectile visual lookup coverage in src/texture.rs
```

---

## Implementation Strategy

### MVP First (US1 Only)

1. Complete Phase 1.
2. Complete Phase 2.
3. Complete Phase 3.
4. Validate that avatars still render correctly and the camera follows the controlled actor object.

### Incremental Delivery

1. Land the object-backed state and render assembly seam.
2. Migrate actor rendering without changing visible gameplay.
3. Add static props and pickups as the first non-player object types.
4. Add projectiles on top of the same object pipeline.
5. Finish by deleting the remaining sprite-only code paths.

### Suggested MVP Scope

- Phase 1
- Phase 2
- Phase 3

---

## Notes

- All tasks follow the required checklist format with task ID, optional `[P]` marker, optional story label, and concrete file paths.
- User stories are ordered to keep the first deliverable focused on preserving current rendering while changing the architecture underneath it.
- Later stories add new gameplay object classes without reopening the renderer boundary.