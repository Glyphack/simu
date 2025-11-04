use crate::db::{
    Circuit, Clock, DB, Gate, GateKind, InstanceId, InstanceKind, Label, LabelId, Lamp,
    ModuleDefId, Pin, Power, Wire,
};
use std::collections::HashSet;
use std::fmt::Write as _;

use egui::{
    Align, Button, Color32, CornerRadius, Image, Layout, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget as _, pos2, vec2,
};

use crate::assets::PinKind;
use crate::drag::CanvasDrag;
use crate::simulator::{SimulationStatus, Simulator, Value, lamp_input, wire_start};
use crate::{
    assets::{self},
    config::CanvasConfig,
    connection_manager::{Connection, ConnectionManager},
    drag::Drag,
    module::Module,
};

pub const PANEL_BUTTON_MAX_HEIGHT: f32 = 50.0;

pub const LABEL_EDIT_TEXT_SIZE: f32 = 16.0;
pub const LABEL_DISPLAY_TEXT_SIZE: f32 = 19.0;

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
pub const PIN_HOVER_THRESHOLD: f32 = 10.0;

pub const INSTANEC_OUTLINE_EXPAND: f32 = 6.0;
pub const INSTANEC_OUTLINE: Vec2 = vec2(6.0, 6.0);
pub const INSTANEC_OUTLINE_THICKNESS: f32 = 2.0;

pub const NEW_PIN_ON_WIRE_THRESHOLD: f32 = 10.0;

// Connections
pub const COLOR_POTENTIAL_CONN_HIGHLIGHT: Color32 = Color32::LIGHT_BLUE;
pub const WIRE_HIT_DISTANCE: f32 = 8.0;
pub const SNAP_THRESHOLD: f32 = 10.0;
pub const PIN_MOVE_HINT_D: f32 = 10.0;
pub const PIN_MOVE_HINT_COLOR: Color32 = Color32::GRAY;

pub const COLOR_SELECTION_HIGHLIGHT: Color32 = Color32::GRAY;
pub const COLOR_SELECTION_BOX: Color32 = Color32::LIGHT_BLUE;

pub const MIN_WIRE_SIZE: f32 = 40.0;

#[derive(serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash, Copy, Debug, Clone)]
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum ClipBoardItem {
    Gate(GateKind, Vec2),
    Power(Vec2),
    Wire(Vec2, Vec2),
    Lamp(Vec2),
    Clock(Vec2),
    // Index to definition
    Module(ModuleDefId, Vec2),
    Label(String, Vec2),
}

pub fn current_dirty() -> bool {
    true
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockState {
    Stopped,
    Running,
}

#[derive(Debug, Clone)]
pub struct ClockController {
    pub voltage: bool,
    pub state: ClockState,
    pub tick_accumulator: f32,
    pub tick_interval: f32, // seconds between ticks
}

impl Default for ClockController {
    fn default() -> Self {
        Self {
            voltage: false,
            state: ClockState::Running,
            tick_accumulator: 0.0,
            tick_interval: 0.5, // 0.5 seconds = 2 Hz
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
    // mark when current needs recomputation
    #[serde(skip, default = "current_dirty")]
    pub current_dirty: bool,
    pub show_debug: bool,
    // selection set and move preview
    // TODO: Selection is not handling labels.
    pub selected: HashSet<InstanceId>,
    //Copied. Items with their offset compared to a middle point in the rectangle
    pub clipboard: Vec<ClipBoardItem>,
    // Where are we in the world
    pub viewport_offset: Vec2,
    // For web load functionality - stores pending JSON to load
    #[serde(skip)]
    pub pending_load_json: Option<String>,
    #[serde(skip)]
    pub panning: bool,
    #[serde(skip)]
    pub panel_width: f32,
    // Label editing state
    #[serde(skip)]
    pub editing_label: Option<LabelId>,
    #[serde(skip)]
    pub label_edit_buffer: String,
    // Module creation dialog state
    #[serde(skip)]
    pub creating_module: bool,
    #[serde(skip)]
    pub module_name_buffer: String,
    #[serde(skip)]
    pub module_creation_error: Option<String>,
    // Simulation service - holds simulation state and results
    #[serde(skip)]
    pub simulator: Simulator,
    // Clock controller for managing clock ticking
    #[serde(skip, default = "ClockController::default")]
    pub clock_controller: ClockController,
}

impl Default for App {
    fn default() -> Self {
        let canvas_config = CanvasConfig::default();
        let db = DB::default();
        let c = ConnectionManager::new(&db.circuit, &canvas_config);
        Self {
            db,
            canvas_config,
            drag: Default::default(),
            hovered: Default::default(),
            connection_manager: c,
            potential_connections: Default::default(),
            current_dirty: true,
            show_debug: true,
            selected: Default::default(),
            clipboard: Default::default(),
            pending_load_json: None,
            viewport_offset: Vec2::ZERO,
            panning: false,
            panel_width: 0.0,
            editing_label: None,
            label_edit_buffer: String::new(),
            creating_module: false,
            module_name_buffer: String::new(),
            module_creation_error: None,
            simulator: Simulator::default(),
            clock_controller: ClockController::default(),
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

                ui.menu_button("Tools", |ui| {
                    if ui.button("Create module").clicked() {
                        self.create_module();
                    }
                });
                ui.add_space(16.0);

                // Clock controls
                ui.label("Clock:");
                if ui.button("⏹ Stop").clicked() {
                    self.clock_controller.state = ClockState::Stopped;
                }
                if ui.button("⏭ Step").clicked() {
                    self.clock_controller.voltage = !self.clock_controller.voltage;
                    self.current_dirty = true;
                }
                if ui.button("▶ Start").clicked() {
                    self.clock_controller.state = ClockState::Running;
                    self.clock_controller.tick_accumulator = 0.0;
                }
                ui.add_space(8.0);

                // Clock speed slider
                ui.label("Speed:");
                let mut speed_hz = 1.0 / self.clock_controller.tick_interval;
                if ui
                    .add(egui::Slider::new(&mut speed_hz, 0.5..=5.0).text("Hz"))
                    .changed()
                {
                    self.clock_controller.tick_interval = 1.0 / speed_hz;
                }
                ui.add_space(16.0);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);

                    ui.add_space(16.0);
                });
            });
        });

        let dt = ctx.input(|i| i.stable_dt);
        let should_tick = match self.clock_controller.state {
            ClockState::Running => {
                self.clock_controller.tick_accumulator += dt;
                if self.clock_controller.tick_accumulator >= self.clock_controller.tick_interval {
                    self.clock_controller.tick_accumulator -= self.clock_controller.tick_interval;
                    true
                } else {
                    false
                }
            }
            ClockState::Stopped => false,
        };

        if should_tick {
            self.clock_controller.voltage = !self.clock_controller.voltage;
            self.simulator.clocks_on = self.clock_controller.voltage;
            self.current_dirty = true;
        }

        // Request continuous repaint if clock is running
        if self.clock_controller.state == ClockState::Running {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main(ui);
        });
    }
}

