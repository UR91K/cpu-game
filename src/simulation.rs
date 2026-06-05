#![allow(unused)]
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::input::InputMessage;
use crate::model::{ControllerId, Entity, EntityId, EntityKind, Level, MapTri, PickupKind, RenderBody};
use crate::texture::{VisualId, visual_definition};

/// 1 unit = 10 cm.

pub const PLAYER_RADIUS: f64 = 0.2;
pub const TICK_RATE: u64 = 64;
pub const TICK_DT: f64 = 1.0 / TICK_RATE as f64;
pub const PLAYER_ACCEL: f64 = 40.0;
pub const GROUND_FRICTION: f64 = 10.0;
const STATIC_PROP_RADIUS: f64 = 0.28;
const PICKUP_RADIUS: f64 = 0.2;
const PROJECTILE_RADIUS: f64 = 0.08;
const PROJECTILE_SPEED: f64 = 18.0;
const PROJECTILE_TTL_TICKS: u32 = 96;
const PROJECTILE_DAMAGE: u32 = 1;
const EPSILON: f64 = 1e-6;
const GRAVITY: f64 = 0.98;   // units/s² (world Y, downward)
const JUMP_IMPULSE: f64 = 2.0; // units/s upward

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub pawn_id: EntityId,
    pub dir_x: f64,
    pub dir_y: f64,
}

impl Player {
    pub fn new(pawn_id: EntityId) -> Self {
        Self {
            pawn_id,
            dir_x: -1.0,
            dir_y: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub players: HashMap<ControllerId, Player>,
    pub entities: HashMap<EntityId, Entity>,
    pub tick: u64,
    pub next_entity_id: EntityId,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            entities: HashMap::new(),
            tick: 0,
            next_entity_id: 1,
        }
    }

    pub fn allocate_entity_id(&mut self) -> EntityId {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        id
    }

    pub fn spawn_pawn(&mut self, x: f64, y: f64, z: f64, owner_id: Option<ControllerId>) -> EntityId {
        let id = self.allocate_entity_id();
        self.entities.insert(
            id,
            Entity {
                id,
                x,
                y,
                z,
                vel_x: 0.0,
                vel_y: 0.0,
                vel_z: 0.0,
                radius: PLAYER_RADIUS,
                render: Some(render_body(VisualId::PlayerPawn)),
                kind: EntityKind::Pawn { owner_id },
                on_ground: true,
            },
        );
        id
    }

    pub fn spawn_static_prop(&mut self, x: f64, y: f64) -> EntityId {
        let id = self.allocate_entity_id();
        self.entities.insert(
            id,
            Entity {
                id,
                x,
                y,
                z: 0.0,
                vel_x: 0.0,
                vel_y: 0.0,
                vel_z: 0.0,
                radius: STATIC_PROP_RADIUS,
                render: Some(render_body(VisualId::StaticProp)),
                kind: EntityKind::StaticProp {
                    blocks_movement: true,
                },
                on_ground: false,
            },
        );
        id
    }

    pub fn spawn_pickup(&mut self, x: f64, y: f64, pickup_kind: PickupKind) -> EntityId {
        let id = self.allocate_entity_id();
        self.entities.insert(
            id,
            Entity {
                id,
                x,
                y,
                z: 0.0,
                vel_x: 0.0,
                vel_y: 0.0,
                vel_z: 0.0,
                radius: PICKUP_RADIUS,
                render: Some(render_body(VisualId::Pickup)),
                kind: EntityKind::Pickup { pickup_kind },
                on_ground: false,
            },
        );
        id
    }

