use std::{collections::HashSet, hash::Hash};

use egui::{
    Align, Button, Color32, Image, Layout, Pos2, Rect, Sense, Stroke, Ui, Vec2, Widget as _, pos2,
    vec2,
};

use crate::{assets, config::CanvasConfig};

// TODO Direction is not used anymore. I can calculate it from current positions?
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum Direction {
    Up,
    #[default]
    Right,
    Down,
    Left,
}

impl Direction {
    fn _rotate_cw(self) -> Self {
        match self {
            Self::Up => Self::Right,
            Self::Right => Self::Down,
            Self::Down => Self::Left,
            Self::Left => Self::Up,
        }
    }
}

/// All possible things that can appear on the screen.
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Instance {
    id: InstanceId,
    ty: InstanceType,
}

impl Instance {
    /// Clamp a desired move so this instance stays within rect
    fn clamp_move(&self, move_vec: Vec2, rect: Rect, half_extent: Vec2) -> Vec2 {
        match self.ty {
            InstanceType::Gate(g) => {
                let target = g.pos + move_vec;
                let clamped_x = target
                    .x
                    .clamp(rect.left() + half_extent.x, rect.right() - half_extent.x);
                let clamped_y = target
                    .y
                    .clamp(rect.top() + half_extent.y, rect.bottom() - half_extent.y);
                vec2(clamped_x - g.pos.x, clamped_y - g.pos.y)
            }
            InstanceType::Power(p) => {
                let target = p.pos + move_vec;
                let clamped_x = target
                    .x
                    .clamp(rect.left() + half_extent.x, rect.right() - half_extent.x);
                let clamped_y = target
                    .y
                    .clamp(rect.top() + half_extent.y, rect.bottom() - half_extent.y);
                vec2(clamped_x - p.pos.x, clamped_y - p.pos.y)
            }
            InstanceType::Wire(w) => {
                let ts = w.start + move_vec;
                let te = w.end + move_vec;
                let sx = ts.x.clamp(rect.left(), rect.right());
                let sy = ts.y.clamp(rect.top(), rect.bottom());
                let ex = te.x.clamp(rect.left(), rect.right());
                let ey = te.y.clamp(rect.top(), rect.bottom());
                let safe_dx_start = sx - w.start.x;
                let safe_dy_start = sy - w.start.y;
                let safe_dx_end = ex - w.end.x;
                let safe_dy_end = ey - w.end.y;
                let safe_dx = if move_vec.x.is_sign_positive() {
                    safe_dx_start.min(safe_dx_end)
                } else {
                    safe_dx_start.max(safe_dx_end)
                };
                let safe_dy = if move_vec.y.is_sign_positive() {
                    safe_dy_start.min(safe_dy_end)
                } else {
                    safe_dy_start.max(safe_dy_end)
                };
                vec2(safe_dx, safe_dy)
            }
        }
    }
    fn _new_wire(id: InstanceId, start: Pos2, end: Pos2) -> Self {
        Self {
            id,
            ty: InstanceType::new_wire(start, end),
        }
    }
    fn _new_gate(id: InstanceId, kind: GateKind, pos: Pos2) -> Self {
        Self {
            id,
            ty: InstanceType::new_gate(kind, pos),
        }
    }
    fn new(id: InstanceId, ty: InstanceType) -> Self {
        Self { id, ty }
    }

    // TODO: Reuse array instead of creating
    fn pins(&self) -> Vec<Pin> {
        let mut pins = Vec::new();

        match self.ty {
            InstanceType::Gate(gate_instance) => {
                for (i, pin) in gate_instance.kind.graphics().pins.iter().enumerate() {
                    let pin_pos = gate_instance.pos + pin.offset;
                    pins.push(Pin {
                        pos: pin_pos,
                        ins: self.id,
                        index: i as u32,
                    });
                }
            }
            InstanceType::Power(power_instance) => {
                for (i, pin) in power_instance.graphics().pins.iter().enumerate() {
                    let pin_pos = power_instance.pos + pin.offset;
                    pins.push(Pin {
                        pos: pin_pos,
                        ins: self.id,
                        index: i as u32,
                    });
                }
            }
            InstanceType::Wire(wire_instance) => {
                pins.push(Pin {
                    pos: wire_instance.start,
                    ins: self.id,
                    index: 0,
                });
                pins.push(Pin {
                    pos: wire_instance.end,
                    ins: self.id,
                    index: 1,
                });
            }
        }

        pins
    }

