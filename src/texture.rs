pub fn load_textures(directory: &str) -> Vec<image::RgbaImage> {
    let paths = std::fs::read_dir(directory).expect("Failed to read textures directory");

    let mut entries: Vec<(usize, image::RgbaImage)> = Vec::new();
    for path in paths {
        let path = path.expect("Failed to read texture file").path();
        if path.is_file() {
            if let Some(name) = path.file_name() {
                if name != "map.png" {
                    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                    if let Ok(index) = stem.parse::<usize>() {
                        let img = image::open(&path)
                            .expect(&format!("Failed to open texture: {}", stem))
                            .to_rgba8();
                        entries.push((index, img));
                    }
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
        let textures = super::load_textures("textures");
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