    pub fn spawn_projectile_from_player(
        &mut self,
        controller_id: ControllerId,
    ) -> Option<EntityId> {
        let player = self.players.get(&controller_id)?;
        let pawn = self.entities.get(&player.pawn_id)?;
        let dir_x = player.dir_x;
        let dir_y = player.dir_y;
        let pawn_x = pawn.x;
        let pawn_y = pawn.y;
        let pawn_z = pawn.z;
        let pawn_radius = pawn.radius;
        let spawn_distance = pawn_radius + PROJECTILE_RADIUS + 0.05;
        let spawn_x = pawn_x + dir_x * spawn_distance;
        let spawn_y = pawn_y + dir_y * spawn_distance;

        let id = self.allocate_entity_id();
        self.entities.insert(
            id,
            Entity {
                id,
                x: spawn_x,
                y: spawn_y,
                z: pawn_z,
                vel_x: dir_x * PROJECTILE_SPEED,
                vel_y: dir_y * PROJECTILE_SPEED,
                vel_z: 0.0,
                radius: PROJECTILE_RADIUS,
                render: Some(render_body(VisualId::Projectile)),
                kind: EntityKind::Projectile {
                    owner_id: Some(controller_id),
                    ttl_ticks: PROJECTILE_TTL_TICKS,
                    damage: PROJECTILE_DAMAGE,
                },
                on_ground: false,
            },
        );
        Some(id)
    }

    pub fn remove_entity(&mut self, entity_id: EntityId) {
        self.entities.remove(&entity_id);
    }

    pub fn teleport_entity(&mut self, entity_id: EntityId, x: f64, y: f64) -> Option<()> {
        let entity = self.entities.get_mut(&entity_id)?;
        entity.x = x;
        entity.y = y;
        entity.z = 0.0;
        entity.vel_x = 0.0;
        entity.vel_y = 0.0;
        entity.vel_z = 0.0;
        Some(())
    }

    pub fn controlled_entity(&self, player_id: ControllerId) -> Option<&Entity> {
        let player = self.players.get(&player_id)?;
        self.entities.get(&player.pawn_id)
    }
}

/// pure function to advance the simulation by applying inputs to the given state
/// both controllers and server can use this to stay in sync
pub fn tick(state: &GameState, inputs: &[InputMessage], level: &Level, delta: f64) -> GameState {
    let mut next = state.clone();
    for msg in inputs {
        apply_input(&mut next, msg, level, delta);
    }
    update_projectiles(&mut next, level, delta);
    collect_pickups(&mut next);
    next.tick += 1;
    next
}

pub fn apply_input(state: &mut GameState, input: &InputMessage, level: &Level, delta: f64) {
    let Some(player) = state.players.get_mut(&input.controller_id) else {
        return;
    };

    // rotation
    // apply before movement so that movement is based on the new direction immediately
    if input.rotate_delta != 0.0 {
        let angle = input.rotate_delta;
        let (sin, cos) = angle.sin_cos();
        let old_dir_x = player.dir_x;
        player.dir_x = old_dir_x * cos - player.dir_y * sin;
        player.dir_y = old_dir_x * sin + player.dir_y * cos;
    }

    let pawn_id = player.pawn_id;
    let dir_x = player.dir_x;
    let dir_y = player.dir_y;
    // right vector: dir rotated 90° CW, independent of FOV
    let right_x = dir_y;
    let right_y = -dir_x;

    let mut move_dir_x = 0.0f64;
    let mut move_dir_y = 0.0f64;
    if input.forward {
        move_dir_x += dir_x;
        move_dir_y += dir_y;
    }
    if input.back {
        move_dir_x -= dir_x;
        move_dir_y -= dir_y;
    }
    if input.strafe_left {
        move_dir_x -= right_x;
        move_dir_y -= right_y;
    }
    if input.strafe_right {
        move_dir_x += right_x;
        move_dir_y += right_y;
    }

    // normalize movement
    let move_len_sq = move_dir_x * move_dir_x + move_dir_y * move_dir_y;
    if move_len_sq > 0.0 {
        let move_len = move_len_sq.sqrt();
        move_dir_x /= move_len;
        move_dir_y /= move_len;
    }

    let Some(pawn) = state.entities.get_mut(&pawn_id) else {
        return;
    };

    pawn.vel_x += move_dir_x * PLAYER_ACCEL * delta;
    pawn.vel_y += move_dir_y * PLAYER_ACCEL * delta;

    let speed_sq = pawn.vel_x * pawn.vel_x + pawn.vel_y * pawn.vel_y;
    if speed_sq > 0.0 {
        let speed = speed_sq.sqrt();
        let drop = speed * GROUND_FRICTION * delta;
        let new_speed = (speed - drop).max(0.0);
        if new_speed < speed {
            pawn.vel_x *= new_speed / speed;
            pawn.vel_y *= new_speed / speed;
        }
    }

    if input.jump && pawn.on_ground {
        pawn.vel_z = JUMP_IMPULSE;
    }

    move_dynamic_entity(state, pawn_id, level, delta);

    if input.fire {
        let _ = state.spawn_projectile_from_player(input.controller_id);
    }
}

