use std::collections::HashSet;

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::app::{
    App, COLOR_SELECTION_BOX, Connection, EDGE_THRESHOLD, Gate, InstanceId, InstanceKind, Pin,
    Power, WIRE_HIT_DISTANCE, Wire,
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum Drag {
    Panel { pos: Pos2, kind: InstanceKind },
    Canvas { id: InstanceId, offset: Vec2 },
    Resize { id: InstanceId, start: bool },
    Selecting { start: Pos2 },
    MoveSelection { start: Pos2, has_dragged: bool },
}

impl App {
    pub fn inside_rect(&self, canvas: &Rect, kind: InstanceKind, pos: Pos2) -> bool {
        match kind {
            InstanceKind::Gate(_) | InstanceKind::Power => {
                let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
                canvas.contains_rect(rect)
            }
            InstanceKind::Wire => canvas.contains(pos2(pos.x + 30.0, pos.y)),
        }
    }

    pub fn interacted_instance(&self, mouse_pos: Pos2) -> Option<InstanceId> {
        // Prioritize smaller/overlay items first: Power over Gates, then Wires
        for (k, power) in &self.db.powers {
            let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(k);
            }
        }

        for (k, gate) in &self.db.gates {
            let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(k);
            }
        }

        for (k, wire) in &self.db.wires {
            let dist = distance_point_to_segment(mouse_pos, wire.start, wire.end);
            if dist < WIRE_HIT_DISTANCE {
                return Some(k);
            }
        }
        None
    }

    pub fn handle_drag_start_canvas(&mut self, mouse_pos: Pos2) {
        if self.drag.is_some() {
            return;
        }

        if !self.selected.is_empty() {
            self.drag = Some(Drag::MoveSelection {
                start: mouse_pos,
                has_dragged: false,
            });
            self.potential_connections.clear();
            return;
        }

        if let Some(hovered) = self.hovered {
            match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    if let Some(pin) = self.find_near_pin(hovered, mouse_pos) {
                        self.detach_pin(pin);
                    }
                    let gate = self.db.get_gate(hovered);
                    let offset = gate.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        offset,
                    });
                }
                InstanceKind::Power => {
                    if let Some(pin) = self.find_near_pin(hovered, mouse_pos) {
                        self.detach_pin(pin);
                    }
                    let power = self.db.get_power(hovered);
                    let offset = power.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        offset,
                    });
                }
                InstanceKind::Wire => {
                    let wire = self.db.get_wire(hovered);
                    if mouse_pos.distance(wire.start) < EDGE_THRESHOLD {
                        self.detach_pin(Pin {
                            ins: hovered,
                            index: 0,
                        });
                        self.drag = Some(Drag::Resize {
                            id: hovered,
                            start: true,
                        });
                    } else if mouse_pos.distance(wire.end) < EDGE_THRESHOLD {
                        self.detach_pin(Pin {
                            ins: hovered,
                            index: 1,
                        });
                        self.drag = Some(Drag::Resize {
                            id: hovered,
                            start: false,
                        });
                    } else {
                        let wire_center = pos2(
                            (wire.start.x + wire.end.x) * 0.5,
                            (wire.start.y + wire.end.y) * 0.5,
                        );
                        let offset = wire_center - mouse_pos;
                        self.drag = Some(Drag::Canvas {
                            id: hovered,
                            offset,
                        });
                    }
                }
            }
        } else {
            self.drag = Some(Drag::Selecting { start: mouse_pos });
            self.potential_connections.clear();
        }
    }

    pub fn handle_dragging(&mut self, ui: &mut Ui, mouse: Pos2, canvas_rect: &Rect) {
        match self.drag {
            Some(Drag::Panel { pos: _, kind }) => match kind {
                InstanceKind::Gate(gate_kind) => self.draw_gate_preview(ui, gate_kind, mouse),
                InstanceKind::Power => self.draw_power_preview(ui, mouse),
                InstanceKind::Wire => self.draw_wire(ui, default_wire(mouse), false, false),
            },
            Some(Drag::Selecting { start }) => {
                let min = pos2(start.x.min(mouse.x), start.y.min(mouse.y));
                let max = pos2(start.x.max(mouse.x), start.y.max(mouse.y));
                let rect = Rect::from_min_max(min, max);
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::default(),
                    Stroke::new(1.5, COLOR_SELECTION_BOX),
                    StrokeKind::Outside,
                );
            }
            Some(Drag::MoveSelection {
                start,
                has_dragged: _,
            }) => {
                let desired = mouse - start;
                if desired != Vec2::ZERO {
                    let group_set = self.collect_connected_instances_from_many(&self.selected);
                    let group: Vec<InstanceId> = group_set.iter().copied().collect();
                    if !group.is_empty() {
                        let delta = self.compute_within_bounds_delta(&group, desired, *canvas_rect);
                        if delta != Vec2::ZERO {
                            self.move_nonwires_and_resize_wires(&group, delta);
                            if let Some(Drag::MoveSelection { start, has_dragged }) =
                                self.drag.as_mut()
                            {
                                *start += delta;
                                *has_dragged = true;
                            }
                        }
                    }
                }
                self.potential_connections.clear();
            }
            Some(Drag::Canvas { id, offset }) => {
                let new_pos = mouse + offset;
                match self.db.ty(id) {
                    InstanceKind::Gate(_) | InstanceKind::Power => {
                        let current_pos = if let InstanceKind::Gate(_) = self.db.ty(id) {
                            self.db.get_gate(id).pos
                        } else {
                            self.db.get_power(id).pos
                        };
                        let desired = new_pos - current_pos;
                        let ids = [id];
                        let moved_delta =
                            self.compute_within_bounds_delta(&ids, desired, *canvas_rect);
                        if moved_delta != Vec2::ZERO {
                            self.move_nonwires_and_resize_wires(&ids, moved_delta);
                        }
                    }
                    InstanceKind::Wire => {
                        let w = self.db.get_wire_mut(id);
                        let center = pos2((w.start.x + w.end.x) * 0.5, (w.start.y + w.end.y) * 0.5);
                        let desired = new_pos - center;
                        let delta = clamp_wire_move(w, desired, canvas_rect);
                        w.start += delta;
                        w.end += delta;
                    }
                }

                self.potential_connections = self.compute_potential_connections_for_instance(id);
            }
            Some(Drag::Resize { id, start }) => {
                let mut p = mouse;
                p.x = p.x.clamp(canvas_rect.left(), canvas_rect.right());
                p.y = p.y.clamp(canvas_rect.top(), canvas_rect.bottom());
                let wire = self.db.get_wire_mut(id);
                if start {
                    wire.start = p;
                } else {
                    wire.end = p;
                }

                self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                    ins: id,
                    index: u32::from(!start),
                });
            }
            None => {}
        }

        if let Some(Drag::Panel { pos, kind: _ }) = self.drag.as_mut() {
            *pos = mouse;
        }
    }

    pub fn handle_drag_end(&mut self, canvas_rect: &Rect, mouse_pos: Option<Pos2>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            Drag::Panel { pos, kind } => {
                if !self.inside_rect(canvas_rect, kind, pos) {
                    return;
                }
                match kind {
                    InstanceKind::Gate(gate_kind) => self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos,
                    }),
                    InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                    InstanceKind::Wire => self.db.new_wire(default_wire(pos)),
                };
                self.potential_connections.clear();
                self.current_dirty = true;
            }
            Drag::Selecting { start } => {
                let Some(mouse) = mouse_pos else {
                    return;
                };
                let min = pos2(start.x.min(mouse.x), start.y.min(mouse.y));
                let max = pos2(start.x.max(mouse.x), start.y.max(mouse.y));
                let rect = Rect::from_min_max(min, max);
                let mut sel: HashSet<InstanceId> = HashSet::new();
                for (id, g) in &self.db.gates {
                    let r = Rect::from_center_size(g.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, p) in &self.db.powers {
                    let r = Rect::from_center_size(p.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, w) in &self.db.wires {
                    if rect.contains(w.start) && rect.contains(w.end) {
                        sel.insert(id);
                    }
                }
                self.selected = sel;
                self.potential_connections.clear();
            }
            Drag::MoveSelection {
                start: _,
                has_dragged,
            } => {
                if !has_dragged {
                    self.selected.clear();
                }
                self.potential_connections.clear();
            }
            Drag::Canvas { id, offset: _ } => {
                if !self.potential_connections.is_empty() {
                    self.finalize_connections_for_instance(id, canvas_rect);
                }
                self.potential_connections.clear();
            }
            Drag::Resize { id, start } => {
                if !self.potential_connections.is_empty() {
                    let pin = Pin {
                        ins: id,
                        index: u32::from(!start),
                    };
                    self.finalize_connections_for_pin(pin, canvas_rect);
                }
                self.potential_connections.clear();
            }
        }
    }

    pub fn delete_instance(&mut self, id: InstanceId) {
        self.db.instances.remove(id);
        self.db.types.remove(id);
        self.db.gates.remove(id);
        self.db.powers.remove(id);
        self.db.wires.remove(id);
        self.db.connections.retain(|c| !c.involves_instance(id));
        self.hovered.take();
        self.selected.remove(&id);
        self.current.retain(|p| p.ins != id);
        self.current_dirty = true;
    }

    pub fn compute_potential_connections_for_instance(
        &self,
        id: InstanceId,
    ) -> HashSet<Connection> {
        let mut out = HashSet::new();
        for my_pin in self.db.pins_of(id) {
            let pos = self.db.pin_position(my_pin);
            for (other_id, _) in &self.db.types {
                if other_id == id {
                    continue;
                }
                for other_pin in self.db.pins_of(other_id) {
                    let other_pos = self.db.pin_position(other_pin);
                    if (pos - other_pos).length() <= EDGE_THRESHOLD {
                        out.insert(Connection::new(my_pin, other_pin));
                    }
                }
            }
        }
        out
    }

    pub fn compute_potential_connections_for_pin(&self, pin: Pin) -> HashSet<Connection> {
        let mut out = HashSet::new();
        let pos = self.db.pin_position(pin);
        for (other_id, _) in &self.db.types {
            if other_id == pin.ins {
                continue;
            }
            for other_pin in self.db.pins_of(other_id) {
                let other_pos = self.db.pin_position(other_pin);
                if (pos - other_pos).length() <= EDGE_THRESHOLD {
                    out.insert(Connection::new(pin, other_pin));
                }
            }
        }
        out
    }

    pub fn finalize_connections_for_instance(&mut self, id: InstanceId, canvas_rect: &Rect) {
        let to_add: Vec<Connection> = self
            .potential_connections
            .iter()
            .copied()
            .filter(|c| c.involves_instance(id))
            .collect();
        for c in &to_add {
            let (moving_pin, other_pin) = if c.a.ins == id {
                (c.a, c.b)
            } else {
                (c.b, c.a)
            };
            self.snap_pin_to_other(moving_pin, other_pin, canvas_rect);
        }

        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.involves_instance(id) {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= EDGE_THRESHOLD {
                    new_set.insert(*c);
                }
            } else {
                new_set.insert(*c);
            }
        }
        for c in to_add {
            new_set.insert(c);
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    pub fn finalize_connections_for_pin(&mut self, pin: Pin, canvas_rect: &Rect) {
        let to_add: Vec<Connection> = self
            .potential_connections
            .iter()
            .copied()
            .filter(|c| c.a == pin || c.b == pin)
            .collect();
        for c in &to_add {
            if c.a == pin {
                self.snap_pin_to_other(c.a, c.b, canvas_rect);
            }
            if c.b == pin {
                self.snap_pin_to_other(c.b, c.a, canvas_rect);
            }
        }
        // Rebuild connections set, dropping stale ones for this pin
        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.a == pin || c.b == pin {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= EDGE_THRESHOLD {
                    new_set.insert(*c);
                }
            } else {
                new_set.insert(*c);
            }
        }
        for c in to_add {
            new_set.insert(c);
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    pub fn snap_pin_to_other(&mut self, src: Pin, dst: Pin, canvas_rect: &Rect) {
        let target = self.db.pin_position(dst);
        match self.db.ty(src.ins) {
            InstanceKind::Wire => {
                let w = self.db.get_wire_mut(src.ins);
                if src.index == 0 {
                    w.start = target;
                } else {
                    w.end = target;
                }
            }
            InstanceKind::Gate(gk) => {
                let g = self.db.get_gate_mut(src.ins);
                let info = gk.graphics().pins[src.index as usize];
                let current = g.pos + info.offset;
                let desired = target - current;
                let half = self.canvas_config.base_gate_size * 0.5;
                let delta = clamp_gate_move(g.pos, desired, canvas_rect, half);
                g.pos += delta;
            }
            InstanceKind::Power => {
                let p = self.db.get_power_mut(src.ins);
                let info = crate::assets::POWER_ON_GRAPHICS.pins[src.index as usize];
                let current = p.pos + info.offset;
                let desired = target - current;
                let half = self.canvas_config.base_gate_size * 0.5;
                let delta = clamp_gate_move(p.pos, desired, canvas_rect, half);
                p.pos += delta;
            }
        }
    }

    pub fn find_near_pin(&self, id: InstanceId, mouse: Pos2) -> Option<Pin> {
        for pin in self.db.pins_of(id) {
            let p = self.db.pin_position(pin);
            if mouse.distance(p) <= EDGE_THRESHOLD {
                return Some(pin);
            }
        }
        None
    }

    pub fn detach_pin(&mut self, pin: Pin) {
        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.a == pin || c.b == pin {
                // drop it
            } else {
                new_set.insert(*c);
            }
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    pub fn collect_connected_instances_from_many(
        &self,
        roots: &HashSet<InstanceId>,
    ) -> HashSet<InstanceId> {
        let mut out: HashSet<InstanceId> = HashSet::new();
        let mut seen: HashSet<InstanceId> = HashSet::new();
        let mut stack: Vec<InstanceId> = roots.iter().copied().collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if !matches!(self.db.ty(id), InstanceKind::Wire) {
                out.insert(id);
            }
            for pin in self.db.connected_pins_of_instance(id) {
                stack.push(pin.ins);
            }
        }
        out
    }

    pub fn compute_within_bounds_delta(
        &self,
        ids: &[InstanceId],
        desired: Vec2,
        rect: Rect,
    ) -> Vec2 {
        let half_w = self.canvas_config.base_gate_size.x * 0.5;
        let half_h = self.canvas_config.base_gate_size.y * 0.5;
        let mut dx_min = f32::NEG_INFINITY;
        let mut dx_max = f32::INFINITY;
        let mut dy_min = f32::NEG_INFINITY;
        let mut dy_max = f32::INFINITY;

        for id in ids {
            match self.db.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(*id);
                    let q = g.pos;
                    dx_min = dx_min.max(rect.left() + half_w - q.x);
                    dx_max = dx_max.min(rect.right() - half_w - q.x);
                    dy_min = dy_min.max(rect.top() + half_h - q.y);
                    dy_max = dy_max.min(rect.bottom() - half_h - q.y);
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(*id);
                    let q = p.pos;
                    dx_min = dx_min.max(rect.left() + half_w - q.x);
                    dx_max = dx_max.min(rect.right() - half_w - q.x);
                    dy_min = dy_min.max(rect.top() + half_h - q.y);
                    dy_max = dy_max.min(rect.bottom() - half_h - q.y);
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(*id);
                    for q in [w.start, w.end] {
                        dx_min = dx_min.max(rect.left() - q.x);
                        dx_max = dx_max.min(rect.right() - q.x);
                        dy_min = dy_min.max(rect.top() - q.y);
                        dy_max = dy_max.min(rect.bottom() - q.y);
                    }
                }
            }
        }
        let safe_dx = desired.x.clamp(dx_min, dx_max);
        let safe_dy = desired.y.clamp(dy_min, dy_max);
        vec2(safe_dx, safe_dy)
    }

    pub fn move_nonwires_and_resize_wires(&mut self, ids: &[InstanceId], delta: Vec2) {
        // Move all non-wire instances, then adjust connected wire endpoints
        for id in ids {
            match self.db.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate_mut(*id);
                    g.pos += delta;
                }
                InstanceKind::Power => {
                    let p = self.db.get_power_mut(*id);
                    p.pos += delta;
                }
                InstanceKind::Wire => {}
            }
        }

        // Resize wire endpoints attached to any moved instance
        for id in ids {
            for pin in self.db.connected_pins_of_instance(*id) {
                if matches!(self.db.ty(pin.ins), InstanceKind::Wire) {
                    let w = self.db.get_wire_mut(pin.ins);
                    if pin.index == 0 {
                        w.start += delta;
                    } else {
                        w.end += delta;
                    }
                }
            }
        }
    }
}

