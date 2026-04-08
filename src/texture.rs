use include_dir::{include_dir, Dir};

static TEXTURES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/textures");

pub fn load_textures() -> Vec<image::RgbaImage> {
    let files = TEXTURES_DIR.files();

    let mut entries: Vec<(usize, image::RgbaImage)> = Vec::new();
    for file in files {
        let path = file.path();
        if let Some(name) = path.file_name() {
            if name != "map.png" {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                if let Ok(index) = stem.parse::<usize>() {
                    let img = image::load_from_memory(file.contents())
                        .unwrap_or_else(|_| panic!("Failed to decode embedded texture: {}", stem))
                        .to_rgba8();
                    entries.push((index, img));
                }
            }
        }
    }

    entries.sort_by_key(|(i, _)| *i);
    let max_index = entries.iter().map(|(i, _)| *i).max().unwrap_or(0);
    let mut textures: Vec<Option<image::RgbaImage>> = (0..=max_index).map(|_| None).collect();
    for (i, img) in entries {
        textures[i] = Some(img);
    }
    textures
        .into_iter()
        .map(|t| t.expect("Missing texture index"))
        .collect()
}

mod tests {
    #[test]
    fn test_load_textures() {
        let textures = super::load_textures();
        for (i, texture) in textures.iter().enumerate() {
            println!(
                "Loaded texture: {} ({}x{})",
                i,
                texture.width(),
                texture.height()
            );
        }
    }
}
