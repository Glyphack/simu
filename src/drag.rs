use std::collections::HashSet;

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::{
    app::{
        App, COLOR_SELECTION_BOX, Connection, Gate, Hover, InstanceId, InstanceKind, Pin, Power,
        SNAP_THRESHOLD, Wire,
    },
    assets::{PinInfo, PinKind},
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
pub enum CanvasDrag {
    Single {
        id: InstanceId,
        /// Offset to center of the object
        offset: Vec2,
    },
    Selected {
        /// mouse position when dragging started
        start: Pos2,
    },
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum Drag {
    Panel {
        pos: Pos2,
        kind: InstanceKind,
    },
    CanvasNew(CanvasDrag),
    Resize {
        id: InstanceId,
        start: bool,
    },
    Selecting {
        start: Pos2,
    },
    PinToWire {
        source_pin: Pin,
        wire_id: InstanceId,
    },
}

impl App {
    pub fn handle_drag_start_canvas(&mut self, mouse_pos: Pos2) {
        if self.drag.is_some() {
            return;
        }

        self.drag_had_movement = false;

        if !self.selected.is_empty() {
            if !self.selected.len() == 1
                && matches!(
                    self.db
                        .ty(*self.selected.iter().next().expect("checked size")),
                    InstanceKind::Wire
                )
            {
                self.drag = Some(Drag::CanvasNew(CanvasDrag::Selected { start: mouse_pos }));
                return;
            }
        }

        let Some(hovered) = self.hovered else {
            self.drag = Some(Drag::Selecting { start: mouse_pos });
            self.potential_connections.clear();
            return;
        };

        match hovered {
            Hover::Pin(pin) => {
                if matches!(self.db.ty(pin.ins), InstanceKind::Wire) && pin.index <= 1 {
                    self.drag = Some(Drag::Resize {
                        id: pin.ins,
                        start: pin.index == 0,
                    });
                    return;
                }
                let pin_pos = self.db.pin_position(pin);
                let wire_id = self.db.new_wire(Wire::new(pin_pos, mouse_pos));
                // TODO: Connections
                // self.db.connections.insert(Connection::new(
                //     pin,
                //     Pin {
                //         ins: wire_id,
                //         index: 0,
                //     },
                // ));
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
                    self.drag = Some(Drag::CanvasNew(CanvasDrag::Single {
                        id: hovered,
                        offset,
                    }));
                }
                InstanceKind::Power => {
                    let power = self.db.get_power(hovered);
                    let offset = power.pos - mouse_pos;
                    self.drag = Some(Drag::CanvasNew(CanvasDrag::Single {
                        id: hovered,
                        offset,
                    }));
                }
                InstanceKind::Wire => {
                    let wire = self.db.get_wire(hovered);
                    let wire_center = pos2(
                        (wire.start.x + wire.end.x) * 0.5,
                        (wire.start.y + wire.end.y) * 0.5,
                    );
                    let offset = wire_center - mouse_pos;
                    self.drag = Some(Drag::CanvasNew(CanvasDrag::Single {
                        id: hovered,
                        offset,
                    }));
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(hovered);
                    let offset = cc.pos - mouse_pos;
                    self.drag = Some(Drag::CanvasNew(CanvasDrag::Single {
                        id: hovered,
                        offset,
                    }));
                }
            },
        }
    }

    pub fn handle_dragging(&mut self, ui: &mut Ui, mouse: Pos2) {
        match self.drag {
            Some(Drag::Panel { pos: _, kind }) => match kind {
                InstanceKind::Gate(gate_kind) => self.draw_gate_preview(ui, gate_kind, mouse),
                InstanceKind::Power => self.draw_power_preview(ui, mouse),
                InstanceKind::Wire => {
                    self.draw_wire(ui, &default_wire(mouse), false, false, Vec::new())
                }
                InstanceKind::CustomCircuit(def) => {
                    self.draw_custom_circuit_preview(ui, def, mouse);
                }
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
                if (mouse - start).length_sq() > 0.0 {
                    self.drag_had_movement = true;
                }
            }
            Some(Drag::CanvasNew(canvas_drag)) => match canvas_drag {
                CanvasDrag::Single { id, offset } => {
                    let new_pos = mouse + offset;
                    let mut moved = false;
                    match self.db.ty(id) {
                        InstanceKind::Gate(_) | InstanceKind::Power => {
                            let current_pos = if let InstanceKind::Gate(_) = self.db.ty(id) {
                                self.db.get_gate(id).pos
                            } else {
                                self.db.get_power(id).pos
                            };
                            let desired = new_pos - current_pos;
                            let ids = [id];
                            self.move_nonwires_and_resize_wires(&ids, desired);
                            moved = desired.length_sq() > 0.0;
                        }
                        InstanceKind::Wire => {
                            let w = self.db.get_wire_mut(id);
                            let center =
                                pos2((w.start.x + w.end.x) * 0.5, (w.start.y + w.end.y) * 0.5);
                            let desired = new_pos - center;
                            w.start += desired;
                            w.end += desired;
                            moved = desired.length_sq() > 0.0;
                        }
                        InstanceKind::CustomCircuit(_) => {
                            let cc = self.db.get_custom_circuit_mut(id);
                            if cc.pos != new_pos {
                                cc.pos = new_pos;
                                moved = true;
                            }
                        }
                    }

                    self.potential_connections =
                        self.compute_potential_connections_for_instance(id);
                    if moved {
                        self.drag_had_movement = true;
                    }
                }
                CanvasDrag::Selected { start } => {
                    if self.selected.is_empty() {
                        return;
                    }
                    let desired = mouse - start;
                    self.drag = Some(Drag::CanvasNew(CanvasDrag::Selected { start: mouse }));

                    let group: Vec<InstanceId> = self.selected.iter().copied().collect();
                    self.move_nonwires_and_resize_wires(&group, desired);
                    self.potential_connections.clear();
                    for id in group {
                        self.potential_connections
                            .extend(self.compute_potential_connections_for_instance(id));
                    }
                    if desired.length_sq() > 0.0 {
                        self.drag_had_movement = true;
                    }
                }
            },
            Some(Drag::Resize { id, start }) => {
                let wire = self.db.get_wire_mut(id);
                let mut moved = false;
                if start {
                    if wire.start != mouse {
                        wire.start = mouse;
                        moved = true;
                    }
                } else if wire.end != mouse {
                    wire.end = mouse;
                    moved = true;
                }

                self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                    ins: id,
                    index: u32::from(!start),
                });
                if moved {
                    self.drag_had_movement = true;
                }
            }
            Some(Drag::PinToWire {
                source_pin: _,
                wire_id,
            }) => {
                let wire = self.db.get_wire_mut(wire_id);
                if wire.end != mouse {
                    wire.end = mouse;
                    self.drag_had_movement = true;
                }

                // self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                //     ins: wire_id,
                //     index: 1,
                // });
            }
            None => {}
        }

        if let Some(Drag::Panel { pos, kind: _ }) = self.drag.as_mut() {
            *pos = mouse;
        }
    }

    pub fn handle_drag_end(&mut self, mouse_pos: Pos2) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            Drag::Panel { pos, kind } => {
                if let InstanceKind::CustomCircuit(definition_index) = kind
                    && definition_index >= self.db.custom_circuit_definitions.len()
                {
                    return;
                }

                let _id = match kind {
                    InstanceKind::Gate(gate_kind) => self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos,
                    }),
                    InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                    InstanceKind::Wire => {
                        let w = default_wire(pos);
                        self.db.new_wire(w)
                    }
                    InstanceKind::CustomCircuit(definition_index) => {
                        self.db
                            .new_custom_circuit(crate::custom_circuit::CustomCircuit {
                                pos,
                                definition_index,
                            })
                    }
                };
                self.current_dirty = true;
            }
            Drag::CanvasNew(canvas_drag) => match canvas_drag {
                CanvasDrag::Single { id, offset: _ } => {
                    self.finalize_connections_for_instance(id);
                }
                CanvasDrag::Selected { start: _ } => {
                    for id in self.selected.clone() {
                        self.finalize_connections_for_instance(id);
                    }
                }
            },
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
        self.potential_connections.clear();
        self.drag_had_movement = false;
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
            InstanceKind::CustomCircuit(_) => {
                let cc = self.db.get_custom_circuit_mut(src.ins);
                let current = cc.pos;
                let desired = target - current;
                cc.pos += desired;
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
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit_mut(*id);
                    cc.pos += delta;
                }
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
    Wire::new(pos2(pos.x - 30.0, pos.y), pos2(pos.x + 30.0, pos.y))
}
