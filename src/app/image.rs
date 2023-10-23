use eframe::epaint::{Vec2, TextureHandle, Color32};

pub fn scale_image_size(original: Vec2, max_x: f32) -> Vec2 {
    let factor = max_x / original.x;
    original * factor
}

pub fn selection_tint_color(page_i: usize) -> Color32 {
    #[allow(clippy::cast_possible_truncation)]
    Color32::from_rgba_premultiplied(255 - ((page_i+1) * 50) as u8, 255 - ((page_i+1) * 50) as u8, 255, 50)
}

pub struct ScanEntry {
    pub pixels: Vec<u8>,
    pub texture_handle: TextureHandle,
    pub selected_as_page: Option<usize>,
    pub saved_to_file: bool,
}