use crate::model::{ObjectKind, PlayerId};
use crate::simulation::GameState;
use crate::texture::{visual_definition, AnimationStyle, FacingMode, TextureKey};

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
    pub movement_angle: f64,
    pub is_moving: bool,
    pub width: f32,
    pub height: f32,
    pub facing_mode: FacingMode,
    pub animation: AnimationStyle,
}

pub fn assemble_scene(state: &GameState, viewer: PlayerId) -> Option<RenderScene> {
    let player = state.players.get(&viewer)?;
    let actor = state.objects.get(&player.controlled_object)?;
    let camera = RenderCamera {
        x: actor.x,
        y: actor.y,
        dir_x: player.dir_x,
        dir_y: player.dir_y,
        plane_x: player.plane_x,
        plane_y: player.plane_y,
    };

    let billboards = state
        .objects
        .values()
        .filter_map(|object| {
            let render = object.render.as_ref()?;
            let definition = visual_definition(render.visual);
            let speed_sq = object.vel_x * object.vel_x + object.vel_y * object.vel_y;
            let is_moving = speed_sq > 1e-6;
            let movement_angle = if is_moving {
                object.vel_y.atan2(object.vel_x)
            } else {
                match object.kind {
                    ObjectKind::Actor { owner_player: Some(owner_player) } => state
                        .players
                        .get(&owner_player)
                        .map(|owner| owner.dir_y.atan2(owner.dir_x))
                        .unwrap_or(0.0),
                    _ => 0.0,
                }
            };

            Some(RenderBillboard {
                x: object.x,
                y: object.y,
                texture: definition.texture,
                movement_angle,
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