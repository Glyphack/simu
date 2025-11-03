use std::collections::HashSet;

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2, pos2};

use crate::app::{App, COLOR_HOVER_PIN_TO_WIRE, COLOR_SELECTION_BOX, MIN_WIRE_SIZE};

use crate::db::{InstanceId, InstanceKind, LabelId, Pin, Wire};

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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
pub enum Drag {
    Canvas(CanvasDrag),
    Label {
        id: LabelId,
        offset: Vec2,
    },
    Resize {
        id: InstanceId,
        start: bool,
    },
    Selecting {
        start: Pos2,
    },
    PinToWire {
        source_pin: Pin,
        start_pos: Pos2,
    },
    BranchWire {
        original_wire_id: InstanceId,
        split_point: Pos2,
        start_mouse_pos: Pos2,
    },
}

impl App {
    pub fn set_drag(&mut self, drag: Drag) {
        if self.drag.is_some() {
            return;
        }
        self.drag = Some(drag);
    }

    pub fn handle_dragging(&mut self, ui: &mut Ui, mouse: Pos2) {
        match self.drag {
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
            Some(Drag::Canvas(canvas_drag)) => match canvas_drag {
                CanvasDrag::Single { id, offset } => {
                    let new_pos = mouse + offset;
                    let moved = match self.db.ty(id) {
                        InstanceKind::Gate(_)
                        | InstanceKind::Power
                        | InstanceKind::Lamp
                        | InstanceKind::Module(_)
                        | InstanceKind::Clock => {
                            let current_pos = match self.db.ty(id) {
                                InstanceKind::Gate(_) => self.db.get_gate(id).pos,
                                InstanceKind::Power => self.db.get_power(id).pos,
                                InstanceKind::Lamp => self.db.get_lamp(id).pos,
                                InstanceKind::Clock => self.db.get_clock(id).pos,
                                InstanceKind::Module(_) => self.db.get_module(id).pos,
                                InstanceKind::Wire => unreachable!(),
                            };
                            let desired = new_pos - current_pos;
                            let ids = [id];
                            self.db.move_nonwires_and_resize_wires(&ids, desired);
                            desired.length_sq() > 0.0
                        }
                        InstanceKind::Wire => {
                            let w = self.db.get_wire_mut(id);
                            let center =
                                pos2((w.start.x + w.end.x) * 0.5, (w.start.y + w.end.y) * 0.5);
                            let desired = new_pos - center;
                            w.start += desired;
                            w.end += desired;
                            desired.length_sq() > 0.0
                        }
                    };

                    if moved {
                        self.connection_manager.mark_instance_dirty(id);
                    }
                }
                CanvasDrag::Selected { start } => {
                    if self.selected.is_empty() {
                        return;
                    }
                    let desired = mouse - start;
                    self.drag = Some(Drag::Canvas(CanvasDrag::Selected { start: mouse }));

                    let group: Vec<InstanceId> = self.selected.iter().copied().collect();
                    self.db.move_nonwires_and_resize_wires(&group, desired);
                    if desired.length_sq() > 0.0 {
                        self.connection_manager.mark_instances_dirty(&group);
                    }
                }
            },
            Some(Drag::Resize { id, start }) => {
                let wire = self.db.get_wire_mut(id);
                let mut moved = false;

                if start {
                    let new_start = mouse;
                    let wire_length = (wire.end - new_start).length();
                    if wire_length >= MIN_WIRE_SIZE && wire.start != new_start {
                        wire.start = new_start;
                        moved = true;
                    }
                } else {
                    let new_end = mouse;
                    let wire_length = (wire.start - new_end).length();
                    if wire_length >= MIN_WIRE_SIZE && wire.end != new_end {
                        wire.end = new_end;
                        moved = true;
                    }
                }

                if moved {
                    self.connection_manager.mark_instance_dirty(id);
                }
            }
            Some(Drag::PinToWire {
                source_pin: _,
                start_pos,
            }) => {
                let drag_distance = (mouse - start_pos).length();

                if drag_distance >= MIN_WIRE_SIZE {
                    let wire = Wire::new(start_pos, mouse);
                    let wire_id = self.db.new_wire(wire);

                    self.drag = Some(Drag::Resize {
                        id: wire_id,
                        start: false,
                    });

                    self.connection_manager.mark_instance_dirty(wire_id);
                } else if drag_distance > 2.0 {
                    ui.painter().line_segment(
                        [
                            start_pos - self.viewport_offset,
                            mouse - self.viewport_offset,
                        ],
                        Stroke::new(2.0, COLOR_HOVER_PIN_TO_WIRE),
                    );
                }
            }
            Some(Drag::BranchWire {
                original_wire_id,
                split_point,
                start_mouse_pos,
            }) => {
                let drag_distance = (mouse - start_mouse_pos).length();

                if drag_distance >= MIN_WIRE_SIZE {
                    self.split_wire_at_point(original_wire_id, split_point);
                    let branch_wire = Wire::new(split_point, mouse);
                    let branch_wire_id = self.db.new_wire(branch_wire);

                    self.drag = Some(Drag::Resize {
                        id: branch_wire_id,
                        start: false,
                    });

                    self.connection_manager.mark_instance_dirty(branch_wire_id);
                } else if drag_distance > 2.0 {
                    ui.painter().line_segment(
                        [
                            split_point - self.viewport_offset,
                            mouse - self.viewport_offset,
                        ],
                        Stroke::new(2.0, COLOR_HOVER_PIN_TO_WIRE),
                    );
                }
            }
            Some(Drag::Label { id, offset }) => {
                let new_pos = mouse + offset;
                let label = self.db.get_label_mut(id);
                if label.pos != new_pos {
                    label.pos = new_pos;
                }
            }
            None => {}
        }

        self.compute_potential_connections();
    }

