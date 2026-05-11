use std::collections::HashMap;

use crate::input::InputMessage;
use crate::model::{ControllerId, Entity, EntityId, EntityKind, Level, PickupKind, RenderBody};
use crate::texture::{VisualId, visual_definition};

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

#[derive(Clone, Debug)]
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

    pub fn spawn_pawn(&mut self, x: f64, y: f64, owner_id: Option<ControllerId>) -> EntityId {
        let id = self.allocate_entity_id();
        self.entities.insert(
            id,
            Entity {
                id,
                x,
                y,
                vel_x: 0.0,
                vel_y: 0.0,
                radius: PLAYER_RADIUS,
                render: Some(render_body(VisualId::PlayerPawn)),
                kind: EntityKind::Pawn { owner_id },
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
                vel_x: 0.0,
                vel_y: 0.0,
                radius: STATIC_PROP_RADIUS,
                render: Some(render_body(VisualId::StaticProp)),
                kind: EntityKind::StaticProp {
                    blocks_movement: true,
                },
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
                vel_x: 0.0,
                vel_y: 0.0,
                radius: PICKUP_RADIUS,
                render: Some(render_body(VisualId::Pickup)),
                kind: EntityKind::Pickup { pickup_kind },
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
                vel_x: dir_x * PROJECTILE_SPEED,
                vel_y: dir_y * PROJECTILE_SPEED,
                radius: PROJECTILE_RADIUS,
                render: Some(render_body(VisualId::Projectile)),
                kind: EntityKind::Projectile {
                    owner_id: Some(controller_id),
                    ttl_ticks: PROJECTILE_TTL_TICKS,
                    damage: PROJECTILE_DAMAGE,
                },
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
        entity.vel_x = 0.0;
        entity.vel_y = 0.0;
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

    pawn.vel_x += move_dir_x * MOVE_SPEED * delta;
    pawn.vel_y += move_dir_y * MOVE_SPEED * delta;

    let speed_sq = pawn.vel_x * pawn.vel_x + pawn.vel_y * pawn.vel_y;
    if speed_sq > 0.0 {
        let speed = speed_sq.sqrt();
        let drop = speed * FRICTION * delta;
        let new_speed = (speed - drop).max(0.0);
        if new_speed < speed {
            pawn.vel_x *= new_speed / speed;
            pawn.vel_y *= new_speed / speed;
        }
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

    let blockers: Vec<(f64, f64, f64)> = state
        .entities
        .values()
        .filter(|entity| entity.id != entity_id && blocks_movement(entity))
        .map(|entity| (entity.x, entity.y, entity.radius))
        .collect();

    depenetrate_walls(level, &mut x, &mut y, &mut vel_x, &mut vel_y, radius);
    depenetrate_entities(&blockers, &mut x, &mut y, &mut vel_x, &mut vel_y, radius);

    if let Some(entity) = state.entities.get_mut(&entity_id) {
        entity.x = x;
        entity.y = y;
        entity.vel_x = vel_x;
        entity.vel_y = vel_y;
    }
}

fn depenetrate_walls(
    level: &Level,
    x: &mut f64,
    y: &mut f64,
    vel_x: &mut f64,
    vel_y: &mut f64,
    radius: f64,
) {
    let level_h = level.tiles.len() as i32;
    let level_w = if level_h > 0 {
        level.tiles[0].len() as i32
    } else {
        0
    };

    for _ in 0..2 {
        let cx = x.floor() as i32;
        let cy = y.floor() as i32;
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
        let hit_prop = state.entities.values().any(|entity| {
            entity.id != projectile_id
                && blocks_movement(entity)
                && circles_overlap(
                    next_x,
                    next_y,
                    projectile.radius,
                    entity.x,
                    entity.y,
                    entity.radius,
                )
        });
        let hit_pawn = state.entities.values().any(|entity| match entity.kind {
            EntityKind::Pawn {
                owner_id: target_owner,
            } => {
                entity.id != projectile_id
                    && target_owner != owner_id
                    && circles_overlap(
                        next_x,
                        next_y,
                        projectile.radius,
                        entity.x,
                        entity.y,
                        entity.radius,
                    )
            }
            _ => false,
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
    use super::{GameState, Player, TICK_DT, apply_input};
    use crate::input::InputMessage;
    use crate::model::Level;

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

    fn make_state() -> GameState {
        let mut state = GameState::new();
        let pawn_id = state.spawn_pawn(2.5, 2.5, Some(1));
        state.players.insert(1, Player::new(pawn_id));
        state
    }
}
