use std::collections::HashSet;

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2, pos2};

use crate::app::{
    App, COLOR_SELECTION_BOX, Connection, Gate, Hover, InstanceId, InstanceKind, Pin, Power,
    SNAP_THRESHOLD, Wire,
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum Drag {
    Panel {
        pos: Pos2,
        kind: InstanceKind,
    },
    Canvas {
        id: InstanceId,
        offset: Vec2,
        detach: bool,
    },
    Resize {
        id: InstanceId,
        start: bool,
    },
    Selecting {
        start: Pos2,
    },
    MoveSelection {
        start: Pos2,
        has_dragged: bool,
    },
    PinToWire {
        source_pin: Pin,
        wire_id: InstanceId,
    },
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

        let Some(hovered) = self.hovered else {
            self.drag = Some(Drag::Selecting { start: mouse_pos });
            self.potential_connections.clear();
            return;
        };
        match hovered {
            Hover::Pin(pin) => {
                if matches!(self.db.ty(pin.ins), InstanceKind::Wire) {
                    self.drag = Some(Drag::Resize {
                        id: pin.ins,
                        start: pin.index == 0,
                    });
                    return;
                } else if self.is_pin_connected(pin) {
                    let offset = -self.db.pin_offset(pin);
                    self.drag = Some(Drag::Canvas {
                        id: pin.ins,
                        offset,
                        detach: true,
                    });
                    return;
                }
                let pin_pos = self.db.pin_position(pin);
                let wire_id = self.db.new_wire(Wire {
                    start: pin_pos,
                    end: mouse_pos,
                });
                self.db.connections.insert(Connection::new(
                    pin,
                    Pin {
                        ins: wire_id,
                        index: 0,
                    },
                ));
                self.drag = Some(Drag::PinToWire {
                    source_pin: pin,
                    wire_id,
                });
                self.current_dirty = true;
            }
            Hover::Instance(hovered) => match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    let gate = self.db.get_gate(hovered);
                    let offset = gate.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        detach: false,
                        offset,
                    });
                }
                InstanceKind::Power => {
                    let power = self.db.get_power(hovered);
                    let offset = power.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        detach: false,
                        offset,
                    });
                }
                InstanceKind::Wire => {
                    let wire = self.db.get_wire(hovered);
                    let wire_center = pos2(
                        (wire.start.x + wire.end.x) * 0.5,
                        (wire.start.y + wire.end.y) * 0.5,
                    );
                    let offset = wire_center - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        detach: false,
                        offset,
                    });
                }
            },
        }
    }

    pub fn handle_dragging(&mut self, ui: &mut Ui, mouse: Pos2) {
        match self.drag {
            Some(Drag::Panel { pos: _, kind }) => match kind {
                InstanceKind::Gate(gate_kind) => self.draw_gate_preview(ui, gate_kind, mouse),
                InstanceKind::Power => self.draw_power_preview(ui, mouse),
                InstanceKind::Wire => self.draw_wire(ui, default_wire(mouse), false, false),
            },
            Some(Drag::Selecting { start }) => {
                let start_screen = start - self.viewport_offset;
                let mouse_screen = mouse - self.viewport_offset;
                let min = pos2(
                    start_screen.x.min(mouse_screen.x),
                    start_screen.y.min(mouse_screen.y),
                );
                let max = pos2(
                    start_screen.x.max(mouse_screen.x),
                    start_screen.y.max(mouse_screen.y),
                );
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
                        self.move_nonwires_and_resize_wires(&group, desired);
                        if let Some(Drag::MoveSelection { start, has_dragged }) = self.drag.as_mut()
                        {
                            *start += desired;
                            *has_dragged = true;
                        }
                    }
                }
            }
            Some(Drag::Canvas { id, detach, offset }) => {
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
                        if detach {
                            if let InstanceKind::Gate(_) = self.db.ty(id) {
                                self.db.get_gate_mut(id).pos += desired;
                            } else {
                                self.db.get_power_mut(id).pos += desired;
                            }
                        } else {
                            self.move_nonwires_and_resize_wires(&ids, desired);
                        }
                    }
                    InstanceKind::Wire => {
                        let w = self.db.get_wire_mut(id);
                        let center = pos2((w.start.x + w.end.x) * 0.5, (w.start.y + w.end.y) * 0.5);
                        let desired = new_pos - center;
                        w.start += desired;
                        w.end += desired;
                    }
                }

                self.potential_connections = self.compute_potential_connections_for_instance(id);
            }
            Some(Drag::Resize { id, start }) => {
                let wire = self.db.get_wire_mut(id);
                if start {
                    wire.start = mouse;
                } else {
                    wire.end = mouse;
                }

                self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                    ins: id,
                    index: u32::from(!start),
                });
            }
            Some(Drag::PinToWire {
                source_pin: _,
                wire_id,
            }) => {
                let wire = self.db.get_wire_mut(wire_id);
                wire.end = mouse;

                self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                    ins: wire_id,
                    index: 1,
                });
            }
            None => {}
        }

        if let Some(Drag::Panel { pos, kind: _ }) = self.drag.as_mut() {
            *pos = mouse;
        }
    }

    pub fn handle_drag_end(&mut self, canvas_rect: &Rect, mouse_pos: Pos2) {
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
                let min = pos2(start.x.min(mouse_pos.x), start.y.min(mouse_pos.y));
                let max = pos2(start.x.max(mouse_pos.x), start.y.max(mouse_pos.y));
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
            Drag::Canvas {
                id,
                detach: _,
                offset: _,
            } => {
                self.finalize_connections_for_instance(id);
            }
            Drag::Resize { id, start } => {
                let pin = Pin {
                    ins: id,
                    index: u32::from(!start),
                };
                self.finalize_connections_for_pin(pin);
            }
            Drag::PinToWire {
                source_pin: _,
                wire_id,
            } => {
                let pin = Pin {
                    ins: wire_id,
                    index: 1,
                };
                self.finalize_connections_for_pin(pin);
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
        self.drag.take();
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
                    if (pos - other_pos).length() <= SNAP_THRESHOLD {
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
                if (pos - other_pos).length() <= SNAP_THRESHOLD {
                    out.insert(Connection::new(pin, other_pin));
                }
            }
        }
        out
    }

    pub fn finalize_connections_for_instance(&mut self, id: InstanceId) {
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
            self.snap_pin_to_other(moving_pin, other_pin);
        }

        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.involves_instance(id) {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= SNAP_THRESHOLD {
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

    pub fn finalize_connections_for_pin(&mut self, pin: Pin) {
        let to_add: Vec<Connection> = self
            .potential_connections
            .iter()
            .copied()
            .filter(|c| c.a == pin || c.b == pin)
            .collect();
        for c in &to_add {
            if c.a == pin {
                self.snap_pin_to_other(c.a, c.b);
            }
            if c.b == pin {
                self.snap_pin_to_other(c.b, c.a);
            }
        }
        // Rebuild connections set, dropping stale ones for this pin
        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.a == pin || c.b == pin {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= SNAP_THRESHOLD {
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

    pub fn snap_pin_to_other(&mut self, src: Pin, dst: Pin) {
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
                g.pos += desired;
            }
            InstanceKind::Power => {
                let p = self.db.get_power_mut(src.ins);
                let info = crate::assets::POWER_ON_GRAPHICS.pins[src.index as usize];
                let current = p.pos + info.offset;
                let desired = target - current;
                p.pos += desired;
            }
        }
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
