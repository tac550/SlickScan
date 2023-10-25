use eframe::epaint::{Vec2, TextureHandle, Color32};

pub fn scale_image_size(original: Vec2, max_x: f32) -> Vec2 {
    let factor = max_x / original.x;
    original * factor
}

pub fn selection_tint_color(page_i: usize, total_selected: usize) -> Color32 {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    let blueness = if let 1 = total_selected {
        255.0
    } else {
        (((page_i + 1) as f32) / (total_selected as f32)) * 255.0
    } as u8;
    Color32::from_rgba_premultiplied(255 - blueness, 255 - blueness, 255, 50)
}

pub struct ScanEntry {
    pub pixels: Vec<u8>,
    pub texture_handle: TextureHandle,
    pub selected_as_page: Option<usize>,
    pub saved_to_file: bool,
}