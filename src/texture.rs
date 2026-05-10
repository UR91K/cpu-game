use std::collections::HashMap;

use include_dir::{include_dir, Dir};

static TEXTURES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/textures");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallTexture {
    Green,
    Orange,
    Grey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloorTexture {
    Smooth,
    MilkVeins,
}

impl FloorTexture {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Smooth),
            1 => Some(Self::MilkVeins),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemTexture {
    Health,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorTexture {
    Red,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectileTexture {
    Spiral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureKey {
    Wall(WallTexture),
    Floor(FloorTexture),
    Item(ItemTexture),
    Actor(ActorTexture),
    Projectile(ProjectileTexture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisualId {
    PlayerActor,
    StaticProp,
    Pickup,
    Projectile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacingMode {
    Fixed,
    Movement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationStyle {
    Static,
    WalkPingPong,
    LoopStrip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationPlayback {
    PingPong,
    Loop,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationDescriptor {
    pub frame_width: usize,
    pub frame_height: usize,
    pub columns: usize,
    pub rows: usize,
    pub ms_per_frame: f64,
    pub playback: AnimationPlayback,
    pub directional_rows: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct VisualDefinition {
    pub texture: TextureKey,
    pub billboard_width: f32,
    pub billboard_height: f32,
    pub facing_mode: FacingMode,
    pub animation: AnimationStyle,
}

#[derive(Clone, Debug)]
pub struct TextureManager {
    textures: Vec<image::RgbaImage>,
    keys_by_index: Vec<TextureKey>,
    index_by_key: HashMap<TextureKey, usize>,
}

impl TextureManager {
    pub fn load() -> Self {
        let mut entries: Vec<(TextureKey, image::RgbaImage)> = Vec::new();

        for file in TEXTURES_DIR.files() {
            let path = file.path();
            let Some(name) = path.file_name() else {
                continue;
            };
            if name == "map.png" {
                continue;
            }

            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let Some(key) = parse_texture_key(&stem) else {
                continue;
            };

            let image = image::load_from_memory(file.contents())
                .unwrap_or_else(|_| panic!("Failed to decode embedded texture: {stem}"))
                .to_rgba8();
            entries.push((key, image));
        }

        entries.sort_by_key(|(key, _)| texture_sort_key(*key));

        let mut textures = Vec::with_capacity(entries.len());
        let mut keys_by_index = Vec::with_capacity(entries.len());
        let mut index_by_key = HashMap::with_capacity(entries.len());

        for (index, (key, image)) in entries.into_iter().enumerate() {
            if index_by_key.insert(key, index).is_some() {
                panic!("Duplicate texture asset key loaded: {:?}", key);
            }
            keys_by_index.push(key);
            textures.push(image);
        }

        let manager = Self {
            textures,
            keys_by_index,
            index_by_key,
        };
        manager.validate_required_assets();
        manager
    }

    pub fn images(&self) -> &[image::RgbaImage] {
        &self.textures
    }

    pub fn image(&self, key: TextureKey) -> &image::RgbaImage {
        let index = self.texture_index(key);
        &self.textures[index]
    }

    pub fn image_by_index(&self, index: usize) -> &image::RgbaImage {
        &self.textures[index]
    }

    pub fn texture_index(&self, key: TextureKey) -> usize {
        self.index_by_key
            .get(&key)
            .copied()
            .unwrap_or_else(|| panic!("Missing texture asset for key: {:?}", key))
    }

    pub fn key_at_index(&self, index: usize) -> TextureKey {
        self.keys_by_index[index]
    }

    pub fn wall_texture(&self, tile: u8) -> TextureKey {
        match tile {
            1 => TextureKey::Wall(WallTexture::Green),
            2 => TextureKey::Wall(WallTexture::Orange),
            3 => TextureKey::Wall(WallTexture::Grey),
            _ => panic!("No wall texture defined for tile value {tile}"),
        }
    }

    fn validate_required_assets(&self) {
        for key in [
            TextureKey::Wall(WallTexture::Green),
            TextureKey::Wall(WallTexture::Orange),
            TextureKey::Wall(WallTexture::Grey),
            TextureKey::Floor(FloorTexture::Smooth),
            TextureKey::Floor(FloorTexture::MilkVeins),
            TextureKey::Item(ItemTexture::Health),
            TextureKey::Actor(ActorTexture::Red),
            TextureKey::Projectile(ProjectileTexture::Spiral),
        ] {
            let _ = self.texture_index(key);
        }
    }
}

pub fn visual_definition(visual: VisualId) -> VisualDefinition {
    match visual {
        VisualId::PlayerActor => VisualDefinition {
            texture: TextureKey::Actor(ActorTexture::Red),
            billboard_width: 0.85,
            billboard_height: 0.85,
            facing_mode: FacingMode::Movement,
            animation: AnimationStyle::WalkPingPong,
        },
        VisualId::StaticProp => VisualDefinition {
            texture: TextureKey::Wall(WallTexture::Grey),
            billboard_width: 0.9,
            billboard_height: 0.9,
            facing_mode: FacingMode::Fixed,
            animation: AnimationStyle::Static,
        },
        VisualId::Pickup => VisualDefinition {
            texture: TextureKey::Item(ItemTexture::Health),
            billboard_width: 0.4,
            billboard_height: 0.4,
            facing_mode: FacingMode::Fixed,
            animation: AnimationStyle::Static,
        },
        VisualId::Projectile => VisualDefinition {
            texture: TextureKey::Projectile(ProjectileTexture::Spiral),
            billboard_width: 0.35,
            billboard_height: 0.35,
            facing_mode: FacingMode::Fixed,
            animation: AnimationStyle::LoopStrip,
        },
    }
}

pub fn animation_descriptor(style: AnimationStyle) -> Option<AnimationDescriptor> {
    match style {
        AnimationStyle::Static => None,
        AnimationStyle::WalkPingPong => Some(AnimationDescriptor {
            frame_width: 64,
            frame_height: 64,
            columns: 3,
            rows: 4,
            ms_per_frame: 90.0,
            playback: AnimationPlayback::PingPong,
            directional_rows: true,
        }),
        AnimationStyle::LoopStrip => Some(AnimationDescriptor {
            frame_width: 16,
            frame_height: 16,
            columns: 4,
            rows: 1,
            ms_per_frame: 90.0,
            playback: AnimationPlayback::Loop,
            directional_rows: false,
        }),
    }
}

fn parse_texture_key(stem: &str) -> Option<TextureKey> {
    match stem {
        "wall.green" => Some(TextureKey::Wall(WallTexture::Green)),
        "wall.orange" => Some(TextureKey::Wall(WallTexture::Orange)),
        "wall.grey" => Some(TextureKey::Wall(WallTexture::Grey)),
        "floor.smooth" => Some(TextureKey::Floor(FloorTexture::Smooth)),
        "floor.milkveins" => Some(TextureKey::Floor(FloorTexture::MilkVeins)),
        "item.health" => Some(TextureKey::Item(ItemTexture::Health)),
        "actor.red" => Some(TextureKey::Actor(ActorTexture::Red)),
        "projectile.spiral" => Some(TextureKey::Projectile(ProjectileTexture::Spiral)),
        _ => None,
    }
}

fn texture_sort_key(key: TextureKey) -> (u8, u8) {
    match key {
        TextureKey::Wall(WallTexture::Green) => (0, 0),
        TextureKey::Wall(WallTexture::Orange) => (0, 1),
        TextureKey::Wall(WallTexture::Grey) => (0, 2),
        TextureKey::Floor(FloorTexture::Smooth) => (1, 0),
        TextureKey::Floor(FloorTexture::MilkVeins) => (1, 1),
        TextureKey::Item(ItemTexture::Health) => (2, 0),
        TextureKey::Actor(ActorTexture::Red) => (3, 0),
        TextureKey::Projectile(ProjectileTexture::Spiral) => (4, 0),
    }
}

mod tests {
    #[test]
    fn test_load_texture_manager() {
        let textures = super::TextureManager::load();
        for (i, texture) in textures.images().iter().enumerate() {
            println!(
                "Loaded texture: {} ({}x{})",
                i,
                texture.width(),
                texture.height()
            );
        }
    }
}