pub fn default_wire(pos: Pos2) -> Wire {
    Wire {
        start: pos2(pos.x - 30.0, pos.y),
        end: pos2(pos.x + 30.0, pos.y),
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

pub fn clamp_gate_move(current: Pos2, desired: Vec2, rect: &Rect, half: Vec2) -> Vec2 {
    let target = current + desired;
    let clamped_x = target.x.clamp(rect.left() + half.x, rect.right() - half.x);
    let clamped_y = target.y.clamp(rect.top() + half.y, rect.bottom() - half.y);
    vec2(clamped_x - current.x, clamped_y - current.y)
}

pub fn clamp_wire_move(wire: &Wire, desired: Vec2, rect: &Rect) -> Vec2 {
    let ts = wire.start + desired;
    let te = wire.end + desired;
    let sx = ts.x.clamp(rect.left(), rect.right());
    let sy = ts.y.clamp(rect.top(), rect.bottom());
    let ex = te.x.clamp(rect.left(), rect.right());
    let ey = te.y.clamp(rect.top(), rect.bottom());
    let safe_dx_start = sx - wire.start.x;
    let safe_dy_start = sy - wire.start.y;
    let safe_dx_end = ex - wire.end.x;
    let safe_dy_end = ey - wire.end.y;
    let safe_dx = if desired.x.is_sign_positive() {
        safe_dx_start.min(safe_dx_end)
    } else {
        safe_dx_start.max(safe_dx_end)
    };
    let safe_dy = if desired.y.is_sign_positive() {
        safe_dy_start.min(safe_dy_end)
    } else {
        safe_dy_start.max(safe_dy_end)
    };
    vec2(safe_dx, safe_dy)
}
