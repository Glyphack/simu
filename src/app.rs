use std::collections::HashSet;
use std::fmt::Write as _;
use std::hash::Hash;

use egui::{
    Align, Button, Color32, CornerRadius, Image, Layout, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget as _, pos2, vec2,
};
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    assets::{self},
    config::CanvasConfig,
    connection_manager::ConnectionManager,
    custom_circuit::{self, CustomCircuit, CustomCircuitDefinition},
    drag::Drag,
};

pub const PANEL_BUTTON_MAX_HEIGHT: f32 = 70.0;

// Grid
pub const GRID_SIZE: f32 = 20.0;
pub const COLOR_GRID_LIGHT: Color32 = Color32::from_rgb(230, 230, 230);
pub const COLOR_GRID_DARK: Color32 = Color32::from_rgb(40, 40, 40);

pub const COLOR_PIN_DETACH_HINT: Color32 = Color32::RED;
pub const COLOR_PIN_POWERED_OUTLINE: Color32 = Color32::GREEN;
pub const COLOR_WIRE_POWERED: Color32 = Color32::GREEN;
pub const COLOR_WIRE_IDLE: Color32 = Color32::LIGHT_BLUE;
// Hover
pub const COLOR_WIRE_HOVER: Color32 = Color32::GRAY;
pub const COLOR_HOVER_INSTANCE_OUTLINE: Color32 = Color32::GRAY;
pub const COLOR_HOVER_PIN_TO_WIRE: Color32 = Color32::GRAY;
pub const COLOR_HOVER_PIN_DETACH: Color32 = Color32::RED;
pub const PIN_HOVER_THRESHOLD: f32 = 8.0;

pub const NEW_PIN_ON_WIRE_THRESHOLD: f32 = 10.0;

// Connections
pub const COLOR_POTENTIAL_CONN_HIGHLIGHT: Color32 = Color32::LIGHT_BLUE;
pub const WIRE_HIT_DISTANCE: f32 = 10.0;
pub const SNAP_THRESHOLD: f32 = 10.0;
pub const PIN_MOVE_HINT_D: f32 = 10.0;
pub const PIN_MOVE_HINT_COLOR: Color32 = Color32::GRAY;

pub const COLOR_SELECTION_HIGHLIGHT: Color32 = Color32::GRAY;
pub const COLOR_SELECTION_BOX: Color32 = Color32::LIGHT_BLUE;

pub const MIN_WIRE_SIZE: f32 = 40.0;

slotmap::new_key_type! {
    pub struct InstanceId;
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum Hover {
    Pin(Pin),
    Instance(InstanceId),
}

impl Hover {
    pub fn instance(&self) -> InstanceId {
        match self {
            Self::Pin(pin) => pin.ins,
            Self::Instance(instance_id) => *instance_id,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstancePosOffset {
    Gate(GateKind, Vec2),
    Power(Vec2),
    Wire(Vec2, Vec2),
    // Index to definition
    CustomCircuit(usize, Vec2),
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceKind {
    Gate(GateKind),
    Power,
    Wire,
    CustomCircuit(usize),
}

// A specific pin on an instance
#[derive(
    serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd,
)]
pub struct Pin {
    pub ins: InstanceId,
    pub index: u32,
}

impl Pin {}

// A normalized, order-independent connection between two pins
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq)]
pub struct Connection {
    pub a: Pin,
    pub b: Pin,
}

impl Connection {
    pub fn new(p1: Pin, p2: Pin) -> Self {
        Self { a: p2, b: p1 }
    }

    pub fn involves_instance(&self, id: InstanceId) -> bool {
        self.a.ins == id || self.b.ins == id
    }

    fn get_pin_first(&self, pin: Pin) -> Option<(Pin, Pin)> {
        if self.a == pin {
            Some((self.a, self.b))
        } else if self.b == pin {
            Some((self.b, self.a))
        } else {
            None
        }
    }
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a && self.b == other.b
    }
}

impl Hash for Connection {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.a.hash(state);
        self.b.hash(state);
    }
}

// Gate

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    // Center position
    pub pos: Pos2,
    pub kind: GateKind,
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum GateKind {
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
}

impl GateKind {
    pub fn graphics(&self) -> &assets::InstanceGraphics {
        match self {
            Self::Nand => &assets::NAND_GRAPHICS,
            Self::And => &assets::AND_GRAPHICS,
            Self::Or => &assets::OR_GRAPHICS,
            Self::Nor => &assets::NOR_GRAPHICS,
            Self::Xor => &assets::XOR_GRAPHICS,
            Self::Xnor => &assets::XNOR_GRAPHICS,
        }
    }
}

// Gate end

// Power

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Power {
    // Center position
    pub pos: Pos2,
    pub on: bool,
}

impl Power {
    fn graphics(&self) -> &assets::InstanceGraphics {
        if self.on {
            &assets::POWER_ON_GRAPHICS
        } else {
            &assets::POWER_OFF_GRAPHICS
        }
    }
}

// Power end

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Wire {
    pub start: Pos2,
    pub end: Pos2,
}

impl Wire {
    pub fn new_at(pos: Pos2) -> Self {
        Self::new(pos2(pos.x - 30.0, pos.y), pos2(pos.x + 30.0, pos.y))
    }
    pub fn new(start: Pos2, end: Pos2) -> Self {
        Self { start, end }
    }

    pub fn closest_point_on_line(&self, p: Pos2) -> Pos2 {
        let a = self.start;
        let b = self.end;
        let ab: Vec2 = b - a;
        let ap: Vec2 = p - a;

        let ab_len2 = ab.x * ab.x + ab.y * ab.y;
        if ab_len2 == 0.0 {
            return a;
        }

        let t = ((ap.x * ab.x + ap.y * ab.y) / ab_len2).clamp(0.0, 1.0);

        a + ab * t
    }

    pub fn dist_to_closest_point_on_line(&self, p: Pos2) -> f32 {
        let closest = self.closest_point_on_line(p);
        (p - closest).length()
    }

