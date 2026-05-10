use std::collections::HashMap;

use crate::input::InputMessage;
use crate::model::{
    Map, ObjectId, ObjectKind, PickupKind, PlayerId, RenderBody, WorldObject,
};
use crate::texture::{visual_definition, VisualId};

pub const PLAYER_RADIUS: f64 = 0.2;
pub const TICK_RATE: u64 = 64;
pub const TICK_DT: f64 = 1.0 / TICK_RATE as f64;
pub const MOVE_SPEED: f64 = 40.0;
pub const FRICTION: f64 = 10.0;
const STATIC_PROP_RADIUS: f64 = 0.28;
const PICKUP_RADIUS: f64 = 0.2;
const PROJECTILE_RADIUS: f64 = 0.08;
const PROJECTILE_SPEED: f64 = 18.0;
const PROJECTILE_TTL_TICKS: u32 = 96;
const PROJECTILE_DAMAGE: u32 = 1;
const EPSILON: f64 = 1e-6;

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub controlled_object: ObjectId,
    pub dir_x: f64,
    pub dir_y: f64,
}

impl PlayerState {
    pub fn new(controlled_object: ObjectId) -> Self {
        Self {
            controlled_object,
            dir_x: -1.0,
            dir_y: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub players: HashMap<PlayerId, PlayerState>,
    pub objects: HashMap<ObjectId, WorldObject>,
    pub tick: u64,
    pub next_object_id: ObjectId,
    
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            objects: HashMap::new(),
            tick: 0,
            next_object_id: 1,
        }
    }

    pub fn allocate_object_id(&mut self) -> ObjectId {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }

    pub fn spawn_actor(&mut self, x: f64, y: f64, owner_player: Option<PlayerId>) -> ObjectId {
        let id = self.allocate_object_id();
        self.objects.insert(
            id,
            WorldObject {
                id,
                x,
                y,
                vel_x: 0.0,
                vel_y: 0.0,
                radius: PLAYER_RADIUS,
                render: Some(render_body(VisualId::PlayerActor)),
                kind: ObjectKind::Actor { owner_player },
            },
        );
        id
    }

    pub fn spawn_static_prop(&mut self, x: f64, y: f64) -> ObjectId {
        let id = self.allocate_object_id();
        self.objects.insert(
            id,
            WorldObject {
                id,
                x,
                y,
                vel_x: 0.0,
                vel_y: 0.0,
                radius: STATIC_PROP_RADIUS,
                render: Some(render_body(VisualId::StaticProp)),
                kind: ObjectKind::StaticProp {
                    blocks_movement: true,
                },
            },
        );
        id
    }

    pub fn spawn_pickup(&mut self, x: f64, y: f64, pickup_kind: PickupKind) -> ObjectId {
        let id = self.allocate_object_id();
        self.objects.insert(
            id,
            WorldObject {
                id,
                x,
                y,
                vel_x: 0.0,
                vel_y: 0.0,
                radius: PICKUP_RADIUS,
                render: Some(render_body(VisualId::Pickup)),
                kind: ObjectKind::Pickup { pickup_kind },
            },
        );
        id
    }

    pub fn spawn_projectile_from_player(&mut self, player_id: PlayerId) -> Option<ObjectId> {
        let player = self.players.get(&player_id)?;
        let actor = self.objects.get(&player.controlled_object)?;
        let dir_x = player.dir_x;
        let dir_y = player.dir_y;
        let actor_x = actor.x;
        let actor_y = actor.y;
        let actor_radius = actor.radius;
        let spawn_distance = actor_radius + PROJECTILE_RADIUS + 0.05;
        let spawn_x = actor_x + dir_x * spawn_distance;
        let spawn_y = actor_y + dir_y * spawn_distance;

        let id = self.allocate_object_id();
        self.objects.insert(
            id,
            WorldObject {
                id,
                x: spawn_x,
                y: spawn_y,
                vel_x: dir_x * PROJECTILE_SPEED,
                vel_y: dir_y * PROJECTILE_SPEED,
                radius: PROJECTILE_RADIUS,
                render: Some(render_body(VisualId::Projectile)),
                kind: ObjectKind::Projectile {
                    owner_player: Some(player_id),
                    ttl_ticks: PROJECTILE_TTL_TICKS,
                    damage: PROJECTILE_DAMAGE,
                },
            },
        );
        Some(id)
    }

    pub fn remove_object(&mut self, object_id: ObjectId) {
        self.objects.remove(&object_id);
    }

    pub fn controlled_object(&self, player_id: PlayerId) -> Option<&WorldObject> {
        let player = self.players.get(&player_id)?;
        self.objects.get(&player.controlled_object)
    }
}