fn render_body(visual: VisualId) -> RenderBody {
    let definition = visual_definition(visual);
    RenderBody {
        visual,
        width: definition.billboard_width,
        height: definition.billboard_height,
        facing_mode: definition.facing_mode,
        animation: definition.animation,
    }
}

fn move_dynamic_entity(state: &mut GameState, entity_id: EntityId, level: &Level, delta: f64) {
    let Some(snapshot) = state.entities.get(&entity_id).cloned() else {
        return;
    };

    let mut x = snapshot.x + snapshot.vel_x * delta;
    let mut y = snapshot.y + snapshot.vel_y * delta;
    let mut vel_x = snapshot.vel_x;
    let mut vel_y = snapshot.vel_y;
    let radius = snapshot.radius;

    // Vertical: apply gravity then integrate position.
    let mut vel_z = snapshot.vel_z - GRAVITY * delta;
    let mut z = snapshot.z + vel_z * delta;

    let mut on_ground = false;

    if level.tris.is_empty() {
        // Fallback when no triangle geometry (e.g. unit tests).
        if z <= 0.0 {
            z = 0.0;
            vel_z = vel_z.max(0.0);
            on_ground = true;
        }
    } else {
        let r = radius as f32;
        // Sphere center in map space: map(x, y_up, z_depth) = physics(x, z+r, y)
        let mut cx = x as f32;
        let mut cy = (z + radius) as f32;
        let mut cz = y as f32;
        depenetrate_tris(&level.tris, &mut cx, &mut cy, &mut cz,
                         &mut vel_x, &mut vel_y, &mut vel_z, r, &mut on_ground);
        x = cx as f64;
        y = cz as f64;
        z = cy as f64 - radius;
    }

    let blockers: Vec<(f64, f64, f64)> = state
        .entities
        .values()
        .filter(|entity| entity.id != entity_id && blocks_movement(entity))
        .map(|entity| (entity.x, entity.y, entity.radius))
        .collect();

    depenetrate_entities(&blockers, &mut x, &mut y, &mut vel_x, &mut vel_y, radius);

    if let Some(entity) = state.entities.get_mut(&entity_id) {
        entity.x = x;
        entity.y = y;
        entity.z = z;
        entity.vel_x = vel_x;
        entity.vel_y = vel_y;
        entity.vel_z = vel_z;
        entity.on_ground = on_ground;
    }
}

fn depenetrate_tris(
    tris: &[MapTri],
    cx: &mut f32,
    cy: &mut f32,
    cz: &mut f32,
    vel_x: &mut f64,
    vel_y: &mut f64,
    vel_z: &mut f64,
    radius: f32,
    on_ground: &mut bool,
) {
    for _ in 0..3 {
        for tri in tris {
            let Some((nx, ny, nz, pen)) = sphere_vs_tri([*cx, *cy, *cz], radius, tri) else {
                continue;
            };
            *cx += nx * pen;
            *cy += ny * pen;
            *cz += nz * pen;

            // Velocity sliding: physics vel_map = [vel_x, vel_z, vel_y] (x→x, z↔y)
            let vx = *vel_x as f32;
            let vy = *vel_z as f32;
            let vz = *vel_y as f32;
            let vdotn = vx * nx + vy * ny + vz * nz;
            if vdotn < 0.0 {
                *vel_x -= (nx * vdotn) as f64;
                *vel_z -= (ny * vdotn) as f64;
                *vel_y -= (nz * vdotn) as f64;
            }

            if ny > 0.7 {
                *on_ground = true;
            }
        }
    }
}