    pub fn center(&self) -> Pos2 {
        pos2(
            (self.start.x + self.end.x) * 0.5,
            (self.start.y + self.end.y) * 0.5,
        )
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
pub struct DB {
    // Primary key allocator; ensures unique keys across all instance kinds
    pub instances: SlotMap<InstanceId, ()>,
    // Type registry for each instance id
    pub types: SecondaryMap<InstanceId, InstanceKind>,
    // Per-kind payloads keyed off the primary key space
    pub gates: SecondaryMap<InstanceId, Gate>,
    pub powers: SecondaryMap<InstanceId, Power>,
    pub wires: SecondaryMap<InstanceId, Wire>,
    pub custom_circuits: SecondaryMap<InstanceId, CustomCircuit>,
    // Definition of custom circuits created by the user
    pub custom_circuit_definitions: Vec<CustomCircuitDefinition>,
    pub connections: HashSet<Connection>,
}

impl DB {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_gate(&mut self, g: Gate) -> InstanceId {
        let k = self.instances.insert(());
        self.gates.insert(k, g);
        let kind = self
            .gates
            .get(k)
            .expect("gate must exist right after insertion")
            .kind;
        self.types.insert(k, InstanceKind::Gate(kind));
        k
    }

    pub fn new_power(&mut self, p: Power) -> InstanceId {
        let k = self.instances.insert(());
        self.powers.insert(k, p);
        self.types.insert(k, InstanceKind::Power);
        k
    }

    pub fn new_wire(&mut self, w: Wire) -> InstanceId {
        let k = self.instances.insert(());
        self.wires.insert(k, w);
        self.types.insert(k, InstanceKind::Wire);
        k
    }

    pub fn new_custom_circuit(&mut self, c: crate::custom_circuit::CustomCircuit) -> InstanceId {
        let k = self.instances.insert(());
        let definition_index = c.definition_index;
        self.custom_circuits.insert(k, c);
        self.types
            .insert(k, InstanceKind::CustomCircuit(definition_index));
        k
    }

    pub fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("instance type missing for id: {id:?}"))
    }

    pub fn get_gate(&self, id: InstanceId) -> &Gate {
        self.gates.get(id).expect("gate not found")
    }

    pub fn get_gate_mut(&mut self, id: InstanceId) -> &mut Gate {
        self.gates.get_mut(id).expect("gate not found (mut)")
    }

    pub fn get_power(&self, id: InstanceId) -> &Power {
        self.powers.get(id).expect("power not found")
    }

    pub fn get_power_mut(&mut self, id: InstanceId) -> &mut Power {
        self.powers.get_mut(id).expect("power not found (mut)")
    }

    pub fn get_wire(&self, id: InstanceId) -> &Wire {
        self.wires.get(id).expect("wire not found")
    }

    pub fn get_wire_mut(&mut self, id: InstanceId) -> &mut Wire {
        self.wires.get_mut(id).expect("wire not found (mut)")
    }

    pub fn get_custom_circuit(&self, id: InstanceId) -> &crate::custom_circuit::CustomCircuit {
        self.custom_circuits
            .get(id)
            .expect("custom circuit not found")
    }

    pub fn get_custom_circuit_mut(
        &mut self,
        id: InstanceId,
    ) -> &mut crate::custom_circuit::CustomCircuit {
        self.custom_circuits
            .get_mut(id)
            .expect("custom circuit not found (mut)")
    }