/// pure function to advance the simulation by applying inputs to the given state
/// both clients and server can use this to stay in sync
pub fn tick(state: &GameState, inputs: &[InputMessage], map: &Map, delta: f64) -> GameState {
    let mut next = state.clone();
    for msg in inputs {
        apply_input(&mut next, msg, map, delta);
    }
    update_projectiles(&mut next, map, delta);
    collect_pickups(&mut next);
    next.tick += 1;
    next
}

pub fn apply_input(state: &mut GameState, input: &InputMessage, map: &Map, delta: f64) {
    let Some(player) = state.players.get_mut(&input.player_id) else {
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

    let controlled_object = player.controlled_object;
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

    let Some(actor) = state.objects.get_mut(&controlled_object) else {
        return;
    };

    actor.vel_x += move_dir_x * MOVE_SPEED * delta;
    actor.vel_y += move_dir_y * MOVE_SPEED * delta;

    let speed_sq = actor.vel_x * actor.vel_x + actor.vel_y * actor.vel_y;
    if speed_sq > 0.0 {
        let speed = speed_sq.sqrt();
        let drop = speed * FRICTION * delta;
        let new_speed = (speed - drop).max(0.0);
        if new_speed < speed {
            actor.vel_x *= new_speed / speed;
            actor.vel_y *= new_speed / speed;
        }
    }

    move_dynamic_object(state, controlled_object, map, delta);

    if input.fire {
        let _ = state.spawn_projectile_from_player(input.player_id);
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

fn move_dynamic_object(state: &mut GameState, object_id: ObjectId, map: &Map, delta: f64) {
    let Some(snapshot) = state.objects.get(&object_id).cloned() else {
        return;
    };

    let mut x = snapshot.x + snapshot.vel_x * delta;
    let mut y = snapshot.y + snapshot.vel_y * delta;
    let mut vel_x = snapshot.vel_x;
    let mut vel_y = snapshot.vel_y;
    let radius = snapshot.radius;

    let blockers: Vec<(f64, f64, f64)> = state
        .objects
        .values()
        .filter(|object| object.id != object_id && blocks_movement(object))
        .map(|object| (object.x, object.y, object.radius))
        .collect();

    depenetrate_walls(map, &mut x, &mut y, &mut vel_x, &mut vel_y, radius);
    depenetrate_objects(&blockers, &mut x, &mut y, &mut vel_x, &mut vel_y, radius);

    if let Some(object) = state.objects.get_mut(&object_id) {
        object.x = x;
        object.y = y;
        object.vel_x = vel_x;
        object.vel_y = vel_y;
    }
}

fn depenetrate_walls(
    map: &Map,
    x: &mut f64,
    y: &mut f64,
    vel_x: &mut f64,
    vel_y: &mut f64,
    radius: f64,
) {
    let map_h = map.tiles.len() as i32;
    let map_w = if map_h > 0 { map.tiles[0].len() as i32 } else { 0 };

    for _ in 0..2 {
        let cx = x.floor() as i32;
        let cy = y.floor() as i32;
        for oy in -1..=1i32 {
            for ox in -1..=1i32 {
                let tx = cx + ox;
                let ty = cy + oy;
                if tx < 0 || ty < 0 || tx >= map_w || ty >= map_h {
                    continue;
                }
                if !map.is_wall(tx as usize, ty as usize) {
                    continue;
                }

                let cpx = (*x).clamp(tx as f64, (tx + 1) as f64);
                let cpy = (*y).clamp(ty as f64, (ty + 1) as f64);
                let nx = *x - cpx;
                let ny = *y - cpy;
                let dist_sq = nx * nx + ny * ny;
                if dist_sq >= radius * radius {
                    continue;
                }

                let dist = dist_sq.sqrt();
                let (nx, ny) = if dist < EPSILON {
                    (1.0_f64, 0.0_f64)
                } else {
                    (nx / dist, ny / dist)
                };
                let penetration = radius - dist;
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
}

fn depenetrate_objects(
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

fn update_projectiles(state: &mut GameState, map: &Map, delta: f64) {
    let projectile_ids: Vec<ObjectId> = state
        .objects
        .iter()
        .filter_map(|(id, object)| match object.kind {
            ObjectKind::Projectile { .. } => Some(*id),
            _ => None,
        })
        .collect();

    for projectile_id in projectile_ids {
        let Some(projectile) = state.objects.get(&projectile_id).cloned() else {
            continue;
        };

        let ObjectKind::Projectile {
            owner_player,
            ttl_ticks,
            damage,
        } = projectile.kind
        else {
            continue;
        };

        if ttl_ticks == 0 {
            state.remove_object(projectile_id);
            continue;
        }

        let next_x = projectile.x + projectile.vel_x * delta;
        let next_y = projectile.y + projectile.vel_y * delta;
        let next_ttl = ttl_ticks - 1;

        let hit_wall = overlaps_wall(map, next_x, next_y, projectile.radius);
        let hit_prop = state.objects.values().any(|object| {
            object.id != projectile_id
                && blocks_movement(object)
                && circles_overlap(
                    next_x,
                    next_y,
                    projectile.radius,
                    object.x,
                    object.y,
                    object.radius,
                )
        });
        let hit_actor = state.objects.values().any(|object| match object.kind {
            ObjectKind::Actor { owner_player: target_owner } => {
                object.id != projectile_id
                    && target_owner != owner_player
                    && circles_overlap(
                        next_x,
                        next_y,
                        projectile.radius,
                        object.x,
                        object.y,
                        object.radius,
                    )
            }
            _ => false,
        });

        if hit_wall || hit_prop || hit_actor {
            state.remove_object(projectile_id);
            continue;
        }

        if let Some(object) = state.objects.get_mut(&projectile_id) {
            object.x = next_x;
            object.y = next_y;
            object.kind = ObjectKind::Projectile {
                owner_player,
                ttl_ticks: next_ttl,
                damage,
            };
        }
    }
}

fn collect_pickups(state: &mut GameState) {
    let actor_snapshots: Vec<(f64, f64, f64)> = state
        .objects
        .values()
        .filter_map(|object| match object.kind {
            ObjectKind::Actor { .. } => Some((object.x, object.y, object.radius)),
            _ => None,
        })
        .collect();

    let pickup_ids: Vec<ObjectId> = state
        .objects
        .iter()
        .filter_map(|(id, object)| match object.kind {
            ObjectKind::Pickup { .. } => Some(*id),
            _ => None,
        })
        .collect();

    for pickup_id in pickup_ids {
        let Some(pickup) = state.objects.get(&pickup_id) else {
            continue;
        };

        let collected = actor_snapshots.iter().any(|(actor_x, actor_y, actor_radius)| {
            circles_overlap(
                pickup.x,
                pickup.y,
                pickup.radius,
                *actor_x,
                *actor_y,
                *actor_radius,
            )
        });

        if collected {
            state.remove_object(pickup_id);
        }
    }
}

fn overlaps_wall(map: &Map, x: f64, y: f64, radius: f64) -> bool {
    let cx = x.floor() as i32;
    let cy = y.floor() as i32;
    let map_h = map.tiles.len() as i32;
    let map_w = if map_h > 0 { map.tiles[0].len() as i32 } else { 0 };

    for oy in -1..=1i32 {
        for ox in -1..=1i32 {
            let tx = cx + ox;
            let ty = cy + oy;
            if tx < 0 || ty < 0 || tx >= map_w || ty >= map_h {
                continue;
            }
            if !map.is_wall(tx as usize, ty as usize) {
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

fn blocks_movement(object: &WorldObject) -> bool {
    matches!(
        object.kind,
        ObjectKind::StaticProp {
            blocks_movement: true
        }
    )
}

#[cfg(test)]
mod tests {
    use super::{apply_input, GameState, PlayerState, TICK_DT};
    use crate::input::InputMessage;
    use crate::model::Map;

    #[test]
    fn diagonal_movement_matches_straight_line_speed() {
        let map = Map::new(vec![vec![0; 5]; 5]);
        let forward_state = make_state();
        let diagonal_state = make_state();

        let mut forward_input = InputMessage {
            player_id: 1,
            forward: true,
            ..InputMessage::default()
        };
        let diagonal_input = InputMessage {
            player_id: 1,
            forward: true,
            strafe_right: true,
            ..InputMessage::default()
        };

        let mut forward_state = forward_state;
        let mut diagonal_state = diagonal_state;

        apply_input(&mut forward_state, &forward_input, &map, TICK_DT);
        apply_input(&mut diagonal_state, &diagonal_input, &map, TICK_DT);

        let forward_actor = forward_state.objects.get(&1).unwrap();
        let diagonal_actor = diagonal_state.objects.get(&1).unwrap();
        let forward_speed = (forward_actor.vel_x * forward_actor.vel_x
            + forward_actor.vel_y * forward_actor.vel_y)
            .sqrt();
        let diagonal_speed = (diagonal_actor.vel_x * diagonal_actor.vel_x
            + diagonal_actor.vel_y * diagonal_actor.vel_y)
            .sqrt();

        assert!((diagonal_speed - forward_speed).abs() < 1e-9);

        forward_input.strafe_right = true;
        apply_input(&mut forward_state, &forward_input, &map, TICK_DT);
        let forward_actor = forward_state.objects.get(&1).unwrap();
        let post_turn_speed = (forward_actor.vel_x * forward_actor.vel_x
            + forward_actor.vel_y * forward_actor.vel_y)
            .sqrt();
        assert!(post_turn_speed >= forward_speed);
    }

    fn make_state() -> GameState {
        let mut state = GameState::new();
        let actor_id = state.spawn_actor(2.5, 2.5, Some(1));
        state.players.insert(1, PlayerState::new(actor_id));
        state
    }
}