/// Returns (push_nx, push_ny, push_nz, penetration) if sphere penetrates the triangle.
/// Map surfaces are treated as two-sided because the current test geometry is a shell mesh,
/// not closed solid volumes with thickness.
fn sphere_vs_tri(center: [f32; 3], radius: f32, tri: &MapTri) -> Option<(f32, f32, f32, f32)> {
    let n = tri.normal;
    let d = (center[0] - tri.a[0]) * n[0]
          + (center[1] - tri.a[1]) * n[1]
          + (center[2] - tri.a[2]) * n[2];
    if d.abs() > radius {
        return None;
    }

    let closest = closest_pt_on_tri(center, tri.a, tri.b, tri.c);
    let dx = center[0] - closest[0];
    let dy = center[1] - closest[1];
    let dz = center[2] - closest[2];
    let dist_sq = dx * dx + dy * dy + dz * dz;
    if dist_sq >= radius * radius {
        return None;
    }

    let dist = dist_sq.sqrt();
    let (nx, ny, nz) = if dist < 1e-6 {
        (n[0], n[1], n[2])
    } else {
        (dx / dist, dy / dist, dz / dist)
    };
    Some((nx, ny, nz, radius - dist))
}

/// Closest point on triangle (a, b, c) to point p — Ericson barycentric method.
fn closest_pt_on_tri(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0]-a[0], b[1]-a[1], b[2]-a[2]];
    let ac = [c[0]-a[0], c[1]-a[1], c[2]-a[2]];
    let ap = [p[0]-a[0], p[1]-a[1], p[2]-a[2]];

    let d1 = dot3(ab, ap);
    let d2 = dot3(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 { return a; }

    let bp = [p[0]-b[0], p[1]-b[1], p[2]-b[2]];
    let d3 = dot3(ab, bp);
    let d4 = dot3(ac, bp);
    if d3 >= 0.0 && d4 <= d3 { return b; }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0]+ab[0]*v, a[1]+ab[1]*v, a[2]+ab[2]*v];
    }

    let cp = [p[0]-c[0], p[1]-c[1], p[2]-c[2]];
    let d5 = dot3(ab, cp);
    let d6 = dot3(ac, cp);
    if d6 >= 0.0 && d5 <= d6 { return c; }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0]+ac[0]*w, a[1]+ac[1]*w, a[2]+ac[2]*w];
    }

    let va = d3 * d6 - d5 * d4;
    let d34 = d3 - d4;
    let d65 = d6 - d5;
    if va <= 0.0 && d34 >= 0.0 && d65 >= 0.0 {
        let bc = [c[0]-b[0], c[1]-b[1], c[2]-b[2]];
        let w = d34 / (d34 + d65);
        return [b[0]+bc[0]*w, b[1]+bc[1]*w, b[2]+bc[2]*w];
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [a[0]+ab[0]*v+ac[0]*w, a[1]+ab[1]*v+ac[1]*w, a[2]+ab[2]*v+ac[2]*w]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0]*b[0] + a[1]*b[1] + a[2]*b[2]
}

fn depenetrate_entities(
    blockers: &[(f64, f64, f64)],
    x: &mut f64,
    y: &mut f64,
    vel_x: &mut f64,
    vel_y: &mut f64,
    radius: f64,
) {
    for _ in 0..2 {
        for (other_x, other_y, other_radius) in blockers {
            let dx = *x - *other_x;
            let dy = *y - *other_y;
            let min_dist = radius + *other_radius;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq >= min_dist * min_dist {
                continue;
            }

            let dist = dist_sq.sqrt();
            let (nx, ny) = if dist < EPSILON {
                (1.0_f64, 0.0_f64)
            } else {
                (dx / dist, dy / dist)
            };
            let penetration = min_dist - dist;
            *x += nx * penetration;
            *y += ny * penetration;

            let vel_dot_n = *vel_x * nx + *vel_y * ny;
            if vel_dot_n < 0.0 {
                *vel_x -= nx * vel_dot_n;
                *vel_y -= ny * vel_dot_n;
            }
        }
    }
}