    pub fn pins_of(&self, id: InstanceId) -> Vec<Pin> {
        match self.ty(id) {
            InstanceKind::Gate(gk) => {
                let n = gk.graphics().pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Power => {
                let n = assets::POWER_ON_GRAPHICS.pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Wire => vec![Pin { ins: id, index: 0 }, Pin { ins: id, index: 1 }],
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(id);
                if cc.definition_index < self.custom_circuit_definitions.len() {
                    let def = &self.custom_circuit_definitions[cc.definition_index];
                    (0..def.external_pins.len() as u32)
                        .map(|i| Pin { ins: id, index: i })
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub fn pin_position(&self, pin: Pin) -> Pos2 {
        match self.ty(pin.ins) {
            InstanceKind::Gate(gk) => {
                let g = self.get_gate(pin.ins);
                let info = gk.graphics().pins[pin.index as usize];
                g.pos + info.offset
            }
            InstanceKind::Power => {
                let p = self.get_power(pin.ins);
                let info = p.graphics().pins[pin.index as usize];
                p.pos + info.offset
            }
            InstanceKind::Wire => {
                let w = self.get_wire(pin.ins);
                if pin.index == 0 { w.start } else { w.end }
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(pin.ins);
                cc.pos + self.pin_offset(pin)
            }
        }
    }

    pub fn pin_offset(&self, pin: Pin) -> Vec2 {
        match self.ty(pin.ins) {
            InstanceKind::Gate(gk) => {
                let info = gk.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::Power => {
                let p = self.get_power(pin.ins);
                let info = p.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::Wire => {
                let w = self.get_wire(pin.ins);
                let center = w.center();
                if pin.index == 0 {
                    center - w.start
                } else {
                    center - w.end
                }
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(pin.ins);
                let def = &self.custom_circuit_definitions[cc.definition_index];
                def.external_pins[pin.index as usize].offset
            }
        }
    }

    pub fn connected_pins_of_instance(&self, id: InstanceId) -> Vec<Pin> {
        let mut out = Vec::new();
        for c in &self.connections {
            if c.a.ins == id {
                out.push(c.b);
            } else if c.b.ins == id {
                out.push(c.a);
            }
        }
        out
    }

    pub fn connected_insntances(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut out = vec![id];
        for c in &self.connections {
            if c.a.ins == id {
                out.push(c.b.ins);
            } else if c.b.ins == id {
                out.push(c.a.ins);
            }
        }
        out
    }

    pub fn move_nonwires_and_resize_wires(&mut self, ids: &[InstanceId], delta: Vec2) {
        // Move all non-wire instances, then adjust connected wire endpoints
        for id in ids {
            match self.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.get_gate_mut(*id);
                    g.pos += delta;
                }
                InstanceKind::Power => {
                    let p = self.get_power_mut(*id);
                    p.pos += delta;
                }
                InstanceKind::Wire => {}
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.get_custom_circuit_mut(*id);
                    cc.pos += delta;
                }
            }
        }

        // Resize wire endpoints attached to any moved instance
        for id in ids {
            for pin in self.connected_pins_of_instance(*id) {
                if matches!(self.ty(pin.ins), InstanceKind::Wire) {
                    let w = self.get_wire_mut(pin.ins);
                    if pin.index == 0 {
                        w.start += delta;
                    } else {
                        w.end += delta;
                    }
                }
            }
        }
    }

    pub fn move_instance_and_propagate(&mut self, id: InstanceId, delta: Vec2) {
        let mut visited = HashSet::new();
        self.move_instance_and_propagate_recursive(id, delta, &mut visited);
    }

    fn move_instance_and_propagate_recursive(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        visited: &mut HashSet<InstanceId>,
    ) {
        if !visited.insert(id) {
            return;
        }

        // Move this instance
        match self.ty(id) {
            InstanceKind::Gate(_) => {
                let g = self.get_gate_mut(id);
                g.pos += delta;
            }
            InstanceKind::Power => {
                let p = self.get_power_mut(id);
                p.pos += delta;
            }
            InstanceKind::Wire => {
                let w = self.get_wire_mut(id);
                w.start += delta;
                w.end += delta;
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit_mut(id);
                cc.pos += delta;
            }
        }

        // Get connected instances before we recurse
        let connected = self.connected_insntances(id);

        // Process each connected instance
        for connected_id in connected {
            if connected_id == id || visited.contains(&connected_id) {
                continue;
            }

            match self.ty(connected_id) {
                InstanceKind::Wire => {
                    // For wires, resize them to stay connected
                    // Find which pin of the wire is connected to our moved instance
                    let wire_pins = self.pins_of(connected_id);
                    for wire_pin in wire_pins {
                        // Check if this wire pin is connected to any pin of our moved instance
                        for moved_pin in self.pins_of(id) {
                            if self
                                .connections
                                .contains(&Connection::new(wire_pin, moved_pin))
                            {
                                // Update the wire endpoint to match the new pin position
                                let new_pin_pos = self.pin_position(moved_pin);
                                let w = self.get_wire_mut(connected_id);
                                if wire_pin.index == 0 {
                                    w.start = new_pin_pos;
                                } else {
                                    w.end = new_pin_pos;
                                }
                            }
                        }
                    }
                    // Mark as visited but don't propagate further (wires are endpoints)
                    visited.insert(connected_id);
                }
                InstanceKind::Gate(_) | InstanceKind::Power | InstanceKind::CustomCircuit(_) => {
                    // For non-wires, propagate the same delta
                    self.move_instance_and_propagate_recursive(connected_id, delta, visited);
                }
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct App {
    pub canvas_config: CanvasConfig,
    pub drag: Option<Drag>,
    pub hovered: Option<Hover>,
    pub db: DB,
    // connection manager for handling spatial indexing and validation
    #[serde(skip)]
    pub connection_manager: ConnectionManager,
    // possible connections while dragging
    pub potential_connections: HashSet<Connection>,
    // energized pins based on current simulation
    pub current: HashSet<Pin>,
    // mark when current needs recomputation
    pub current_dirty: bool,
    pub show_debug: bool,
    // selection set and move preview
    pub selected: HashSet<InstanceId>,
    pub clicked_on: Option<InstanceId>,
    pub drag_had_movement: bool,
    //Copied. Items with their offset compared to a middle point in the rectangle
    pub clipboard: Vec<InstancePosOffset>,
    // Where are we in the world
    pub viewport_offset: Vec2,
    // For web load functionality - stores pending JSON to load
    #[serde(skip)]
    pub pending_load_json: Option<String>,
    #[serde(skip)]
    pub panning: bool,
    #[serde(skip)]
    pub panel_width: f32,
    // Context menu state
    #[serde(skip)]
    pub show_right_click_actions_menu: bool,
    #[serde(skip)]
    pub context_menu_pos: Pos2,
}

impl Default for App {
    fn default() -> Self {
        let db = DB::default();
        let c = ConnectionManager::new(&db);
        Self {
            db,
            canvas_config: Default::default(),
            drag: Default::default(),
            hovered: Default::default(),
            connection_manager: c,
            potential_connections: Default::default(),
            current: Default::default(),
            current_dirty: true,
            show_debug: true,
            selected: Default::default(),
            clicked_on: Default::default(),
            drag_had_movement: false,
            clipboard: Default::default(),
            pending_load_json: None,
            viewport_offset: Vec2::ZERO,
            panning: false,
            panel_width: 0.0,
            show_right_click_actions_menu: false,
            context_menu_pos: Pos2::ZERO,
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");

                ui.menu_button("File", |ui| {
                    if ui.button("Save Circuit").clicked()
                        && let Err(e) = self.save_to_file()
                    {
                        log::error!("Failed to save circuit: {e}");
                    }
                    if ui.button("Load Circuit").clicked()
                        && let Err(e) = self.load_from_file()
                    {
                        log::error!("Failed to load circuit: {e}");
                    }
                    if !is_web {
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
                ui.add_space(16.0);

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_debug, "World Debug");
                });
                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main(ui);
        });
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    pub fn screen_to_world(&self, pos: Pos2) -> Pos2 {
        pos + self.viewport_offset
    }

    pub fn draw_main(&mut self, ui: &mut Ui) {
        self.process_pending_load();

        if self.show_debug {
            egui::Window::new("Debug logs").show(ui.ctx(), |ui| {
                egui_logger::logger_ui().show(ui);
            });
        }
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.canvas_config = CanvasConfig::default();
            if self.show_debug {
                let full_h = ui.available_height();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut dbg = self.debug_string(ui);
                    ui.add_sized(vec2(320.0, full_h), egui::TextEdit::multiline(&mut dbg));
                });
            }

            let panel_rect = ui
                .vertical(|ui| {
                    ui.heading("Logic Gates");
                    self.draw_panel(ui);
                })
                .response
                .rect;
            self.panel_width = panel_rect.width();
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Canvas");
                ui.label("press backspace/d to remove object");
                ui.label("right click on powers to toggle");
                ui.label("right click on canvas to drag");
                self.draw_canvas(ui);
            });
        });
    }

    fn draw_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([true; 2])
            .show(ui, |ui| {
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::And));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nand));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Or));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xnor));
                self.draw_panel_button(ui, InstanceKind::Power);
                self.draw_panel_button(ui, InstanceKind::Wire);

                if !self.db.custom_circuit_definitions.is_empty() {
                    ui.add_space(8.0);
                    ui.label("Custom Circuits:");
                }
                let custom_circuit_indices: Vec<usize> =
                    (0..self.db.custom_circuit_definitions.len()).collect();
                for i in custom_circuit_indices {
                    self.draw_panel_button(ui, InstanceKind::CustomCircuit(i));
                }

                ui.add_space(8.0);

                if Button::new("Clear Canvas")
                    .min_size(vec2(48.0, 30.0))
                    .ui(ui)
                    .clicked()
                {
                    self.db = DB::default();
                    self.hovered = None;
                    self.selected.clear();
                    self.drag = None;
                    self.current.clear();
                    self.current_dirty = false;
                    self.connection_manager = ConnectionManager::new(&self.db);
                }
            });
    }

    fn draw_panel_button(&mut self, ui: &mut Ui, kind: InstanceKind) -> Response {
        let resp = match kind {
            InstanceKind::Gate(gate_kind) => {
                let s = get_icon(ui, gate_kind.graphics().svg.clone())
                    .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Power => {
                let s = get_icon(
                    ui,
                    Power {
                        pos: Pos2::ZERO,
                        on: true,
                    }
                    .graphics()
                    .svg
                    .clone(),
                )
                .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Wire => ui.add(
                Button::new("Wire")
                    .sense(Sense::click_and_drag())
                    .min_size(vec2(78.0, 30.0)),
            ),
            InstanceKind::CustomCircuit(_) => ui.add(
                Button::new("Custom")
                    .sense(Sense::hover())
                    .min_size(vec2(78.0, 30.0)),
            ),
        };
        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));

        if resp.drag_started()
            && let Some(pos) = mouse_pos_world
        {
            let id = match kind {
                InstanceKind::Gate(kind) => self.db.new_gate(Gate { pos, kind }),
                InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                InstanceKind::Wire => self.db.new_wire(Wire::new_at(pos)),
                InstanceKind::CustomCircuit(c) => self.db.new_custom_circuit(CustomCircuit {
                    pos,
                    definition_index: c,
                }),
            };
            self.drag = Some(Drag::Canvas(crate::drag::CanvasDrag::Single {
                id,
                offset: Vec2::ZERO,
            }));
        }
        ui.add_space(8.0);

        resp
    }

    fn handle_panning(
        &mut self,
        ui: &Ui,
        right_down: bool,
        right_released: bool,
        mouse_is_visible: bool,
    ) {
        if right_down && self.hovered.is_none() {
            self.panning = true;
        }

        if right_released || !mouse_is_visible {
            self.panning = false;
        }

        if self.panning {
            self.viewport_offset += ui.input(|i| i.pointer.delta());
        }
    }

    fn handle_copy_pasting(&mut self, ui: &Ui, mouse_pos_world: Option<Pos2>) {
        let mut copy_event_detected = false;
        let mut paste_event_detected = false;
        ui.ctx().input(|i| {
            for event in &i.events {
                if matches!(event, egui::Event::Copy) {
                    log::info!("Copy detected");
                    copy_event_detected = true;
                }
                if let egui::Event::Paste(_) = event {
                    // TODO(paste-json): If user pasted json convert it to gates and add it.
                    paste_event_detected = true;
                }
            }
        });

        if copy_event_detected && !self.selected.is_empty() {
            self.copy_to_clipboard();
        }

        if paste_event_detected
            && !self.clipboard.is_empty()
            && let Some(mouse) = mouse_pos_world
        {
            self.paste_from_clipboard(mouse);
        }
    }

    fn handle_deletion(&mut self, ui: &Ui) {
        let bs_pressed = ui.input(|i| i.key_pressed(egui::Key::Backspace));
        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));

        if bs_pressed || d_pressed {
            if let Some(id) = self.hovered.take() {
                let id = match id {
                    Hover::Pin(pin) => pin.ins,
                    Hover::Instance(instance_id) => instance_id,
                };
                self.delete_instance(id);
            } else if self.hovered.is_none() && !self.selected.is_empty() {
                // Collect Iavoid mutable borrow conflict
                let ids_to_delete: Vec<InstanceId> = self.selected.drain().collect();
                for id in ids_to_delete {
                    self.delete_instance(id);
                }
            }
        }
    }

    pub fn delete_instance(&mut self, id: InstanceId) {
        self.db.instances.remove(id);
        self.db.types.remove(id);
        self.db.gates.remove(id);
        self.db.powers.remove(id);
        self.db.wires.remove(id);
        self.db.custom_circuits.remove(id);
        self.db.connections.retain(|c| !c.involves_instance(id));
        self.hovered.take();
        self.drag.take();
        self.selected.remove(&id);

        // Remove from connection manager tracking
        self.connection_manager.dirty_instances.remove(&id);
        self.current.retain(|p| p.ins != id);
        self.connection_manager.rebuild_spatial_index(&self.db);
    }

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas_rect = resp.rect;

        // Set clip rectangle to prevent canvas objects from drawing outside canvas bounds
        ui.set_clip_rect(canvas_rect);

        Self::draw_grid(ui, canvas_rect, self.viewport_offset);

        let mouse_up = ui.input(|i| i.pointer.any_released());
        let mouse_clicked = ui.input(|i| i.pointer.primary_down());
        let right_released = ui.input(|i| i.pointer.secondary_released());
        let right_down = ui.input(|i| i.pointer.secondary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));
        let mouse_is_visible = ui.ctx().input(|i| i.pointer.has_pointer());

        self.handle_panning(ui, right_down, right_released, mouse_is_visible);
        self.handle_copy_pasting(ui, mouse_pos_world);
        self.handle_deletion(ui);

        if let Some(mouse) = mouse_pos_world {
            let dragging = self.drag.is_some();
            let hovered_now = self.get_hovered(mouse);

            if mouse_clicked && self.drag.is_none() {
                self.hovered = hovered_now;
                self.clicked_on = hovered_now.map(|h| h.instance());
                self.handle_drag_start_canvas(mouse);
            }

            if dragging {
                self.handle_dragging(ui, mouse);
            } else {
                self.hovered = hovered_now;
            }

            if mouse_up {
                if dragging {
                    let drag_had_movement = self.drag_had_movement;
                    self.handle_drag_end(mouse);

                    if self.connection_manager.update_connections(&mut self.db) {
                        self.current_dirty = true;
                    }
                    if !drag_had_movement {
                        self.selected.clear();
                        if let Some(Hover::Instance(id)) = hovered_now {
                            self.selected.insert(id);
                        }
                    }
                } else {
                    self.selected.clear();
                    if let Some(Hover::Instance(id)) = hovered_now {
                        self.selected.insert(id);
                    }
                }
                self.clicked_on = None;
                self.hovered = hovered_now;
            }
        }
        if self.selected.len() == 1 {
            self.highlight_selected_actions(ui, mouse_pos_world, mouse_clicked);
        }

        // Toggle power
        if right_clicked
            && let Some(id) = self.hovered.as_ref().map(|i| i.instance())
            && matches!(self.db.ty(id), InstanceKind::Power)
        {
            let p = self.db.get_power_mut(id);
            p.on = !p.on;
            self.current_dirty = true;
        }

        // Handle context menu for selected components
        // TODO: Custom circuits
        // if right_clicked
        //     && !self.selected.is_empty()
        //     && let Some(mouse_world) = mouse_pos_world
        // {
        //     self.show_right_click_actions_menu = true;
        //     self.context_menu_pos = mouse_world - self.viewport_offset; // Convert to screen coordinates
        // }

        if self.current_dirty {
            self.recompute_current();
        }

        // Draw world
        for (id, gate) in &self.db.gates {
            self.draw_gate(ui, id, gate);
        }
        for (id, power) in &self.db.powers {
            self.draw_power(ui, id, power);
        }
        for (id, custom_circuit) in &self.db.custom_circuits {
            self.draw_custom_circuit(ui, id, custom_circuit);
        }
        for (id, wire) in &self.db.wires {
            let has_current = self.current.contains(&Pin { ins: id, index: 0 });
            self.draw_wire(
                ui,
                wire,
                self.hovered
                    .as_ref()
                    .is_some_and(|f| matches!(f, Hover::Instance(_)) && f.instance() == id),
                has_current,
            );
        }

        for c in &self.potential_connections {
            // Highlight the pin that it's going to attach. The stable pin.
            let pin_to_highlight = c.b;
            let p = self.db.pin_position(pin_to_highlight);
            ui.painter().circle_filled(
                p - self.viewport_offset,
                SNAP_THRESHOLD,
                COLOR_POTENTIAL_CONN_HIGHLIGHT,
            );
        }

        if self.drag.is_none() {
            self.highlight_hovered(ui);
        }
        self.draw_selection_highlight(ui);

        self.draw_right_click_actions_menu(ui);

        // Preview wire branching
        if !self.selected.is_empty()
            && let Some(hovered) = self.hovered
            && let Some(mouse) = mouse_pos_world
            && self.drag.is_none()
            && let Hover::Instance(instance_id) = hovered
            && let Some(split_point) = self.wire_branching_action_point(mouse, instance_id)
        {
            ui.painter().circle_filled(
                split_point - self.viewport_offset,
                PIN_HOVER_THRESHOLD,
                COLOR_HOVER_PIN_TO_WIRE,
            );
        }
    }

    fn draw_grid(ui: &Ui, canvas_rect: Rect, viewport_offset: Vec2) {
        let grid_color = if ui.visuals().dark_mode {
            COLOR_GRID_DARK
        } else {
            COLOR_GRID_LIGHT
        };

        let painter = ui.painter();

        // Draw vertical lines
        let start_x =
            (canvas_rect.left() / GRID_SIZE).floor() * GRID_SIZE - viewport_offset.x % GRID_SIZE;
        let mut x = start_x;
        while x <= canvas_rect.right() {
            if x >= canvas_rect.left() {
                painter.line_segment(
                    [pos2(x, canvas_rect.top()), pos2(x, canvas_rect.bottom())],
                    Stroke::new(1.0, grid_color),
                );
            }
            x += GRID_SIZE;
        }

        // Draw horizontal lines
        let start_y =
            (canvas_rect.top() / GRID_SIZE).floor() * GRID_SIZE - viewport_offset.y % GRID_SIZE;
        let mut y = start_y;
        while y <= canvas_rect.bottom() {
            if y >= canvas_rect.top() {
                painter.line_segment(
                    [pos2(canvas_rect.left(), y), pos2(canvas_rect.right(), y)],
                    Stroke::new(1.0, grid_color),
                );
            }
            y += GRID_SIZE;
        }
    }

    fn draw_instance_graphics<F>(
        &self,
        ui: &mut Ui,
        graphics: &assets::InstanceGraphics,
        screen_center: Pos2,
        highlight_pin: F,
    ) where
        F: Fn(usize) -> bool,
    {
        let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
        draw_icon_canvas(ui, graphics.svg.clone(), rect);

        for (i, pin) in graphics.pins.iter().enumerate() {
            let pin_offset = pin.offset;
            let pin_pos = screen_center + pin_offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);

            if highlight_pin(i) {
                ui.painter().circle_stroke(
                    pin_pos,
                    self.canvas_config.base_pin_size + 3.0,
                    Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                );
            }
        }
    }

    fn draw_gate(&self, ui: &mut Ui, id: InstanceId, gate: &Gate) {
        let screen_center = gate.pos - self.viewport_offset;
        self.draw_instance_graphics(ui, gate.kind.graphics(), screen_center, |pin_index| {
            self.current.contains(&Pin {
                ins: id,
                index: pin_index as u32,
            })
        });
    }

    pub fn draw_gate_preview(&self, ui: &mut Ui, gate_kind: GateKind, pos: Pos2) {
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, gate_kind.graphics(), screen_center, |_| false);
    }

    fn draw_power(&self, ui: &mut Ui, id: InstanceId, power: &Power) {
        let screen_center = power.pos - self.viewport_offset;
        self.draw_instance_graphics(ui, power.graphics(), screen_center, |pin_index| {
            self.current.contains(&Pin {
                ins: id,
                index: pin_index as u32,
            })
        });
    }

    pub fn draw_power_preview(&self, ui: &mut Ui, pos: Pos2) {
        let power = Power { pos, on: true };
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, power.graphics(), screen_center, |_| false);
    }

    fn draw_custom_circuit(
        &self,
        ui: &Ui,
        id: InstanceId,
        custom_circuit: &crate::custom_circuit::CustomCircuit,
    ) {
        let screen_center = custom_circuit.pos - self.viewport_offset;

        // Get the definition for this custom circuit
        if let Some(definition) = self
            .db
            .custom_circuit_definitions
            .get(custom_circuit.definition_index)
        {
            // Draw as a dark blue rectangle with the name
            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter()
                .rect_filled(rect, CornerRadius::default(), egui::Color32::DARK_BLUE);

            // Draw the name
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &definition.name,
                egui::FontId::default(),
                egui::Color32::WHITE,
            );

            // Draw external pins
            for (pin_index, ext_pin) in definition.external_pins.iter().enumerate() {
                let pin_world_pos = custom_circuit.pos + ext_pin.offset;
                let pin_screen_pos = pin_world_pos - self.viewport_offset;

                // Determine pin color based on whether it has current
                let has_current = self.current.contains(&Pin {
                    ins: id,
                    index: pin_index as u32,
                });

                let pin_color = match ext_pin.kind {
                    crate::assets::PinKind::Input => egui::Color32::LIGHT_GREEN,
                    crate::assets::PinKind::Output => egui::Color32::LIGHT_RED,
                };

                // Draw the pin
                ui.painter().circle_filled(
                    pin_screen_pos,
                    self.canvas_config.base_pin_size,
                    pin_color,
                );

                // Add outline if it has current
                if has_current {
                    ui.painter().circle_stroke(
                        pin_screen_pos,
                        self.canvas_config.base_pin_size + 3.0,
                        egui::Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                    );
                }
            }
        }
    }

    pub fn draw_custom_circuit_preview(&self, ui: &Ui, definition_index: usize, pos: Pos2) {
        let screen_center = pos - self.viewport_offset;

        // Get the definition for this custom circuit
        if let Some(definition) = self.db.custom_circuit_definitions.get(definition_index) {
            // Draw as a dark blue rectangle with the name
            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter()
                .rect_filled(rect, CornerRadius::default(), egui::Color32::DARK_BLUE);

            // Draw the name
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &definition.name,
                egui::FontId::default(),
                egui::Color32::WHITE,
            );

            // Draw external pins (no current highlighting in preview)
            for ext_pin in &definition.external_pins {
                let pin_world_pos = pos + ext_pin.offset;
                let pin_screen_pos = pin_world_pos - self.viewport_offset;

                let pin_color = match ext_pin.kind {
                    crate::assets::PinKind::Input => egui::Color32::LIGHT_GREEN,
                    crate::assets::PinKind::Output => egui::Color32::LIGHT_RED,
                };

                // Draw the pin
                ui.painter().circle_filled(
                    pin_screen_pos,
                    self.canvas_config.base_pin_size,
                    pin_color,
                );
            }
        }
    }

    pub fn draw_wire(&self, ui: &Ui, wire: &Wire, hovered: bool, has_current: bool) {
        let mut color = if has_current {
            COLOR_WIRE_POWERED
        } else {
            COLOR_WIRE_IDLE
        };

        if hovered {
            color = COLOR_WIRE_HOVER;
        }

        ui.painter().line_segment(
            [
                wire.start - self.viewport_offset,
                wire.end - self.viewport_offset,
            ],
            Stroke::new(self.canvas_config.wire_thickness, color),
        );
        ui.painter().circle(
            wire.start - self.viewport_offset,
            PIN_HOVER_THRESHOLD / 2.0,
            color,
            Stroke::NONE,
        );
        ui.painter().circle(
            wire.end - self.viewport_offset,
            PIN_HOVER_THRESHOLD / 2.0,
            color,
            Stroke::NONE,
        );
    }

    pub fn get_hovered(&self, mouse_pos: Pos2) -> Option<Hover> {
        if let Some(v) = self.drag {
            match v {
                Drag::Canvas(canvas_drag) => match canvas_drag {
                    crate::drag::CanvasDrag::Single { id, offset: _ } => {
                        return Some(Hover::Instance(id));
                    }
                    crate::drag::CanvasDrag::Selected { .. } => {}
                },
                Drag::Resize { id, start } => {
                    return Some(Hover::Pin(Pin {
                        ins: id,
                        index: u32::from(!start),
                    }));
                }
                Drag::PinToWire {
                    source_pin: _,
                    wire_id,
                } => {
                    // End of the new wire is hovered
                    return Some(Hover::Pin(Pin {
                        ins: wire_id,
                        index: 1,
                    }));
                }
                Drag::BranchWire {
                    original_wire_id,
                    split_point: _,
                    start_mouse_pos: _,
                } => {
                    return Some(Hover::Instance(original_wire_id));
                }
                Drag::Panel { .. } | Drag::Selecting { .. } => {}
            }
        }
        for selected in &self.selected {
            match self.db.ty(*selected) {
                InstanceKind::Wire => {
                    for pin in self.db.pins_of(*selected) {
                        if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                            return Some(Hover::Pin(pin));
                        }
                    }
                    let wire = self.db.get_wire(*selected);
                    let dist = wire.dist_to_closest_point_on_line(mouse_pos);
                    if dist < WIRE_HIT_DISTANCE {
                        return Some(Hover::Instance(*selected));
                    }
                }
                InstanceKind::Gate(_) | InstanceKind::Power | InstanceKind::CustomCircuit(_) => {}
            }
        }

        for (k, power) in &self.db.powers {
            let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }

        for (k, gate) in &self.db.gates {
            let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, wire) in &self.db.wires {
            for pin in self.db.pins_of(k) {
                if self.is_pin_connected(pin) {
                    continue;
                }
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
            let dist = wire.dist_to_closest_point_on_line(mouse_pos);
            if dist < WIRE_HIT_DISTANCE {
                return Some(Hover::Instance(k));
            }
        }

        None
    }

    fn highlight_hovered(&self, ui: &Ui) {
        let Some(hovered) = self.hovered else {
            return;
        };

        match hovered {
            Hover::Pin(pin) => {
                let color = COLOR_HOVER_PIN_TO_WIRE;
                let pin_pos = self.db.pin_position(pin) - self.viewport_offset;
                ui.painter()
                    .circle_filled(pin_pos, PIN_HOVER_THRESHOLD, color);
            }
            Hover::Instance(hovered) => match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    let gate = self.db.get_gate(hovered);
                    let outer = Rect::from_center_size(
                        gate.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(3.0, 3.0),
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Power => {
                    let power = self.db.get_power(hovered);
                    let outer = Rect::from_center_size(
                        power.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(3.0, 3.0),
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                // Wire is highlighted when drawing
                InstanceKind::Wire => {}
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(hovered);
                    let outer = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(3.0, 3.0),
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
            },
        }
    }

    pub fn is_pin_connected(&self, pin: Pin) -> bool {
        self.db.connections.iter().any(|c| c.a == pin || c.b == pin)
    }

    fn draw_selection_highlight(&self, ui: &Ui) {
        for &id in &self.selected {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    let r = Rect::from_center_size(
                        g.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(6.0, 6.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    let r = Rect::from_center_size(
                        p.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(6.0, 6.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Wire => {
                    for pin in self.db.pins_of(id) {
                        let pos = self.db.pin_position(pin);
                        ui.painter().circle_filled(
                            pos - self.viewport_offset,
                            PIN_MOVE_HINT_D,
                            PIN_MOVE_HINT_COLOR,
                        );
                    }
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    let r = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + vec2(6.0, 6.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
            }
        }
    }

    // ---------- Simulation ----------

    fn recompute_current(&mut self) {
        let mut current: HashSet<Pin> = HashSet::new();
        let ids: Vec<InstanceId> = self.db.types.keys().collect();
        for id in ids {
            self.eval_instance(id, &mut current);
        }
        self.current = current;
        self.current_dirty = false;
    }

    fn eval_instance(&self, id: InstanceId, current: &mut HashSet<Pin>) {
        match self.db.ty(id) {
            InstanceKind::Power => {
                let p = self.db.get_power(id);
                if p.on {
                    current.insert(Pin { ins: id, index: 0 });
                }
            }
            InstanceKind::Wire => {
                let a = Pin { ins: id, index: 0 };
                let b = Pin { ins: id, index: 1 };
                let mut visiting = HashSet::new();
                let a_on = self.eval_pin(a, current, &mut visiting);
                visiting.clear();
                let b_on = self.eval_pin(b, current, &mut visiting);
                if a_on || b_on {
                    current.insert(a);
                    current.insert(b);
                }
            }
            InstanceKind::Gate(kind) => {
                // pins: 0=input A, 2=input B, 1=output
                let a = Pin { ins: id, index: 0 };
                let b = Pin { ins: id, index: 2 };
                let out = Pin { ins: id, index: 1 };
                let mut visiting = HashSet::new();
                let a_on = self.eval_pin(a, current, &mut visiting);
                visiting.clear();
                let b_on = self.eval_pin(b, current, &mut visiting);
                let out_on = match kind {
                    GateKind::And => a_on && b_on,
                    GateKind::Nand => !(a_on && b_on),
                    GateKind::Or => a_on || b_on,
                    GateKind::Nor => !(a_on || b_on),
                    GateKind::Xor => (a_on && !b_on) || (!a_on && b_on),
                    GateKind::Xnor => a_on == b_on,
                };
                if out_on {
                    current.insert(out);
                }
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.db.get_custom_circuit(id);
                if let Some(definition) =
                    self.db.custom_circuit_definitions.get(cc.definition_index)
                {
                    self.eval_custom_circuit(id, definition, current);
                }
            }
        }
    }

    fn eval_custom_circuit(
        &self,
        custom_circuit_id: InstanceId,
        definition: &CustomCircuitDefinition,
        current: &mut HashSet<Pin>,
    ) {
        // Create a mapping from external pins to their current state
        let mut internal_current = HashSet::new();

        // For each external input pin, check if it has current and activate the corresponding internal pin
        for (external_pin_index, external_pin) in definition.external_pins.iter().enumerate() {
            if external_pin.kind == crate::assets::PinKind::Input {
                let external_pin_obj = Pin {
                    ins: custom_circuit_id,
                    index: external_pin_index as u32,
                };

                // Check if this external input pin has current flowing into it
                let mut visiting = HashSet::new();
                if self.eval_pin(external_pin_obj, current, &mut visiting) {
                    // Activate the corresponding internal pin
                    internal_current.insert(external_pin.internal_pin);
                }
            }
        }

        // Simulate the internal circuit components
        // This is a simplified simulation that processes components in order
        // A full implementation might need topological sorting or iterative convergence
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: u32 = 100; // Prevent infinite loops

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            let old_size = internal_current.len();

            // Evaluate each internal component
            for component in &definition.internal_components {
                match component {
                    crate::app::InstancePosOffset::Gate(gate_kind, _offset) => {
                        // For simplicity, we'll implement a basic gate evaluation
                        // This assumes the standard gate pin layout: input A (0), output (1), input B (2)
                        // Note: This is a simplified approach - a full implementation would need
                        // to create a temporary DB and simulate properly
                        Self::eval_internal_gate(
                            *gate_kind,
                            &definition.internal_connections,
                            &mut internal_current,
                        );
                    }
                    crate::app::InstancePosOffset::Power(_offset) => {
                        // Powers in the internal circuit should activate their output pins
                        // For now, we'll assume they're always on when included in a custom circuit
                        // A full implementation would track their state
                    }
                    crate::app::InstancePosOffset::Wire(..)
                    | crate::app::InstancePosOffset::CustomCircuit(..) => {
                        // Wires propagate current from one end to the other
                        // Nested custom circuits would need recursive evaluation
                        // For now, these cases are handled by connection processing
                    }
                }
            }

            // Propagate current through internal connections
            let mut new_current = internal_current.clone();
            for connection in &definition.internal_connections {
                if internal_current.contains(&connection.a) {
                    new_current.insert(connection.b);
                }
                if internal_current.contains(&connection.b) {
                    new_current.insert(connection.a);
                }
            }

            if new_current.len() != old_size {
                changed = true;
            }
            internal_current = new_current;
            iterations += 1;
        }

        // Map internal outputs back to external outputs
        for (external_pin_index, external_pin) in definition.external_pins.iter().enumerate() {
            if external_pin.kind == crate::assets::PinKind::Output
                && internal_current.contains(&external_pin.internal_pin)
            {
                current.insert(Pin {
                    ins: custom_circuit_id,
                    index: external_pin_index as u32,
                });
            }
        }
    }

    fn eval_internal_gate(
        _gate_kind: GateKind,
        connections: &[Connection],
        internal_current: &mut HashSet<Pin>,
    ) {
        // This is a simplified gate evaluation for internal components
        // In a full implementation, we would need to properly identify which pins
        // belong to which internal component instances

        // For now, we'll implement a basic version that looks for gate patterns in connections
        // This is not a complete implementation but provides a foundation

        // Note: This is a placeholder implementation
        // A proper implementation would require:
        // 1. Mapping internal components to their pins
        // 2. Proper gate logic evaluation
        // 3. Handling of component positioning and identification

        // For the current implementation, we'll use a simplified approach
        // that just propagates current through connections
        for connection in connections {
            if internal_current.contains(&connection.a) && !internal_current.contains(&connection.b)
            {
                internal_current.insert(connection.b);
            }
            if internal_current.contains(&connection.b) && !internal_current.contains(&connection.a)
            {
                internal_current.insert(connection.a);
            }
        }
    }

    fn connected_pins(&self, pin: Pin) -> Vec<Pin> {
        let mut res = Vec::new();
        for c in &self.db.connections {
            if let Some((_, other)) = c.get_pin_first(pin) {
                res.push(other);
            }
        }
        res
    }

    fn eval_pin(&self, pin: Pin, current: &mut HashSet<Pin>, visiting: &mut HashSet<Pin>) -> bool {
        if current.contains(&pin) {
            return true;
        }
        if !visiting.insert(pin) {
            return false;
        }

        for other in self.connected_pins(pin) {
            match self.db.ty(other.ins) {
                InstanceKind::Power => {
                    if self.db.get_power(other.ins).on {
                        current.insert(pin);
                        visiting.remove(&pin);
                        return true;
                    }
                }
                InstanceKind::Wire => {
                    let other_end = Pin {
                        ins: other.ins,
                        index: 1 - other.index,
                    };
                    if self.eval_pin(other_end, current, visiting) {
                        current.insert(pin);
                        visiting.remove(&pin);
                        return true;
                    }
                }
                InstanceKind::Gate(_) => {
                    let out_pin = Pin {
                        ins: other.ins,
                        index: 1,
                    };
                    if other == out_pin {
                        self.eval_instance(other.ins, current);
                        if current.contains(&out_pin) {
                            current.insert(pin);
                            visiting.remove(&pin);
                            return true;
                        }
                    }
                }
                InstanceKind::CustomCircuit(_) => {
                    // Custom circuits need special handling in simulation
                    // For now, we don't propagate through them
                }
            }
        }
        visiting.remove(&pin);
        false
    }

    fn debug_string(&self, ui: &Ui) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "counts: gates={}, powers={}, wires={}, custom_circuits={}, custom_defs={}, conns={}",
            self.db.gates.len(),
            self.db.powers.len(),
            self.db.wires.len(),
            self.db.custom_circuits.len(),
            self.db.custom_circuit_definitions.len(),
            self.db.connections.len()
        )
        .ok();
        writeln!(out, "hovered: {:?}", self.hovered).ok();
        writeln!(out, "drag: {:?}", self.drag).ok();
        writeln!(out, "viewport_offset: {:?}", self.viewport_offset).ok();
        writeln!(out, "potential_conns: {}", self.potential_connections.len()).ok();
        writeln!(out, "clipboard: {:?}", self.clipboard).ok();
        writeln!(out, "selected: {:?}", self.selected).ok();

        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));
        writeln!(out, "mouse: {mouse_pos_world:?}").ok();

        writeln!(out, "\nInstances:").ok();
        for (id, _) in &self.db.instances {
            writeln!(out, "  {id:?}").ok();
        }

        writeln!(out, "\nGates:").ok();
        for (id, g) in &self.db.gates {
            writeln!(
                out,
                "  {:?}: kind={:?} pos=({:.1},{:.1})",
                id, g.kind, g.pos.x, g.pos.y
            )
            .ok();
            // pins
            for (i, pin) in g.kind.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let p = g.pos + pin_offset;
                writeln!(out, "    pin#{i} {:?} at ({:.1},{:.1})", pin.kind, p.x, p.y).ok();
            }
        }

        writeln!(out, "\nPowers:").ok();
        for (id, p) in &self.db.powers {
            writeln!(
                out,
                "  {:?}: on={} pos=({:.1},{:.1})",
                id, p.on, p.pos.x, p.pos.y
            )
            .ok();
            for (i, pin) in p.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let pp = p.pos + pin_offset;
                writeln!(
                    out,
                    "    pin#{i} {:?} at ({:.1},{:.1})",
                    pin.kind, pp.x, pp.y
                )
                .ok();
            }
        }

        writeln!(out, "\nWires:").ok();
        for (id, w) in &self.db.wires {
            writeln!(
                out,
                "  {:?}: start=({:.1},{:.1}) end=({:.1},{:.1})",
                id, w.start.x, w.start.y, w.end.x, w.end.y
            )
            .ok();
        }

        writeln!(out, "\nConnections:").ok();
        for c in &self.db.connections {
            let p1 = self.db.pin_position(c.a);
            let p2 = self.db.pin_position(c.b);
            writeln!(
                out,
                "  ({:?}:{}) <-> ({:?}:{}) | ({:.1},{:.1})<->({:.1},{:.1})",
                c.a.ins, c.a.index, c.b.ins, c.b.index, p1.x, p1.y, p2.x, p2.y
            )
            .ok();
        }

        if self.potential_connections.is_empty() {
            writeln!(out, "\nPotential Connections: none").ok();
        } else {
            writeln!(out, "\nPotential Connections:").ok();
            for c in &self.potential_connections {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                writeln!(
                    out,
                    "  ({:?}:{}) <-> ({:?}:{}) | ({:.1},{:.1})<->({:.1},{:.1})",
                    c.a.ins, c.a.index, c.b.ins, c.b.index, p1.x, p1.y, p2.x, p2.y
                )
                .ok();
            }
        }

        writeln!(out, "\n{}", self.connection_manager.debug_info()).ok();

        out
    }

    pub fn extract_instances_with_offsets(
        &self,
        instances: &HashSet<InstanceId>,
    ) -> (Rect, Vec<InstancePosOffset>) {
        let mut points = vec![];
        for &id in instances {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    points.push(g.pos);
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    points.push(p.pos);
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(id);
                    points.push(w.start);
                    points.push(w.end);
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    points.push(cc.pos);
                }
            }
        }
        let rect = Rect::from_points(&points);
        let center = rect.center();

        let mut object_pos = vec![];

        for &id in instances {
            let ty = self.db.ty(id);
            match ty {
                InstanceKind::Gate(kind) => {
                    let g = self.db.get_gate(id);
                    object_pos.push(InstancePosOffset::Gate(kind, center - g.pos));
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    object_pos.push(InstancePosOffset::Power(center - p.pos));
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(id);
                    object_pos.push(InstancePosOffset::Wire(center - w.start, center - w.end));
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    object_pos.push(InstancePosOffset::CustomCircuit(
                        cc.definition_index,
                        center - cc.pos,
                    ));
                }
            }
        }

        (rect, object_pos)
    }

    fn copy_to_clipboard(&mut self) {
        let (_, object_pos) = self.extract_instances_with_offsets(&self.selected);
        self.clipboard = object_pos;
    }

    fn paste_from_clipboard(&mut self, mouse: Pos2) {
        self.selected.clear(); // Clear existing selection before pasting new items
        for to_paste in self.clipboard.clone() {
            let id = match to_paste {
                InstancePosOffset::Gate(gate_kind, offset) => self.db.new_gate(Gate {
                    kind: gate_kind,
                    pos: mouse - offset,
                }),
                InstancePosOffset::Power(offset) => self.db.new_power(Power {
                    pos: mouse - offset,
                    on: false,
                }),
                InstancePosOffset::Wire(s, e) => self.db.new_wire(Wire::new(mouse - s, mouse - e)),
                InstancePosOffset::CustomCircuit(def_index, offset) => {
                    self.db.new_custom_circuit(custom_circuit::CustomCircuit {
                        pos: mouse - offset,
                        definition_index: def_index,
                    })
                }
            };
            self.connection_manager.mark_instance_dirty(id);
            self.selected.insert(id);
        }
        self.connection_manager.rebuild_spatial_index(&self.db);
    }

    fn draw_right_click_actions_menu(&mut self, ui: &Ui) {
        if !self.show_right_click_actions_menu {
            return;
        }

        let mut should_close = false;
        egui::Window::new("Context Menu")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fixed_pos(self.context_menu_pos)
            .show(ui.ctx(), |ui| {
                ui.set_min_width(150.0);
                if ui.button("Create Custom Circuit").clicked() {
                    // Generate a unique name for the custom circuit
                    let circuit_name = format!(
                        "CustomCircuit_{}",
                        self.db.custom_circuit_definitions.len() + 1
                    );

                    match self.create_custom_circuit(circuit_name, &self.selected.clone()) {
                        Ok(()) => {
                            log::info!("Custom circuit created successfully");
                            self.selected.clear();
                        }
                        Err(e) => {
                            log::error!("Failed to create custom circuit: {e}");
                        }
                    }
                    should_close = true;
                }

                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

        if should_close {
            self.show_right_click_actions_menu = false;
        }
    }

    fn highlight_selected_actions(&mut self, ui: &Ui, mouse: Option<Pos2>, mouse_down: bool) {
        let Some(selected) = self.selected.iter().next() else {
            return;
        };
        let selected = *selected;

        match self.db.ty(selected) {
            InstanceKind::Wire => {
                for pin in self.db.pins_of(selected) {
                    let pos = self.db.pin_position(pin);
                    ui.painter().circle_filled(
                        pos - self.viewport_offset,
                        PIN_MOVE_HINT_D,
                        PIN_MOVE_HINT_COLOR,
                    );

                    if let Some(mouse) = mouse
                        && mouse_down
                        && mouse.distance(pos) < PIN_MOVE_HINT_D
                    {
                        self.drag = Some(Drag::Resize {
                            id: selected,
                            start: pin.index == 0,
                        });
                    }
                }
            }
            InstanceKind::Gate(_) | InstanceKind::Power | InstanceKind::CustomCircuit(_) => {}
        }
    }

    pub fn split_wire_at_point(&mut self, wire_id: InstanceId, split_point: Pos2) {
        let original_wire = *self.db.get_wire(wire_id);

        let new_wire = Wire::new(split_point, original_wire.end);
        let new_wire_id = self.db.new_wire(new_wire);

        let original_wire_mut = self.db.get_wire_mut(wire_id);
        original_wire_mut.end = split_point;

        self.connection_manager
            .mark_instances_dirty(&[wire_id, new_wire_id]);
        self.current_dirty = true;
    }

    pub fn wire_branching_action_point(
        &self,
        mouse: Pos2,
        instance_id: InstanceId,
    ) -> Option<Pos2> {
        if !self.selected.contains(&instance_id) || self.selected.len() != 1 {
            return None;
        }
        if !matches!(self.db.ty(instance_id), InstanceKind::Wire) {
            return None;
        }
        let wire = self.db.get_wire(instance_id);

        if wire.dist_to_closest_point_on_line(mouse) > NEW_PIN_ON_WIRE_THRESHOLD {
            return None;
        }

        let split_point = wire.closest_point_on_line(mouse);
        if (split_point - wire.start).length() < MIN_WIRE_SIZE
            || (split_point - wire.end).length() < MIN_WIRE_SIZE
        {
            return None;
        }

        Some(split_point)
    }
}

fn get_icon<'a>(ui: &Ui, source: egui::ImageSource<'a>) -> Image<'a> {
    let mut image = egui::Image::new(source);

    if ui.visuals().dark_mode {
        image = image.bg_fill(Color32::WHITE);
    }

    image
}

fn draw_icon_canvas(ui: &mut Ui, source: egui::ImageSource<'_>, rect: Rect) {
    let image = get_icon(ui, source).fit_to_exact_size(rect.size());

    ui.put(rect, image);
}
