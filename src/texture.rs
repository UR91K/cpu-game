use std::collections::HashMap;

pub fn load_textures(directory: &str) -> HashMap<String, image::RgbImage> {
    let mut textures: HashMap<String, image::RgbImage> = HashMap::new();
    let paths = std::fs::read_dir(directory).expect("Failed to read textures directory");

    for path in paths {
        let path = path.expect("Failed to read texture file").path();
        if path.is_file() {
            if let Some(name) = path.file_name() {
                if name != "map.png" {
                    let texture_name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let img = image::open(&path)
                        .expect(&format!("Failed to open texture: {}", texture_name))
                        .to_rgb8();
                    textures.insert(texture_name, img);
                }
            }
        }
    }
    textures
}

mod tests {
    #[test]
    fn test_load_textures() {
        let textures = super::load_textures("textures");
        for (name, texture) in &textures {
            println!("Loaded texture: {} ({}x{})", name, texture.width(), texture.height());
        }
    }
}