    fn mov(&mut self, move_vec: Vec2) {
        match &mut self.ty {
            InstanceType::Wire(wire) => {
                wire.start += move_vec;
                wire.end += move_vec;
            }
            InstanceType::Gate(gate) => {
                gate.pos += move_vec;
            }
            InstanceType::Power(power) => {
                power.pos += move_vec;
            }
        }
    }

    fn move_pin(&mut self, pin: Pin, new_pos: Pos2) {
        match &mut self.ty {
            InstanceType::Wire(wire) => {
                // TODO: Maybe wire.start and wire.end can go to a vector of size 2?
                if pin.index == 0 {
                    wire.start = new_pos;
                } else {
                    wire.end = new_pos;
                }
            }
            InstanceType::Gate(gate) => {
                let move_vec = new_pos - pin.pos;
                gate.pos += move_vec;
            }
            InstanceType::Power(power) => {
                let move_vec = new_pos - pin.pos;
                power.pos += move_vec;
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceType {
    Gate(GateInstance),
    Wire(WireInstance),
    Power(PowerInstance),
}

impl InstanceType {
    fn new_wire(start: Pos2, end: Pos2) -> Self {
        Self::Wire(WireInstance { start, end })
    }

    pub fn new_wire_from_point(p: Pos2) -> Self {
        Self::new_wire(p, pos2(p.x + 30.0, p.y))
    }
    fn new_gate(kind: GateKind, pos: Pos2) -> Self {
        Self::Gate(GateInstance { kind, pos })
    }
    fn new_power(pos: Pos2, on: bool) -> Self {
        Self::Power(PowerInstance { pos, on })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct GateInstance {
    kind: GateKind,
    // Center position
    pos: Pos2,
}

impl GateInstance {}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum GateKind {
    Nand,
}

impl GateKind {
    fn graphics(&self) -> &assets::InstanceGraphics {
        match self {
            Self::Nand => &assets::NAND_GRAPHICS,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct PowerInstance {
    // Center position
    pos: Pos2,
    on: bool,
}

impl PowerInstance {
    fn graphics(&self) -> &assets::InstanceGraphics {
        if self.on {
            &assets::POWER_ON_GRAPHICS
        } else {
            &assets::POWER_OFF_GRAPHICS
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct WireInstance {
    pub start: Pos2,
    pub end: Pos2,
}

impl WireInstance {
    fn rotate_cw(&mut self) {
        let origin = (self.start.to_vec2() + self.end.to_vec2()) / 2.0;
        // 0 -1
        // 1 0
        self.start = rotate_point(self.start, origin.to_pos2(), std::f32::consts::FRAC_PI_2);
        self.end = rotate_point(self.end, origin.to_pos2(), std::f32::consts::FRAC_PI_2);
    }
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Default,
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Copy,
)]
pub struct InstanceId(u32);

impl InstanceId {
    pub fn incr(&mut self) {
        self.0 += 1;
    }

    pub fn usize(&self) -> usize {
        self.0 as usize
    }
}

impl From<u32> for InstanceId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<InstanceId> for u32 {
    fn from(val: InstanceId) -> Self {
        val.0
    }
}

impl From<InstanceId> for usize {
    fn from(val: InstanceId) -> Self {
        val.0 as Self
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Resize {
    pub id: InstanceId,
    pub start: bool,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct PanelDrag {
    ty: InstanceType,
    /// Temporary drag position
    pos: Pos2,
}

impl PanelDrag {
    fn new(ty: InstanceType) -> Self {
        Self {
            ty,
            pos: pos2(0.0, 0.0),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct CanvasDrag {
    id: InstanceId,
    /// Offset from mouse pointer to gate center at drag start
    offset: Vec2,
}

impl CanvasDrag {
    fn new(id: InstanceId, offset: Vec2) -> Self {
        Self { id, offset }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub pos: Pos2,
    pub ins: InstanceId,
    pub index: u32,
}

impl Hash for Pin {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ins.hash(state);
        self.index.hash(state);
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Connection {
    pin1: Pin,
    pin2: Pin,
}

impl Connection {
    fn new(pin1: Pin, pin2: Pin) -> Self {
        Self { pin1, pin2 }
    }
}

//

#[derive(serde::Deserialize, serde::Serialize)]
pub struct TemplateApp {
    /// State
    ///
    instances: Vec<Instance>,
    /// Connected instance pairs
    connections: HashSet<Connection>,
    /// Next unique ID for gates
    next_instance_id: InstanceId,

    /// If a drag from panel is happening
    panel_drag: Option<PanelDrag>,

    /// Currently dragged gate id from canvas
    canvas_drag: Option<CanvasDrag>,

    /// An item is being resized
    resize: Option<Resize>,

    /// Config
    canvas_config: CanvasConfig,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            instances: Default::default(),
            connections: Default::default(),
            next_instance_id: InstanceId(0),
            panel_drag: None,
            canvas_drag: None,
            resize: None,
            canvas_config: CanvasConfig::default(),
        }
    }
}

impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register all supported image loaders
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    pub fn main_layout(&mut self, ui: &mut Ui) {
        // egui::Window::new("Debug logs").show(ui.ctx(), |ui| {
        //     egui_logger::logger_ui().show(ui);
        // });
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.canvas_config = CanvasConfig::default();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_sized(
                    vec2(200.0, 50.0),
                    egui::TextEdit::multiline(&mut format!(
                        "world {:#?}\n conn: {:#?}",
                        self.instances, self.connections
                    )),
                )
            });

            ui.vertical(|ui| {
                ui.heading("Logic Gates");
                self.draw_palette(ui);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Canvas");
                self.draw_canvas(ui);
            });
        });
    }

    fn draw_palette(&mut self, ui: &mut Ui) {
        let image = egui::Image::new(GateKind::Nand.graphics().svg.clone()).max_height(70.0);
        let nand_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if nand_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.panel_drag = Some(PanelDrag::new(InstanceType::new_gate(GateKind::Nand, pos)));
            log::debug!("dragged {:#?}", self.panel_drag);
        }

        ui.add_space(8.0);

        let pwr_image = egui::Image::new(assets::POWER_ON_GRAPHICS.svg.clone()).max_height(70.0);
        let pwr_resp = ui.add(egui::ImageButton::new(pwr_image).sense(Sense::click_and_drag()));
        if pwr_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.panel_drag = Some(PanelDrag::new(InstanceType::new_power(pos, true)));
        }

        ui.add_space(8.0);

        let wire_resp = ui.add(
            Button::new("Wire")
                .sense(Sense::click_and_drag())
                .min_size(vec2(48.0, 30.0)),
        );
        if wire_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.panel_drag = Some(PanelDrag::new(InstanceType::new_wire_from_point(pos)));
        }

        ui.add_space(8.0);

        if Button::new("Clear Canvas")
            .min_size(vec2(48.0, 30.0))
            .ui(ui)
            .clicked()
        {
            self.instances.clear();
            self.connections.clear();
            self.next_instance_id = InstanceId(0);
            self.canvas_drag = None;
            self.panel_drag = None;
            self.resize = None;
            self.resize = None;
        }
    }

    #[expect(clippy::too_many_lines)]
    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas_rect = resp.rect;
        let threshold = 10.0;

        if let Some(pd) = &mut self.panel_drag {
            if let Some(mouse) = ui.ctx().pointer_interact_pos() {
                match &mut pd.ty {
                    InstanceType::Gate(g) => {
                        let half_extent = vec2(
                            self.canvas_config.base_gate_size.x * 0.5,
                            self.canvas_config.base_gate_size.y * 0.5,
                        );
                        let desired = mouse - g.pos;
                        let tmp = Instance::new(InstanceId(0), InstanceType::Gate(*g));
                        let delta = tmp.clamp_move(desired, canvas_rect, half_extent);
                        g.pos += delta;
                    }
                    InstanceType::Power(p) => {
                        let half_extent = vec2(
                            self.canvas_config.base_gate_size.x * 0.5,
                            self.canvas_config.base_gate_size.y * 0.5,
                        );
                        let desired = mouse - p.pos;
                        let tmp = Instance::new(InstanceId(0), InstanceType::Power(*p));
                        let delta = tmp.clamp_move(desired, canvas_rect, half_extent);
                        p.pos += delta;
                    }
                    InstanceType::Wire(w) => {
                        let desired = mouse - w.start;
                        let tmp = Instance::new(InstanceId(0), InstanceType::Wire(*w));
                        let delta = tmp.clamp_move(desired, canvas_rect, vec2(0.0, 0.0));
                        w.start += delta;
                        w.end += delta;
                    }
                }
            }
            if inside_rect(&canvas_rect, &pd.ty) {
                match pd.ty {
                    InstanceType::Gate(gate) => self.draw_gate(ui, &gate),
                    InstanceType::Power(power) => self.draw_power(ui, &power),
                    InstanceType::Wire(wire) => Self::draw_wire(ui, &wire),
                }
            }
        }
        let mouse_up = ui.input(|i| i.pointer.any_released());

        // spawn a new gate
        if let Some(panel_drag) = &self.panel_drag
            && mouse_up
            && inside_rect(&canvas_rect, &panel_drag.ty)
        {
            self.instances
                .push(Instance::new(self.next_instance_id, panel_drag.ty));
            self.next_instance_id.incr();
            self.panel_drag = None;
        }
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());

        if right_clicked
            && let Some(mouse_pos) = pointer_pos
            && self.panel_drag.is_none()
            && self.canvas_drag.is_none()
            && self.resize.is_none()
            && let Some(instance) = self.interacted_instance(mouse_pos)
            && let InstanceType::Power(_) = instance.ty
        {
            let id = instance.id;
            if let InstanceType::Power(pi) = &mut self.get_instance_mut(id).ty {
                pi.on = !pi.on;
            }
        }

        if pointer_pressed
            && let Some(mouse_pos) = pointer_pos
            && self.panel_drag.is_none()
            && self.canvas_drag.is_none()
            && self.resize.is_none()
        {
            let i = self.interacted_instance(mouse_pos);
            if let Some(instance) = i {
                match instance.ty {
                    InstanceType::Gate(gate) => {
                        self.canvas_drag = Some(CanvasDrag::new(
                            instance.id,
                            gate.pos.to_vec2() - mouse_pos.to_vec2(),
                        ));
                    }
                    InstanceType::Power(power) => {
                        self.canvas_drag = Some(CanvasDrag::new(
                            instance.id,
                            power.pos.to_vec2() - mouse_pos.to_vec2(),
                        ));
                    }
                    InstanceType::Wire(wire) => {
                        if mouse_pos.distance(wire.end) < 10.0 {
                            self.resize = Some(Resize {
                                id: instance.id,
                                start: false,
                            });
                        } else if mouse_pos.distance(wire.start) < 10.0 {
                            self.resize = Some(Resize {
                                id: instance.id,
                                start: true,
                            });
                        } else {
                            self.canvas_drag = Some(CanvasDrag::new(
                                instance.id,
                                wire.end.to_vec2() - mouse_pos.to_vec2(),
                            ));
                        }
                    }
                }
            }
        }

        // dragging on canvas
        if let (Some(id), Some(mouse_pos), Some(offset)) = {
            let id_opt = self.canvas_drag.as_ref().map(|c| c.id);
            let offset = self.canvas_drag.as_ref().map(|c| c.offset);
            (id_opt, pointer_pos, offset)
        } {
            let instance = self.get_instance_mut(id);
            match &mut instance.ty {
                InstanceType::Gate(gate) => {
                    let desired = (mouse_pos + offset) - gate.pos;
                    self.mov_component_with_connected(id, desired, canvas_rect);
                }
                InstanceType::Power(power) => {
                    let desired = (mouse_pos + offset) - power.pos;
                    self.mov_component_with_connected(id, desired, canvas_rect);
                }
                InstanceType::Wire(wire) => {
                    let desired = (mouse_pos + offset) - wire.end;
                    self.mov_component_with_connected(id, desired, canvas_rect);
                }
            }
        }
        log::info!(
            "drag: {}, resize: {}",
            self.canvas_drag.is_some(),
            self.resize.is_some()
        );

        if self.canvas_drag.is_some() || self.resize.is_some() {
            // TODO: Only need to check this on placement and moving.
            // Also use a better way than iterating on everything.
            let moving_instance_id = {
                if let Some(c) = &self.canvas_drag {
                    c.id
                } else if let Some(r) = &self.resize {
                    r.id
                } else {
                    unreachable!("should not happen");
                }
            };
            let mut possible_connections = HashSet::new();
            for self_ins in &self.instances {
                log::info!("{self_ins:#?}");
                let self_pins = self_ins.pins();
                for self_pin in self_pins {
                    for other_ins in &self.instances {
                        if self_ins.id == other_ins.id {
                            continue;
                        }

                        for other_pin in other_ins.pins() {
                            if self_pin.pos.distance(other_pin.pos) > threshold {
                                continue;
                            }

                            log::info!("{self_pin:#?}, {other_pin:#?}");
                            let t = Connection::new(self_pin, other_pin);
                            let t_other = Connection::new(other_pin, self_pin);
                            // TODO: Use the hashset hashing instead of this check
                            let found = possible_connections.contains(&t)
                                || possible_connections.contains(&t_other);
                            if !found {
                                possible_connections.insert(t);
                            }
                        }
                    }
                }
            }
            // paint connected pins (iterate in a stable order)
            let mut conns: Vec<_> = possible_connections.iter().collect();
            conns.sort_by_key(|c| {
                (
                    c.pin1.pos.x.to_bits(),
                    c.pin1.pos.y.to_bits(),
                    c.pin2.pos.x.to_bits(),
                    c.pin2.pos.y.to_bits(),
                )
            });
            for conn in conns {
                if conn.pin1.ins == moving_instance_id {
                    ui.painter()
                        .circle_filled(conn.pin1.pos, 10.0, Color32::LIGHT_YELLOW);
                } else if conn.pin2.ins == moving_instance_id {
                    ui.painter()
                        .circle_filled(conn.pin2.pos, 10.0, Color32::LIGHT_YELLOW);
                }
            }
            // snap connections together
            if mouse_up {
                // snap in a stable order
                let mut conns: Vec<_> = possible_connections.iter().collect();
                conns.sort_by_key(|c| {
                    (
                        c.pin1.pos.x.to_bits(),
                        c.pin1.pos.y.to_bits(),
                        c.pin2.pos.x.to_bits(),
                        c.pin2.pos.y.to_bits(),
                    )
                });
                for conn in conns {
                    let instance = self.get_instance_mut(conn.pin1.ins);
                    instance.move_pin(conn.pin1, conn.pin2.pos);
                }
                self.connections = possible_connections;
            }
        }

        // expanding
        if let (Some(id), Some(mouse_pos), Some(start)) = {
            let id = self.resize.as_ref().map(|c| c.id);
            let start = self.resize.as_ref().map(|c| c.start);
            (id, pointer_pos, start)
        } {
            let wire = self.get_wire_mut(id);
            let mut p = mouse_pos;
            p.x = p.x.clamp(canvas_rect.left(), canvas_rect.right());
            p.y = p.y.clamp(canvas_rect.top(), canvas_rect.bottom());
            if start {
                wire.start = p;
            } else {
                wire.end = p;
            }
        }

        let r_pressed = ui.input(|i| i.key_pressed(egui::Key::R));
        if r_pressed {
            if let Some(_c_drag) = &self.canvas_drag {
                // TODO: implement instance rotate
            } else if let Some(_p_drag) = &mut self.panel_drag {
                // rotate instance when dragging from panel
            } else if let Some(mouse_pos) = pointer_pos
                && let Some(i) = self.interacted_instance(mouse_pos)
            {
                let wire = self.get_wire_mut(i.id);
                wire.rotate_cw();
            }
        }
        for instance in &self.instances {
            match instance.ty {
                InstanceType::Gate(gate) => {
                    self.draw_gate(ui, &gate);
                }
                InstanceType::Power(power) => {
                    self.draw_power(ui, &power);
                }
                InstanceType::Wire(wire) => {
                    Self::draw_wire(ui, &wire);
                }
            }
        }

        if mouse_up {
            self.resize = None;
            self.canvas_drag = None;
        }
    }

    fn draw_wire(ui: &Ui, wire: &WireInstance) {
        let thickness = 6.0;
        ui.painter().line_segment(
            [wire.start, wire.end],
            Stroke::new(thickness, Color32::LIGHT_RED),
        );
    }

    fn draw_gate(&self, ui: &mut Ui, gate: &GateInstance) {
        let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
        let image = Image::new(gate.kind.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in gate.kind.graphics().pins {
            let pin_pos = gate.pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
        }
    }

    fn draw_power(&self, ui: &mut Ui, power: &PowerInstance) {
        let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
        let image = Image::new(power.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in power.graphics().pins {
            let pin_pos = power.pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
        }
    }

    fn interacted_instance(&self, mouse_pos: Pos2) -> Option<&Instance> {
        let mut i: Option<&Instance> = None;
        for instance in &self.instances {
            match instance.ty {
                InstanceType::Gate(gate) => {
                    let size = self.canvas_config.base_gate_size;
                    let gate_rect = egui::Rect::from_center_size(gate.pos, size);
                    if gate_rect.contains(mouse_pos) {
                        i = Some(instance);
                        break;
                    }
                }
                InstanceType::Power(power) => {
                    let size = self.canvas_config.base_gate_size;
                    let rect = egui::Rect::from_center_size(power.pos, size);
                    if rect.contains(mouse_pos) {
                        i = Some(instance);
                        break;
                    }
                }
                InstanceType::Wire(wire) => {
                    let dist = distance_point_to_segment(mouse_pos, wire.start, wire.end);
                    if dist < 10.0 {
                        i = Some(instance);
                        break;
                    }
                }
            }
        }
        i
    }
}

fn inside_rect(canvas_rect: &Rect, ty: &InstanceType) -> bool {
    match ty {
        InstanceType::Gate(gate_instance) => canvas_rect.contains(gate_instance.pos),
        InstanceType::Power(power_instance) => canvas_rect.contains(power_instance.pos),
        InstanceType::Wire(wire_instance) => {
            canvas_rect.contains(wire_instance.start) && canvas_rect.contains(wire_instance.end)
        }
    }
}

impl TemplateApp {
    fn get_wire_mut(&mut self, id: InstanceId) -> &mut WireInstance {
        match &mut self.get_instance_mut(id).ty {
            InstanceType::Wire(wire_instance) => wire_instance,
            InstanceType::Gate(_) | InstanceType::Power(_) => panic!("Should not happen"),
        }
    }

    fn get_instance_mut(&mut self, id: InstanceId) -> &mut Instance {
        self.instances
            .get_mut(id.usize())
            .expect("should not happen")
    }

    fn get_connected_instances(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut connecteds = Vec::new();
        let mut conns: Vec<_> = self.connections.iter().collect();
        conns.sort_by_key(|c| {
            (
                c.pin1.pos.x.to_bits(),
                c.pin1.pos.y.to_bits(),
                c.pin2.pos.x.to_bits(),
                c.pin2.pos.y.to_bits(),
            )
        });
        for con in conns {
            if con.pin1.ins == id {
                connecteds.push(con.pin2.ins);
            }
            if con.pin2.ins == id {
                connecteds.push(con.pin1.ins);
            }
        }

        connecteds
    }

    fn collect_connected_instances(&self, root: InstanceId) -> Vec<InstanceId> {
        let mut stack = vec![root];
        let mut seen: HashSet<InstanceId> = HashSet::new();
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.insert(id);
            out.push(id);
            for n in self.get_connected_instances(id) {
                stack.push(n);
            }
        }
        out
    }

    fn compute_within_bounds_delta(&self, ids: &[InstanceId], desired: Vec2, rect: Rect) -> Vec2 {
        let half_w = self.canvas_config.base_gate_size.x * 0.5;
        let half_h = self.canvas_config.base_gate_size.y * 0.5;
        let mut dx_min = f32::NEG_INFINITY;
        let mut dx_max = f32::INFINITY;
        let mut dy_min = f32::NEG_INFINITY;
        let mut dy_max = f32::INFINITY;

        for id in ids {
            let ins = &self.instances[id.usize()];
            match ins.ty {
                InstanceType::Gate(g) => {
                    let q = g.pos;
                    let left = rect.left() + half_w;
                    let right = rect.right() - half_w;
                    let top = rect.top() + half_h;
                    let bottom = rect.bottom() - half_h;
                    dx_min = dx_min.max(left - q.x);
                    dx_max = dx_max.min(right - q.x);
                    dy_min = dy_min.max(top - q.y);
                    dy_max = dy_max.min(bottom - q.y);
                }
                InstanceType::Power(p) => {
                    let q = p.pos;
                    let left = rect.left() + half_w;
                    let right = rect.right() - half_w;
                    let top = rect.top() + half_h;
                    let bottom = rect.bottom() - half_h;
                    dx_min = dx_min.max(left - q.x);
                    dx_max = dx_max.min(right - q.x);
                    dy_min = dy_min.max(top - q.y);
                    dy_max = dy_max.min(bottom - q.y);
                }
                InstanceType::Wire(w) => {
                    for q in [w.start, w.end] {
                        let left = rect.left();
                        let right = rect.right();
                        let top = rect.top();
                        let bottom = rect.bottom();
                        dx_min = dx_min.max(left - q.x);
                        dx_max = dx_max.min(right - q.x);
                        dy_min = dy_min.max(top - q.y);
                        dy_max = dy_max.min(bottom - q.y);
                    }
                }
            }
        }
        let safe_dx = desired.x.clamp(dx_min, dx_max);
        let safe_dy = desired.y.clamp(dy_min, dy_max);
        vec2(safe_dx, safe_dy)
    }

    fn mov_component_with_connected(&mut self, id: InstanceId, desired: Vec2, rect: Rect) {
        let ids = self.collect_connected_instances(id);
        let delta = self.compute_within_bounds_delta(&ids, desired, rect);
        for cid in ids {
            self.get_instance_mut(cid).mov(delta);
        }
    }
}

impl eframe::App for TemplateApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_layout(ui);
        });
    }
}

pub fn distance_point_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab: Vec2 = b - a;
    let ap: Vec2 = p - a;

    let ab_len2 = ab.x * ab.x + ab.y * ab.y;
    if ab_len2 == 0.0 {
        return (p - a).length();
    }

    let t = ((ap.x * ab.x + ap.y * ab.y) / ab_len2).clamp(0.0, 1.0);

    let closest = a + ab * t;
    (p - closest).length()
}

fn rotate_point(point: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let s = angle.sin();
    let c = angle.cos();

    let px = point.x - origin.x;
    let py = point.y - origin.y;

    let xnew = px * c - py * s;
    let ynew = px * s + py * c;

    Pos2::new(xnew + origin.x, ynew + origin.y)
}
