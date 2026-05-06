
The current bottleneck is exactly where you called it out: simulation.rs stores both player state and a sprite list, and simulation.rs rebuilds that sprite list from players every tick. Then app.rs pulls camera data directly from players and app.rs passes the derived sprites into both renderers. That means “things that can be seen” are not first-class simulation objects, they are a render-side projection of players only.

**Target Shape**

Use a small world-object model, not an ECS.

- Keep players as controllers/view state.
- Move anything that physically exists in the world into a single object store.
- Add a render assembly pass that converts world objects into a renderer-friendly draw list.
- Remove sprite construction from simulation entirely.

The core runtime state should become:

- GameState
  - players: controller records keyed by PlayerId
  - objects: world objects keyed by ObjectId
  - tick
  - next_object_id

Players should no longer own world position directly. They should reference a controlled object.

- PlayerState
  - controlled_object: ObjectId
  - dir_x, dir_y
  - plane_x, plane_y

World objects should be enum-based and explicit, because this codebase is still small and an ECS would be overhead.

- WorldObject
  - id: ObjectId
  - x, y
  - vel_x, vel_y
  - radius
  - render: Option<RenderBody>
  - kind: ObjectKind

- ObjectKind
  - Actor { owner_player: Option<PlayerId> }
  - StaticProp { blocks_movement: bool }
  - Pickup { pickup_kind }
  - Projectile { owner_player: Option<PlayerId>, ttl_ticks: u32, damage: u32 }

That gives you one place to put static objects, pickups, projectiles, and player avatars without teaching the renderer about gameplay rules.

**Render Boundary**

Add a new module, preferably src/render_assembly.rs as the seam between simulation and rendering.

Its job should be:

- Resolve the viewer camera from PlayerState plus its controlled object.
- Iterate world objects that have a render component.
- Convert them into a flat render scene.

The output should look like:

- RenderScene
  - camera: RenderCamera
  - billboards: Vec<RenderBillboard>

- RenderCamera
  - x, y
  - dir_x, dir_y
  - plane_x, plane_y

- RenderBillboard
  - x, y
  - visual: VisualId
  - facing_angle: Option<f64>
  - moving: bool
  - width, height
  - elevation if you want later vertical offsets
  - sort_distance_sq if you want assembly to own sort order

The important point is that render assembly should produce “what to draw,” not “what exists in gameplay.” That decouples gpu_renderer.rs and renderer.rs from GameState internals.

**Asset/Visual Model**

Do not keep raw texture_index in world state. That leaks renderer storage details into gameplay.

Instead, add a visual id layer in texture.rs or a new assets module.

- VisualId
  - PlayerMarine
  - Barrel
  - Medkit
  - Fireball
  - etc.

- RenderBody
  - visual: VisualId
  - width
  - height
  - facing_mode: Fixed | EightWay
  - animation: None | WalkCycle | Spin | PingPong

Simulation objects reference VisualId. The render assembly and texture system resolve that into atlas slots or image frames. That is the right place to express “projectile uses this sprite” or “pickup spins.”

**How This Maps To Existing Files**

- model.rs should stop being the place for Sprite. Keep Map and AO there, or rename it toward static world data later.
- simulation.rs should own GameState, PlayerState, WorldObject, ObjectKind, and tick logic. The line that rebuilds sprites from players goes away completely.
- server.rs should create a player avatar object when a client joins, then store its ObjectId in PlayerState.
- bot.rs should steer from its controlled object position, not from state.players position fields.
- app.rs should ask render assembly for a RenderScene instead of separately fetching player and sprites.
- gpu_renderer.rs should accept RenderCamera plus RenderBillboard slice.
- renderer.rs should accept the same RenderCamera plus RenderBillboard slice.
- main.rs should seed initial world objects, either directly for now or via map spawn data.

**Simulation Rules After The Change**

This is the behavior split I would use:

- Player input mutates only the controlled actor object’s movement and the player’s view direction.
- Static props exist in objects and may block movement.
- Pickups exist in objects, do not block movement, and are removed when an actor overlaps them.
- Projectiles exist in objects, advance each tick, collide with walls or actors, and expire on ttl or impact.
- Render assembly reads all of that and emits billboards, but does not change GameState.

That keeps simulation authoritative and deterministic, which matters because client.rs replays authoritative state locally.

**Migration Order**

1. Add ObjectId, WorldObject, ObjectKind, and controlled_object on PlayerState while keeping the old sprite list temporarily.
2. Change server join logic in server.rs so each player gets an actor object.
3. Add render assembly and have it build player billboards from actor objects, but still compare output against the old sprite path.
4. Switch app.rs and both renderers to RenderScene input.
5. Delete Sprite from model.rs and remove sprite generation from simulation.rs.
6. Add StaticProp, Pickup, and Projectile constructors plus simple spawn helpers on GameState or Server.
7. Only after that, extend map loading to include object spawn markers if you want authored props in the map.

**What I Would Not Do**

- I would not introduce a full ECS.
- I would not store render-ready vertices or atlas indices in GameState.
- I would not keep player position duplicated in both PlayerState and WorldObject.
- I would not add projectiles as a special-case side list next to objects. That just recreates the current sprite problem with a different name.

**Concrete End State**

When this is done, the flow should be:

- Inputs come in.
- Simulation updates GameState.players and GameState.objects.
- App asks render assembly for a RenderScene for one viewer.
- Renderers draw RenderScene.
- New gameplay objects are created by inserting WorldObject entries, not by teaching the renderer new special cases.

That is the smallest architectural change that actually unlocks static props, pickups, and projectiles without making the codebase heavier than it needs to be.

If you want, I can do the next step in code and scaffold the target shape directly:
1. Introduce the new GameState and world object types without changing behavior.
2. Add render_assembly and cut the renderer inputs over.
3. Remove Sprite entirely and migrate player avatars into world objects.