fn update_projectiles(state: &mut GameState, level: &Level, delta: f64) {
    let projectile_ids: Vec<EntityId> = state
        .entities
        .iter()
        .filter_map(|(id, entity)| match entity.kind {
            EntityKind::Projectile { .. } => Some(*id),
            _ => None,
        })
        .collect();
    let static_blockers: Vec<(EntityId, f64, f64, f64)> = state
        .entities
        .iter()
        .filter_map(|(id, entity)| {
            blocks_movement(entity).then_some((*id, entity.x, entity.y, entity.radius))
        })
        .collect();
    let pawn_targets: Vec<(EntityId, Option<ControllerId>, f64, f64, f64)> = state
        .entities
        .iter()
        .filter_map(|(id, entity)| match entity.kind {
            EntityKind::Pawn { owner_id } => {
                Some((*id, owner_id, entity.x, entity.y, entity.radius))
            }
            _ => None,
        })
        .collect();

    for projectile_id in projectile_ids {
        let Some(projectile) = state.entities.get(&projectile_id).cloned() else {
            continue;
        };

        let EntityKind::Projectile {
            owner_id,
            ttl_ticks,
            damage,
        } = projectile.kind
        else {
            continue;
        };

        if ttl_ticks == 0 {
            state.remove_entity(projectile_id);
            continue;
        }

        let next_x = projectile.x + projectile.vel_x * delta;
        let next_y = projectile.y + projectile.vel_y * delta;
        let next_ttl = ttl_ticks - 1;

        let hit_wall = overlaps_wall(level, next_x, next_y, projectile.radius);
        let hit_prop = static_blockers.iter().any(|(entity_id, x, y, radius)| {
            *entity_id != projectile_id
                && circles_overlap(next_x, next_y, projectile.radius, *x, *y, *radius)
        });
        let hit_pawn = pawn_targets
            .iter()
            .any(|(entity_id, target_owner, x, y, radius)| {
                *entity_id != projectile_id
                    && *target_owner != owner_id
                    && circles_overlap(next_x, next_y, projectile.radius, *x, *y, *radius)
            });

        if hit_wall || hit_prop || hit_pawn {
            state.remove_entity(projectile_id);
            continue;
        }

        if let Some(entity) = state.entities.get_mut(&projectile_id) {
            entity.x = next_x;
            entity.y = next_y;
            entity.kind = EntityKind::Projectile {
                owner_id,
                ttl_ticks: next_ttl,
                damage,
            };
        }
    }
}

fn collect_pickups(state: &mut GameState) {
    let pawn_snapshots: Vec<(f64, f64, f64)> = state
        .entities
        .values()
        .filter_map(|entity| match entity.kind {
            EntityKind::Pawn { .. } => Some((entity.x, entity.y, entity.radius)),
            _ => None,
        })
        .collect();

    let pickup_ids: Vec<EntityId> = state
        .entities
        .iter()
        .filter_map(|(id, entity)| match entity.kind {
            EntityKind::Pickup { .. } => Some(*id),
            _ => None,
        })
        .collect();

    for pickup_id in pickup_ids {
        let Some(pickup) = state.entities.get(&pickup_id) else {
            continue;
        };

        let collected = pawn_snapshots.iter().any(|(pawn_x, pawn_y, pawn_radius)| {
            circles_overlap(
                pickup.x,
                pickup.y,
                pickup.radius,
                *pawn_x,
                *pawn_y,
                *pawn_radius,
            )
        });

        if collected {
            state.remove_entity(pickup_id);
        }
    }
}

fn overlaps_wall(level: &Level, x: f64, y: f64, radius: f64) -> bool {
    let cx = x.floor() as i32;
    let cy = y.floor() as i32;
    let level_h = level.tiles.len() as i32;
    let level_w = if level_h > 0 {
        level.tiles[0].len() as i32
    } else {
        0
    };

    for oy in -1..=1i32 {
        for ox in -1..=1i32 {
            let tx = cx + ox;
            let ty = cy + oy;
            if tx < 0 || ty < 0 || tx >= level_w || ty >= level_h {
                continue;
            }
            if !level.is_wall(tx as usize, ty as usize) {
                continue;
            }

            let cpx = x.clamp(tx as f64, (tx + 1) as f64);
            let cpy = y.clamp(ty as f64, (ty + 1) as f64);
            let dx = x - cpx;
            let dy = y - cpy;
            if dx * dx + dy * dy < radius * radius {
                return true;
            }
        }
    }

    false
}