impl App {
    pub fn circuit(&self) -> &Circuit {
        &self.db.circuit
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    fn is_on(&self, pin: Pin) -> bool {
        let Some(v) = self.simulator.current.get(&pin) else {
            return false;
        };

        *v == Value::One
    }

    pub fn draw_main(&mut self, ui: &mut Ui) {
        self.process_pending_load();

        if self.show_debug {
            egui::Window::new("Debug logs").show(ui.ctx(), |ui| {
                egui_logger::logger_ui().show(ui);
            });
        }

        if self.creating_module {
            egui::Window::new("Create Module")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.vertical(|ui| {
                        ui.label("Enter module name:");
                        let response = ui.text_edit_singleline(&mut self.module_name_buffer);

                        // Auto-focus the text input when dialog opens
                        response.request_focus();

                        // Check for Enter key to confirm
                        if ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !self.module_name_buffer.trim().is_empty()
                        {
                            self.confirm_module_creation();
                        }

                        // Check for Escape key to cancel
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.creating_module = false;
                            self.module_name_buffer.clear();
                            self.module_creation_error = None;
                        }

                        // Show error message if any
                        if let Some(error) = &self.module_creation_error {
                            ui.colored_label(egui::Color32::RED, error);
                        }

                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                self.confirm_module_creation();
                            }
                            if ui.button("Cancel").clicked() {
                                self.creating_module = false;
                                self.module_name_buffer.clear();
                                self.module_creation_error = None;
                            }
                        });
                    });
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
                    ui.heading("Tools");
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
            .auto_shrink([true, false])
            .show(ui, |ui| {
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::And));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nand));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Or));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xnor));
                self.draw_panel_button(ui, InstanceKind::Power);
                self.draw_panel_button(ui, InstanceKind::Lamp);
                self.draw_panel_button(ui, InstanceKind::Clock);
                self.draw_panel_button(ui, InstanceKind::Wire);

                ui.add_space(8.0);
                self.draw_label_button(ui);

                if !self.db.module_definitions.is_empty() {
                    ui.add_space(8.0);
                    ui.label("Modules:");
                }
                let keys: Vec<ModuleDefId> = self.db.module_definitions.keys().collect();
                for i in keys {
                    self.draw_panel_button(ui, InstanceKind::Module(i));
                }

                ui.add_space(8.0);

                if Button::new("Clear")
                    .min_size(vec2(PANEL_BUTTON_MAX_HEIGHT, 30.0))
                    .ui(ui)
                    .clicked()
                {
                    self.db = DB::default();
                    self.hovered = None;
                    self.selected.clear();
                    self.drag = None;
                    self.connection_manager =
                        ConnectionManager::new(self.circuit(), &self.canvas_config);
                    self.simulator = Simulator::new();
                }
            });
    }

    fn draw_panel_button(&mut self, ui: &mut Ui, kind: InstanceKind) -> Response {
        let resp = match kind {
            InstanceKind::Gate(gate_kind) => {
                let s = get_icon(ui, gate_kind.graphics().svg.clone())
                    .fit_to_exact_size(vec2(PANEL_BUTTON_MAX_HEIGHT, PANEL_BUTTON_MAX_HEIGHT));
                ui.add(egui::Button::image(s).sense(Sense::click_and_drag()))
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
                .fit_to_exact_size(vec2(PANEL_BUTTON_MAX_HEIGHT, PANEL_BUTTON_MAX_HEIGHT));
                ui.add(egui::Button::image(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Lamp => {
                let s = get_icon(ui, Lamp { pos: Pos2::ZERO }.graphics().svg.clone())
                    .fit_to_exact_size(vec2(PANEL_BUTTON_MAX_HEIGHT, PANEL_BUTTON_MAX_HEIGHT));
                ui.add(egui::Button::image(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Clock => {
                let s = get_icon(ui, Clock { pos: Pos2::ZERO }.graphics().svg.clone())
                    .fit_to_exact_size(vec2(PANEL_BUTTON_MAX_HEIGHT, PANEL_BUTTON_MAX_HEIGHT));
                ui.add(egui::Button::image(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Wire => ui.add(
                Button::new("Wire")
                    .sense(Sense::click_and_drag())
                    .min_size(vec2(PANEL_BUTTON_MAX_HEIGHT, 30.0)),
            ),
            InstanceKind::Module(i) => ui.add(
                Button::new(self.db.get_module_def(i).name.clone())
                    .sense(Sense::click_and_drag())
                    .min_size(vec2(PANEL_BUTTON_MAX_HEIGHT, 30.0)),
            ),
        };
        let mouse_pos_world = self.mouse_pos_world(ui);

        if resp.drag_started()
            && let Some(pos) = mouse_pos_world
        {
            let id = match kind {
                InstanceKind::Gate(kind) => self.db.new_gate(Gate { pos, kind }),
                InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                InstanceKind::Wire => self.db.new_wire(Wire::new_at(pos)),
                InstanceKind::Lamp => self.db.new_lamp(Lamp { pos }),
                InstanceKind::Clock => self.db.new_clock(Clock { pos }),
                InstanceKind::Module(c) => self.db.new_module(Module {
                    pos,
                    definition_index: c,
                }),
            };
            self.set_drag(Drag::Canvas(crate::drag::CanvasDrag::Single {
                id,
                offset: Vec2::ZERO,
            }));
        }

        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));
        if resp.hovered()
            && d_pressed
            && let InstanceKind::Module(i) = kind
        {
            let mut ids = Vec::new();
            for (id, m) in &self.circuit().modules {
                if m.definition_index == i {
                    ids.push(id);
                }
            }
            for id in ids {
                self.delete_instance(id);
            }
            self.db.module_definitions.remove(i);
        }
        ui.add_space(8.0);

        resp
    }

    fn draw_label_button(&mut self, ui: &mut Ui) -> Response {
        let resp = ui.add(
            Button::new("Label")
                .sense(Sense::click_and_drag())
                .min_size(vec2(PANEL_BUTTON_MAX_HEIGHT, 30.0)),
        );

        let mouse = self.mouse_pos_world(ui);

        if resp.drag_started()
            && let Some(pos) = mouse
        {
            let id = self.db.new_label(Label::new(pos));
            self.set_drag(Drag::Label {
                id,
                offset: Vec2::ZERO,
            });
        }
        ui.add_space(8.0);

        resp
    }

    fn handle_copy_pasting(&mut self, ui: &Ui, mouse_pos_world: Option<Pos2>) {
        if self.creating_module {
            return;
        }

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

        if copy_event_detected {
            self.copy_to_clipboard();
        }

        if paste_event_detected
            && !self.clipboard.is_empty()
            && let Some(mouse) = mouse_pos_world
        {
            self.paste_from_clipboard(mouse);
            self.current_dirty = true;
        }
    }

    fn handle_deletion(&mut self, ui: &Ui) {
        if self.creating_module {
            return;
        }

        let bs_pressed = ui.input(|i| i.key_pressed(egui::Key::Backspace));
        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));

        if bs_pressed || d_pressed {
            if let Some(id) = self.hovered.take() {
                match id {
                    Hover::Pin(pin) => self.delete_instance(pin.ins),
                    Hover::Instance(instance_id) => self.delete_instance(instance_id),
                }
            } else if self.hovered.is_none() && !self.selected.is_empty() {
                let ids_to_delete: Vec<InstanceId> = self.selected.drain().collect();
                for id in ids_to_delete {
                    self.delete_instance(id);
                }
            }
        }
    }

    pub fn delete_instance(&mut self, id: InstanceId) {
        self.hovered.take();
        self.drag.take();
        self.selected.remove(&id);

        self.connection_manager.dirty_instances.remove(&id);
        self.db.circuit.remove(id);
        self.connection_manager
            .rebuild_spatial_index(&self.db.circuit);
        self.current_dirty = true;
    }

    pub fn delete_label(&mut self, id: LabelId) {
        self.db.circuit.labels.remove(id);
        if self.editing_label == Some(id) {
            self.editing_label = None;
        }
        self.hovered.take();
        self.drag.take();
    }

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = resp.rect;

        // Set clip rectangle to prevent canvas objects from drawing outside canvas bounds
        ui.set_clip_rect(canvas_rect);

        Self::draw_grid(ui, canvas_rect, self.viewport_offset);

        let mouse_clicked_canvas = resp.clicked();
        let mouse_dragging_canvas = resp.dragged();
        let double_clicked = ui.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        let mouse_is_visible = resp.contains_pointer();
        let mouse_pos_world = self.mouse_pos_world(ui);

        let mouse_up = ui.input(|i| i.pointer.any_released());
        // To use the canvas clicked we need to set everything on objects. Right now some stuff are
        // on canvas rect
        let mouse_clicked = ui.input(|i| i.pointer.primary_pressed()) && mouse_is_visible;
        let right_released = ui.input(|i| i.pointer.secondary_released());
        let right_down = ui.input(|i| i.pointer.secondary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());

        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc_pressed = ui.input(|i| i.key_released(egui::Key::Escape));

        if !self.creating_module {
            if right_down && mouse_is_visible && self.hovered.is_none() {
                self.panning = true;
            }
            if right_released || !mouse_is_visible {
                self.panning = false;
            }
            if self.panning {
                self.viewport_offset += ui.input(|i| i.pointer.delta());
            }

            self.handle_copy_pasting(ui, mouse_pos_world);
            self.handle_deletion(ui);

            if let Some(editing_id) = self.editing_label {
                let label = self.db.get_label_mut(editing_id);
                label.text = self.label_edit_buffer.clone();
                if mouse_up || enter_pressed || esc_pressed {
                    self.editing_label = None;
                    self.label_edit_buffer.clear();
                }
            }

            if double_clicked
                && self.hovered.is_none()
                && let Some(mouse) = mouse_pos_world
            {
                let id = self.db.new_label(Label::new(mouse));
                self.editing_label = Some(id);
                self.label_edit_buffer = String::from("Label");
            }

            if let Some(mouse) = mouse_pos_world {
                if mouse_dragging_canvas {
                    self.set_drag(Drag::Selecting { start: mouse });
                }
                let instance_dragging = self.drag.is_some();

                if instance_dragging {
                    self.handle_dragging(ui, mouse);
                }

                if mouse_up && instance_dragging {
                    self.handle_drag_end(mouse);

                    if self
                        .connection_manager
                        .update_connections(&mut self.db.circuit)
                    {
                        // self.current_dirty = true;
                    }
                }
            }
        }

        if !self.creating_module {
            if self.selected.len() == 1 {
                self.highlight_selected_actions(ui, mouse_pos_world, mouse_clicked);
            }

            if mouse_clicked_canvas {
                self.selected.clear();
            }

            if right_clicked
                && let Some(id) = self.hovered.as_ref().map(|i| i.instance())
                && matches!(self.db.ty(id), InstanceKind::Power)
            {
                let p = self.db.get_power_mut(id);
                p.on = !p.on;
                self.current_dirty = true;
            }
        }

        if self.current_dirty {
            self.simulator.compute(&self.db.circuit);
            self.current_dirty = false;
        }

        // Draw world
        self.hovered = None;
        for id in self.db.gate_ids() {
            self.draw_gate(ui, id);
        }
        for id in self.db.power_ids() {
            self.draw_power(ui, id);
        }
        for id in self.db.lamp_ids() {
            self.draw_lamp(ui, id);
        }
        for id in self.db.clock_ids() {
            self.draw_clock(ui, id);
        }
        for id in self.db.module_ids() {
            self.draw_module(ui, id);
        }
        for id in self.db.wire_ids() {
            let has_current = self.is_on(wire_start(id));
            self.draw_wire(
                ui,
                id,
                self.hovered
                    .as_ref()
                    .is_some_and(|f| matches!(f, Hover::Instance(_)) && f.instance() == id),
                has_current,
            );
        }
        // Collect labels to avoid borrowing issues
        for id in self.db.label_ids() {
            self.draw_label(ui, id);
        }

        for c in &self.potential_connections {
            // Highlight the pin that it's going to attach. The stable pin.
            let pin_to_highlight = c.b;
            let p = self.db.pin_position(pin_to_highlight, &self.canvas_config);
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

    fn draw_instance_graphics_new(
        &mut self,
        ui: &mut Ui,
        graphics: assets::InstanceGraphics,
        pos: Pos2,
        id: InstanceId,
    ) -> Rect {
        let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
        let image = get_icon(ui, graphics.svg)
            .fit_to_exact_size(rect.size())
            .sense(Sense::click_and_drag());
        let rect = rect.expand(INSTANEC_OUTLINE_EXPAND);
        let response = ui.put(rect, image);

        if response.clicked() {
            self.selected.clear();
            self.selected.insert(id);
        }
        if response.hovered() {
            self.hovered = Some(Hover::Instance(id));
        }
        if response.dragged()
            && let Some(mouse) = ui.ctx().pointer_interact_pos()
        {
            self.selected.clear();
            self.set_drag(Drag::Canvas(CanvasDrag::Single {
                id,
                offset: pos - mouse,
            }));
        }

        for (i, pin) in graphics.pins.iter().enumerate() {
            let pin_pos = pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };

            let rect = Rect::from_center_size(
                pin_pos,
                Vec2::splat(self.canvas_config.base_pin_size + PIN_HOVER_THRESHOLD),
            );
            let pin_resp = ui.allocate_rect(rect, Sense::drag());
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);

            let pin = Pin::new(id, i as u32, pin.kind);
            if pin_resp.hovered() {
                self.hovered = Some(Hover::Pin(pin));
            }

            if pin_resp.dragged() {
                self.selected.clear();
                self.set_drag(Drag::PinToWire {
                    source_pin: pin,
                    start_pos: pin_pos,
                });
            }

            if self.is_on(pin) {
                ui.painter().circle_stroke(
                    pin_pos,
                    self.canvas_config.base_pin_size + 3.0,
                    Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                );
            }
        }
        rect
    }

    fn draw_gate(&mut self, ui: &mut Ui, id: InstanceId) {
        let (pos, kind) = {
            let gate = self.db.get_gate(id);
            (gate.pos, gate.kind)
        };
        self.draw_instance_graphics_new(ui, kind.graphics(), self.adjusted_pos(pos), id);
    }

    fn draw_power(&mut self, ui: &mut Ui, id: InstanceId) {
        let (pos, graphics) = {
            let power = self.db.get_power(id);
            (power.pos, power.graphics())
        };
        self.draw_instance_graphics_new(ui, graphics, self.adjusted_pos(pos), id);
    }

    fn draw_lamp(&mut self, ui: &mut Ui, id: InstanceId) {
        let has_current = self.is_on(lamp_input(id));
        let (pos, graphics) = {
            let lamp = self.db.get_lamp(id);
            (lamp.pos, lamp.graphics())
        };
        let pos = self.adjusted_pos(pos);

        if has_current {
            let glow_radius = 40.0;
            let gradient_steps = 30;
            for i in 0..gradient_steps {
                let t = i as f32 / gradient_steps as f32;
                let radius = glow_radius * (1.0 - t);
                let alpha = (255.0 * (1.0 - t) * 0.4) as u8;
                ui.painter().circle_filled(
                    pos + vec2(0.0, -25.0),
                    radius,
                    Color32::from_rgba_unmultiplied(255, 255, 0, alpha),
                );
            }
        }

        self.draw_instance_graphics_new(ui, graphics, pos, id);
    }

    fn draw_clock(&mut self, ui: &mut Ui, id: InstanceId) {
        let (pos, graphics) = {
            let clock = self.db.get_clock(id);
            (clock.pos, clock.graphics())
        };
        let pos = self.adjusted_pos(pos);
        self.draw_instance_graphics_new(ui, graphics, pos, id);
    }

    fn draw_module(&mut self, ui: &mut Ui, id: InstanceId) {
        let (pos, definition_index) = {
            let module = self.db.get_module(id);
            (module.pos, module.definition_index)
        };
        let screen_center = pos - self.viewport_offset;

        {
            let definition = self.db.get_module(id).definition(&self.db);
            // TODO: Pins for modules
            let name = definition.name.clone();
            let pins = [];

            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter()
                .rect_filled(rect, CornerRadius::default(), egui::Color32::DARK_BLUE);

            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &name,
                egui::FontId::default(),
                egui::Color32::WHITE,
            );

            let response = ui.allocate_rect(rect, Sense::click_and_drag());

            if response.clicked() {
                self.selected.clear();
                self.selected.insert(id);
            }
            if response.hovered() {
                self.hovered = Some(Hover::Instance(id));
            }
            if response.dragged()
                && let Some(mouse) = ui.ctx().pointer_interact_pos()
            {
                self.selected.clear();
                self.set_drag(Drag::Canvas(CanvasDrag::Single {
                    id,
                    offset: screen_center - mouse,
                }));
            }

            // Collect input and output pin indices
            let mut input_indices = vec![];
            let mut output_indices = vec![];
            for (i, &kind) in pins.iter().enumerate() {
                match kind {
                    crate::assets::PinKind::Input => input_indices.push(i),
                    crate::assets::PinKind::Output => output_indices.push(i),
                }
            }

            // Base size for layout
            let base_size = self.canvas_config.base_gate_size;
            let left_x = screen_center.x - base_size.x / 2.0;
            let right_x = screen_center.x + base_size.x / 2.0;
            let top_y = screen_center.y - base_size.y / 2.0;
            let _bottom_y = screen_center.y + base_size.y / 2.0;

            // Helper function to place pins
            let mut place_pins = |indices: &Vec<usize>, x: f32| {
                if indices.is_empty() {
                    return;
                }
                let num = indices.len();
                let spacing = if num == 1 {
                    0.0
                } else {
                    base_size.y / (num - 1) as f32
                };
                for (local_i, &pin_index) in indices.iter().enumerate() {
                    let y = if num == 1 {
                        screen_center.y
                    } else {
                        top_y + local_i as f32 * spacing
                    };
                    let pin_pos_world = egui::Pos2::new(x, y);
                    let pin_screen_pos = self.adjusted_pos(pin_pos_world);

                    let pin_color = match pins[pin_index] {
                        crate::assets::PinKind::Input => egui::Color32::LIGHT_RED,
                        crate::assets::PinKind::Output => egui::Color32::LIGHT_GREEN,
                    };

                    ui.painter().circle_filled(
                        pin_screen_pos,
                        self.canvas_config.base_pin_size,
                        pin_color,
                    );

                    let has_current = self.is_on(Pin::new(id, pin_index as u32, pins[pin_index]));

                    if has_current {
                        ui.painter().circle_stroke(
                            pin_screen_pos,
                            self.canvas_config.base_pin_size + 3.0,
                            egui::Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                        );
                    }

                    let pin_rect = Rect::from_center_size(
                        pin_screen_pos,
                        Vec2::splat(self.canvas_config.base_pin_size + PIN_HOVER_THRESHOLD),
                    );
                    let pin_resp = ui.allocate_rect(pin_rect, Sense::drag());
                    let pin = Pin::new(id, pin_index as u32, pins[pin_index]);
                    if pin_resp.hovered() {
                        self.hovered = Some(Hover::Pin(pin));
                    }
                    if pin_resp.dragged() {
                        self.selected.clear();
                        self.set_drag(Drag::PinToWire {
                            source_pin: pin,
                            start_pos: pin_pos_world,
                        });
                    }
                }
            };

            place_pins(&input_indices, left_x);
            place_pins(&output_indices, right_x);
        }
    }

    pub fn draw_wire(&mut self, ui: &mut Ui, id: InstanceId, hovered: bool, has_current: bool) {
        let mut color = if has_current {
            COLOR_WIRE_POWERED
        } else {
            COLOR_WIRE_IDLE
        };
        if hovered {
            color = COLOR_WIRE_HOVER;
        }
        let wire = *self.db.get_wire(id);
        let mut pin_interact = false;

        for (i, pin_pos) in [wire.start, wire.end].iter().enumerate() {
            let pin_pos = self.adjusted_pos(*pin_pos);
            let kind = if i == wire.input_index as usize {
                PinKind::Input
            } else {
                PinKind::Output
            };
            let pin = Pin::new(id, i as u32, kind);
            let rect = Rect::from_center_size(
                pin_pos,
                Vec2::splat(self.canvas_config.base_pin_size + PIN_HOVER_THRESHOLD),
            );
            let pin_resp = ui.allocate_rect(rect, Sense::click_and_drag());
            if pin_resp.hovered() {
                self.hovered = Some(Hover::Pin(pin));
                pin_interact = true;
            }
            if pin_resp.clicked() {
                self.selected.clear();
                self.selected.insert(id);
                pin_interact = true;
            }
            if pin_resp.dragged() {
                if self.selected.contains(&id) {
                    self.set_drag(Drag::Resize {
                        id: pin.ins,
                        start: pin.index != 1,
                    });
                } else {
                    self.set_drag(Drag::PinToWire {
                        source_pin: pin,
                        start_pos: pin_pos,
                    });
                }
                pin_interact = true;
            }
            let mut pin_color = if i == wire.input_index as usize {
                Color32::RED
            } else {
                Color32::GREEN
            };
            // Only show red/green if pin is not connected
            let is_connected = self
                .db
                .circuit
                .connections
                .iter()
                .any(|conn| conn.a == pin || conn.b == pin);
            if is_connected {
                pin_color = color;
            }
            ui.painter()
                .circle(pin_pos, PIN_HOVER_THRESHOLD / 2.0, pin_color, Stroke::NONE);
        }

        let start = self.adjusted_pos(wire.start);
        let end = self.adjusted_pos(wire.end);

        let hit_wire = if pin_interact {
            false
        } else if let Some(mouse_world) = self.mouse_pos_world(ui) {
            let dist = wire.dist_to_closest_point_on_line(mouse_world);
            dist < WIRE_HIT_DISTANCE
        } else {
            false
        };

        if hit_wire {
            self.hovered = Some(Hover::Instance(id));
            color = COLOR_WIRE_HOVER;

            if ui.input(|i| i.pointer.primary_clicked()) {
                self.selected.clear();
                self.selected.insert(id);
            }

            if self.selected.len() == 1
                && self.selected.contains(&id)
                && let Some(mouse) = self.mouse_pos_world(ui)
                && let Some(split_point) = self.wire_branching_action_point(mouse, id)
            {
                ui.painter().circle_filled(
                    split_point,
                    PIN_HOVER_THRESHOLD,
                    COLOR_HOVER_PIN_TO_WIRE,
                );
            }

            if ui.input(|i| i.pointer.primary_down())
                && let Some(mouse) = self.mouse_pos_world(ui)
            {
                if self.selected.len() == 1
                    && self.selected.contains(&id)
                    && let Some(split_point) = self.wire_branching_action_point(mouse, id)
                {
                    ui.painter().circle_filled(
                        split_point,
                        PIN_HOVER_THRESHOLD,
                        COLOR_HOVER_PIN_TO_WIRE,
                    );
                    self.set_drag(Drag::BranchWire {
                        original_wire_id: id,
                        split_point,
                        start_mouse_pos: mouse,
                    });
                } else {
                    let wire_center = pos2(
                        (wire.start.x + wire.end.x) * 0.5,
                        (wire.start.y + wire.end.y) * 0.5,
                    );
                    let offset = wire_center - mouse;
                    self.set_drag(Drag::Canvas(CanvasDrag::Single { id, offset }));
                }
            }
        }

        ui.painter().line_segment(
            [start, end],
            Stroke::new(self.canvas_config.wire_thickness, color),
        );
    }
    fn draw_label(&mut self, ui: &mut Ui, id: LabelId) {
        let (pos, text) = {
            let label = self.db.get_label(id);
            (label.pos, label.text.clone())
        };
        let screen_pos = pos - self.viewport_offset;

        let is_editing = matches!(self.editing_label, Some(editing_id) if editing_id == id);

        let text_color = if ui.visuals().dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        };

        if is_editing {
            let text_size = ui
                .painter()
                .layout_no_wrap(
                    self.label_edit_buffer.clone(),
                    egui::FontId::proportional(LABEL_EDIT_TEXT_SIZE),
                    text_color,
                )
                .size();

            let text_edit_size = vec2(text_size.x.max(100.0), text_size.y);
            let rect = Rect::from_center_size(screen_pos, text_edit_size + vec2(8.0, 4.0));

            let text_edit = egui::TextEdit::singleline(&mut self.label_edit_buffer)
                .desired_width(text_edit_size.x)
                .font(egui::FontId::proportional(LABEL_EDIT_TEXT_SIZE));

            ui.put(rect, text_edit).request_focus();
        } else {
            let text_size = ui
                .painter()
                .layout_no_wrap(
                    text.clone(),
                    egui::FontId::proportional(LABEL_DISPLAY_TEXT_SIZE),
                    text_color,
                )
                .size();

            let rect = Rect::from_center_size(screen_pos, text_size + vec2(8.0, 4.0));

            let response = ui.allocate_rect(rect, Sense::click());

            ui.painter().text(
                screen_pos,
                egui::Align2::CENTER_CENTER,
                &text,
                egui::FontId::proportional(LABEL_DISPLAY_TEXT_SIZE),
                text_color,
            );

            if response.double_clicked() {
                self.editing_label = Some(id);
                self.label_edit_buffer = text;
            }
            if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::D)) {
                self.delete_label(id);
            }
        }
    }

    fn highlight_hovered(&self, ui: &Ui) {
        let Some(hovered) = self.hovered else {
            return;
        };

        match hovered {
            Hover::Pin(pin) => {
                let color = COLOR_HOVER_PIN_TO_WIRE;
                let pin_pos = self.db.pin_position(pin, &self.canvas_config) - self.viewport_offset;
                ui.painter()
                    .circle_filled(pin_pos, PIN_HOVER_THRESHOLD, color);
            }
            Hover::Instance(hovered) => match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    let gate = self.db.get_gate(hovered);
                    let outer = Rect::from_center_size(
                        gate.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Power => {
                    let power = self.db.get_power(hovered);
                    let outer = Rect::from_center_size(
                        power.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Lamp => {
                    let lamp = self.db.get_lamp(hovered);
                    let outer = Rect::from_center_size(
                        lamp.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Clock => {
                    let clock = self.db.get_clock(hovered);
                    let outer = Rect::from_center_size(
                        clock.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                // Wire is highlighted when drawing
                InstanceKind::Wire => {}
                InstanceKind::Module(_) => {
                    let cc = self.db.get_module(hovered);
                    let outer = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
            },
        }
    }

    fn draw_selection_highlight(&self, ui: &Ui) {
        for &id in &self.selected {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    let r = Rect::from_center_size(
                        g.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    let r = Rect::from_center_size(
                        p.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    let r = Rect::from_center_size(
                        l.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    let r = Rect::from_center_size(
                        c.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Wire => {
                    for pin in self.circuit().pins_of(id) {
                        let pos = self.db.pin_position(pin, &self.canvas_config);
                        ui.painter().circle_filled(
                            pos - self.viewport_offset,
                            PIN_MOVE_HINT_D,
                            PIN_MOVE_HINT_COLOR,
                        );
                    }
                }
                InstanceKind::Module(_) => {
                    let cc = self.db.get_module(id);
                    let r = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_THICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
            }
        }
    }

    fn debug_string(&self, ui: &Ui) -> String {
        let mut out = String::new();
        let mouse_pos_world = self.mouse_pos_world(ui);
        writeln!(out, "mouse: {mouse_pos_world:?}").ok();

        writeln!(out, "hovered: {:?}", self.hovered).ok();
        writeln!(out, "drag: {:?}", self.drag).ok();
        writeln!(out, "viewport_offset: {:?}", self.viewport_offset).ok();
        writeln!(out, "potential_conns: {}", self.potential_connections.len()).ok();
        writeln!(out, "clipboard: {:?}", self.clipboard).ok();
        writeln!(out, "selected: {:?}", self.selected).ok();
        writeln!(out, "editing_label: {:?}", self.editing_label).ok();
        writeln!(out, "label_edit_buffer: {}", self.label_edit_buffer).ok();

        // Simulation status
        writeln!(out, "\n=== Simulation Status ===").ok();
        writeln!(out, "needs update {}", self.current_dirty).ok();
        match self.simulator.status {
            SimulationStatus::Stable { iterations } => {
                writeln!(out, "Status: STABLE (after {iterations} iterations)").ok();
            }
            SimulationStatus::Unstable { max_reached } => {
                if max_reached {
                    let iters = self.simulator.last_iterations;
                    writeln!(out, "Status: UNSTABLE (max iterations: {iters})").ok();
                } else {
                    writeln!(out, "Status: UNSTABLE").ok();
                }
            }
            SimulationStatus::Running => {
                writeln!(out, "Status: RUNNING...").ok();
            }
        }
        let iters = self.simulator.last_iterations;
        writeln!(out, "Iterations: {iters}").ok();

        // Clock controller state
        writeln!(out, "\n--- Clock Controller ---").ok();
        writeln!(out, "State: {:?}", self.clock_controller.state).ok();
        writeln!(
            out,
            "Tick interval: {:.2}s",
            self.clock_controller.tick_interval
        )
        .ok();
        writeln!(
            out,
            "Tick accumulator: {:.3}s",
            self.clock_controller.tick_accumulator
        )
        .ok();
        writeln!(out, "Voltage: {}", self.clock_controller.voltage).ok();

        if self.potential_connections.is_empty() {
            writeln!(out, "\nPotential Connections: none").ok();
        } else {
            writeln!(out, "\nPotential Connections:").ok();
            for c in &self.potential_connections {
                writeln!(out, "  {}", c.display(self.circuit())).ok();
            }
        }

        writeln!(out, "\n{}", self.connection_manager.debug_info()).ok();

        writeln!(out, "\n").ok();

        out.write_str(&self.circuit().display()).ok();

        if !self.db.module_definitions.is_empty() {
            writeln!(out, "\nModule Def:").ok();
            let mut iter = self.db.module_definitions.iter();
            if let Some((_id, first)) = iter.next() {
                writeln!(out, "  {}", first.display_definition()).ok();
            }
            for (_id, m) in iter {
                writeln!(out).ok(); // blank line
                writeln!(out, "  {}", m.display_definition()).ok();
            }
        }

        out
    }

    pub fn extract_instances_with_offsets(
        &self,
        instances: &HashSet<InstanceId>,
    ) -> (Rect, Vec<ClipBoardItem>) {
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
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    points.push(l.pos);
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    points.push(c.pos);
                }
                InstanceKind::Module(_) => {
                    let cc = self.db.get_module(id);
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
                    object_pos.push(ClipBoardItem::Gate(kind, center - g.pos));
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    object_pos.push(ClipBoardItem::Power(center - p.pos));
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(id);
                    object_pos.push(ClipBoardItem::Wire(center - w.start, center - w.end));
                }
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    object_pos.push(ClipBoardItem::Lamp(center - l.pos));
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    object_pos.push(ClipBoardItem::Clock(center - c.pos));
                }
                InstanceKind::Module(_) => {
                    let cc = self.db.get_module(id);
                    object_pos.push(ClipBoardItem::Module(cc.definition_index, center - cc.pos));
                }
            }
        }

        (rect, object_pos)
    }

    fn copy_to_clipboard(&mut self) {
        let instances = if self.selected.is_empty() {
            if let Some(hovered) = self.hovered {
                &HashSet::from_iter(vec![hovered.instance()])
            } else {
                &HashSet::new()
            }
        } else {
            &self.selected
        };
        if instances.is_empty() {
            return;
        }
        let (_, object_pos) = self.extract_instances_with_offsets(instances);
        self.clipboard = object_pos;
    }

    fn paste_from_clipboard(&mut self, mouse: Pos2) {
        self.selected.clear();
        for to_paste in self.clipboard.clone() {
            match to_paste {
                ClipBoardItem::Gate(gate_kind, offset) => {
                    let id = self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos: mouse - offset,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Power(offset) => {
                    let id = self.db.new_power(Power {
                        pos: mouse - offset,
                        on: false,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Wire(s, e) => {
                    let id = self.db.new_wire(Wire::new(mouse - s, mouse - e));
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Module(def_index, offset) => {
                    let id = self.db.new_module(Module {
                        pos: mouse - offset,
                        definition_index: def_index,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Lamp(offset) => {
                    let id = self.db.new_lamp(Lamp {
                        pos: mouse - offset,
                    });
                    self.selected.insert(id);
                }
                ClipBoardItem::Clock(offset) => {
                    let id = self.db.new_clock(Clock {
                        pos: mouse - offset,
                    });
                    self.selected.insert(id);
                }
                ClipBoardItem::Label(text, offset) => {
                    let _id = self.db.new_label(Label {
                        pos: mouse - offset,
                        text,
                    });
                }
            }
        }
        self.connection_manager
            .rebuild_spatial_index(&self.db.circuit);
        self.current_dirty = true;
    }

    fn highlight_selected_actions(&mut self, ui: &Ui, mouse: Option<Pos2>, mouse_down: bool) {
        let Some(selected) = self.selected.iter().next() else {
            return;
        };
        let selected = *selected;

        match self.db.ty(selected) {
            InstanceKind::Wire => {
                for pin in self.circuit().pins_of(selected) {
                    let pos = self.db.pin_position(pin, &self.canvas_config);
                    ui.painter().circle_filled(
                        pos - self.viewport_offset,
                        PIN_MOVE_HINT_D,
                        PIN_MOVE_HINT_COLOR,
                    );

                    if let Some(mouse) = mouse
                        && mouse_down
                        && mouse.distance(pos) < PIN_MOVE_HINT_D
                    {
                        self.set_drag(Drag::Resize {
                            id: selected,
                            start: pin.index == 0,
                        });
                    }
                }
            }
            InstanceKind::Gate(_)
            | InstanceKind::Power
            | InstanceKind::Lamp
            | InstanceKind::Clock
            | InstanceKind::Module(_) => {}
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

    fn create_module(&mut self) {
        self.creating_module = true;
        self.module_name_buffer = format!("module {}", self.db.module_definitions.len() + 1);
        self.module_creation_error = None;
    }

    fn confirm_module_creation(&mut self) {
        // Validate the module name and clone it to avoid borrow issues
        let name = self.module_name_buffer.trim().to_owned();
        if name.is_empty() {
            self.module_creation_error = Some("Module name cannot be empty".to_owned());
            return;
        }

        match self.create_module_definition(name.clone(), &self.selected.clone()) {
            Ok(()) => {
                log::info!("module created successfully: {name}");
                self.selected.clear();
                // Close the dialog
                self.creating_module = false;
                self.module_name_buffer.clear();
                self.module_creation_error = None;
            }
            Err(e) => {
                log::error!("Failed to create module: {e}");
                self.module_creation_error = Some(format!("Failed to create module: {e}"));
            }
        }
    }

    // Adjust position of an object to this screen
    fn adjusted_pos(&self, pos: Pos2) -> Pos2 {
        pos - self.viewport_offset
    }

    fn mouse_pos_world(&self, ui: &Ui) -> Option<Pos2> {
        ui.ctx()
            .pointer_interact_pos()
            .map(|p| p + self.viewport_offset)
    }
}

fn get_icon<'a>(ui: &Ui, source: egui::ImageSource<'a>) -> Image<'a> {
    let mut image = egui::Image::new(source);

    if ui.visuals().dark_mode {
        image = image.bg_fill(Color32::WHITE);
    }

    image
}