    pub fn handle_drag_end(&mut self, mouse_pos: Pos2) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            Drag::Canvas(canvas_drag) => match canvas_drag {
                CanvasDrag::Single { id, offset: _ } => {
                    self.connection_manager.mark_instance_dirty(id);
                    self.current_dirty = true;
                }
                CanvasDrag::Selected { start: _ } => {
                    let selected: Vec<InstanceId> = self.selected.iter().copied().collect();
                    self.connection_manager.mark_instances_dirty(&selected);
                    self.current_dirty = true;
                }
            },
            Drag::Selecting { start } => {
                let min = pos2(start.x.min(mouse_pos.x), start.y.min(mouse_pos.y));
                let max = pos2(start.x.max(mouse_pos.x), start.y.max(mouse_pos.y));
                let rect = Rect::from_min_max(min, max);
                let mut sel: HashSet<InstanceId> = HashSet::new();
                for (id, g) in &self.db.circuit.gates {
                    let r = Rect::from_center_size(g.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, p) in &self.db.circuit.powers {
                    let r = Rect::from_center_size(p.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, l) in &self.db.circuit.lamps {
                    let r = Rect::from_center_size(l.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, c) in &self.db.circuit.clocks {
                    let r = Rect::from_center_size(c.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, m) in &self.db.circuit.modules {
                    let r = Rect::from_center_size(m.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, w) in &self.db.circuit.wires {
                    if rect.contains(w.start) && rect.contains(w.end) {
                        sel.insert(id);
                    }
                }
                self.selected = sel;
            }
            Drag::Resize { id, start: _ } => {
                self.connection_manager.mark_instance_dirty(id);
                self.current_dirty = true;
            }
            Drag::PinToWire {
                source_pin: _,
                start_pos: _,
            }
            | Drag::Label { id: _, offset: _ } => {
                // Wire was never created if drag distance was too short
                // Label position already updated during dragging
                // Nothing to clean up
            }
            Drag::BranchWire {
                original_wire_id,
                split_point: _,
                start_mouse_pos: _,
            } => {
                self.connection_manager
                    .mark_instance_dirty(original_wire_id);
                self.current_dirty = true;
            }
        }
        self.connection_manager.rebuild_spatial_index(&self.db);
        self.potential_connections.clear();
    }

    pub fn compute_potential_connections(&mut self) {
        let pins_to_update = self.connection_manager.pins_to_update(&self.db);
        let mut new_connections = Vec::new();
        for &pin in &pins_to_update {
            new_connections.extend(
                self.connection_manager
                    .find_connections_for_pin(&self.db, pin),
            );
        }

        self.potential_connections = new_connections.into_iter().collect();
    }
}
