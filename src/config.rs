use egui::{Color32, Vec2, vec2};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct CanvasConfig {
    pub base_gate_size: Vec2,
    pub base_pin_size: f32,
    pub base_input_pin_color: Color32,
    pub base_output_pin_color: Color32,
    pub wire_thickness: f32,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            base_gate_size: vec2(85.0, 75.0),
            base_pin_size: 4.5,
            base_input_pin_color: Color32::RED,
            base_output_pin_color: Color32::GREEN,
            wire_thickness: 6.0,
        }
    }
}
