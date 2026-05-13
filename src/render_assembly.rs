use crate::model::{ControllerId, EntityKind};
use crate::simulation::GameState;
use crate::texture::{AnimationStyle, FacingMode, TextureKey, visual_definition};

const WALK_ANIMATION_MIN_SPEED_SQ: f64 = 1.0;

#[derive(Clone, Debug)]
pub struct RenderScene {
    pub camera: RenderCamera,
    pub billboards: Vec<RenderBillboard>,
}

#[derive(Clone, Debug)]
pub struct RenderCamera {
    pub x: f64,
    pub y: f64,
    pub dir_x: f64,
    pub dir_y: f64,
    pub plane_x: f64,
    pub plane_y: f64,
}

#[derive(Clone, Debug)]
pub struct RenderBillboard {
    pub x: f64,
    pub y: f64,
    pub texture: TextureKey,
    pub facing_dir: (f64, f64),
    pub is_moving: bool,
    pub width: f32,
    pub height: f32,
    pub facing_mode: FacingMode,
    pub animation: AnimationStyle,
}

pub fn assemble_scene(
    state: &GameState,
    viewer: ControllerId,
    fov_plane_len: f64,
) -> Option<RenderScene> {
    let player = state.players.get(&viewer)?;
    let pawn = state.entities.get(&player.pawn_id)?;
    // Derive camera plane from dir rotated 90° CW, scaled by fov_plane_len.
    // plane_len = tan(half_hfov), so fov_plane_len controls horizontal FOV.
    let plane_x = player.dir_y * fov_plane_len;
    let plane_y = -player.dir_x * fov_plane_len;
    let camera = RenderCamera {
        x: pawn.x,
        y: pawn.y,
        dir_x: player.dir_x,
        dir_y: player.dir_y,
        plane_x,
        plane_y,
    };

    let billboards = state
        .entities
        .values()
        .filter_map(|entity| {
            let render = entity.render.as_ref()?;
            let definition = visual_definition(render.visual);
            let speed_sq = entity.vel_x * entity.vel_x + entity.vel_y * entity.vel_y;
            let is_moving = speed_sq > WALK_ANIMATION_MIN_SPEED_SQ;
            let facing_dir = match entity.kind {
                EntityKind::Pawn {
                    owner_id: Some(owner_id),
                } => state
                    .players
                    .get(&owner_id)
                    .map(|owner| (owner.dir_x, owner.dir_y))
                    .unwrap_or((0.0, 0.0)),
                _ if is_moving => (entity.vel_x, entity.vel_y),
                _ => (0.0, 0.0),
            };

            Some(RenderBillboard {
                x: entity.x,
                y: entity.y,
                texture: definition.texture,
                facing_dir,
                is_moving,
                width: render.width,
                height: render.height,
                facing_mode: render.facing_mode,
                animation: render.animation,
            })
        })
        .collect();

    Some(RenderScene { camera, billboards })
}