fn circles_overlap(x1: f64, y1: f64, r1: f64, x2: f64, y2: f64, r2: f64) -> bool {
    let dx = x1 - x2;
    let dy = y1 - y2;
    let min_dist = r1 + r2;
    dx * dx + dy * dy < min_dist * min_dist
}

fn blocks_movement(entity: &Entity) -> bool {
    matches!(
        entity.kind,
        EntityKind::StaticProp {
            blocks_movement: true
        }
    )
}

#[cfg(test)]
mod tests {
    use super::{GameState, Player, TICK_DT, apply_input, sphere_vs_tri};
    use crate::input::InputMessage;
    use crate::model::{Level, MapTri};

    #[test]
    fn diagonal_movement_matches_straight_line_speed() {
        let level = Level::new(vec![vec![0; 5]; 5]);
        let forward_state = make_state();
        let diagonal_state = make_state();

        let mut forward_input = InputMessage {
            controller_id: 1,
            forward: true,
            ..InputMessage::default()
        };
        let diagonal_input = InputMessage {
            controller_id: 1,
            forward: true,
            strafe_right: true,
            ..InputMessage::default()
        };

        let mut forward_state = forward_state;
        let mut diagonal_state = diagonal_state;

        apply_input(&mut forward_state, &forward_input, &level, TICK_DT);
        apply_input(&mut diagonal_state, &diagonal_input, &level, TICK_DT);

        let forward_pawn = forward_state.entities.get(&1).unwrap();
        let diagonal_pawn = diagonal_state.entities.get(&1).unwrap();
        let forward_speed = (forward_pawn.vel_x * forward_pawn.vel_x
            + forward_pawn.vel_y * forward_pawn.vel_y)
            .sqrt();
        let diagonal_speed = (diagonal_pawn.vel_x * diagonal_pawn.vel_x
            + diagonal_pawn.vel_y * diagonal_pawn.vel_y)
            .sqrt();

        assert!((diagonal_speed - forward_speed).abs() < 1e-9);

        forward_input.strafe_right = true;
        apply_input(&mut forward_state, &forward_input, &level, TICK_DT);
        let forward_pawn = forward_state.entities.get(&1).unwrap();
        let post_turn_speed = (forward_pawn.vel_x * forward_pawn.vel_x
            + forward_pawn.vel_y * forward_pawn.vel_y)
            .sqrt();
        assert!(post_turn_speed >= forward_speed);
    }

    #[test]
    fn teleport_entity_updates_position_and_clears_velocity() {
        let mut state = make_state();
        let pawn = state.entities.get_mut(&1).unwrap();
        pawn.vel_x = 3.5;
        pawn.vel_y = -1.25;

        assert_eq!(state.teleport_entity(1, 7.0, 9.0), Some(()));

        let pawn = state.entities.get(&1).unwrap();
        assert_eq!(pawn.x, 7.0);
        assert_eq!(pawn.y, 9.0);
        assert_eq!(pawn.vel_x, 0.0);
        assert_eq!(pawn.vel_y, 0.0);
    }

    #[test]
    fn wall_triangle_blocks_from_both_sides() {
        let tri = MapTri::new([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0]);
        let radius = 0.2;

        let from_positive_x = sphere_vs_tri([0.1, 0.5, 0.5], radius, &tri);
        let from_negative_x = sphere_vs_tri([-0.1, 0.5, 0.5], radius, &tri);

        assert!(from_positive_x.is_some());
        assert!(from_negative_x.is_some());
    }

    fn make_state() -> GameState {
        let mut state = GameState::new();
        let pawn_id = state.spawn_pawn(2.5, 2.5, 0.0, Some(1));
        state.players.insert(1, Player::new(pawn_id));
        state
    }
}
