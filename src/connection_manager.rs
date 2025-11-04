#![allow(clippy::allow_attributes)]
use crate::app::SNAP_THRESHOLD;
use crate::assets;
use crate::config::CanvasConfig;
use crate::db::{Circuit, DB, InstanceId, InstanceKind, Pin};
use egui::Pos2;
use std::collections::{HashMap, HashSet};

const SPATIAL_INDEX_GRID_SIZE: f32 = 100.0;

// A normalized, order-independent connection between two pins
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
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

    pub fn display(&self, db: &DB) -> String {
        format!("{} <-> {}", self.a.display(db), self.b.display(db))
    }

    pub fn get_pin_first(&self, pin: Pin) -> Option<(Pin, Pin)> {
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
        (self.a == other.a && self.b == other.b) || (self.a == other.b && self.b == other.a)
    }
}

impl Eq for Connection {}

impl std::hash::Hash for Connection {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let (p1, p2) = if self.a <= self.b {
            (self.a, self.b)
        } else {
            (self.b, self.a)
        };
        p1.hash(state);
        p2.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridCell(i32, i32);

impl GridCell {
    fn from_pos(pos: Pos2) -> Self {
        Self(
            (pos.x / SPATIAL_INDEX_GRID_SIZE).floor() as i32,
            (pos.y / SPATIAL_INDEX_GRID_SIZE).floor() as i32,
        )
    }

    fn neighbors(&self) -> Vec<Self> {
        let mut neighbors = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                neighbors.push(Self(self.0 + dx, self.1 + dy));
            }
        }
        neighbors
    }
}

#[derive(Default)]
pub struct ConnectionManager {
    /// Instances that need connection updates
    pub(crate) dirty_instances: HashSet<InstanceId>,

    spatial_index: HashMap<GridCell, Vec<Pin>>,

    /// Cache of pin positions to detect when pins move
    pin_position_cache: HashMap<Pin, Pos2>,

    canvas_config: CanvasConfig,
}

impl ConnectionManager {
    pub fn new(db: &Circuit, canvas_config: &CanvasConfig) -> Self {
        let mut new = Self {
            dirty_instances: Default::default(),
            spatial_index: Default::default(),
            pin_position_cache: Default::default(),
            canvas_config: canvas_config.clone(),
        };
        new.rebuild_spatial_index(db);
        new
    }

    /// Mark an instance as needing connection updates
    pub fn mark_instance_dirty(&mut self, instance_id: InstanceId) {
        self.dirty_instances.insert(instance_id);
    }

    /// Mark multiple instances as dirty (useful for group operations)
    pub fn mark_instances_dirty(&mut self, instance_ids: &[InstanceId]) {
        for &id in instance_ids {
            self.dirty_instances.insert(id);
        }
    }

    /// Update the spatial index for all pins in the database
    pub fn rebuild_spatial_index(&mut self, db: &Circuit) {
        self.spatial_index.clear();
        self.pin_position_cache.clear();

        // Index all pins by their grid cell
        for (instance_id, _) in &db.types {
            for pin in db.pins_of(instance_id) {
                let pos = db.pin_position(pin, &self.canvas_config);
                let cell = GridCell::from_pos(pos);

                self.spatial_index.entry(cell).or_default().push(pin);

                self.pin_position_cache.insert(pin, pos);
            }
        }
    }

    /// Update spatial index for specific pins that have moved
    fn update_spatial_index_for_pins(&mut self, db: &Circuit, pins: &[Pin]) {
        for &pin in pins {
            let new_pos = db.pin_position(pin, &self.canvas_config);

            // Remove from old cell if position changed
            if let Some(old_pos) = self.pin_position_cache.get(&pin)
                && *old_pos != new_pos
            {
                let old_cell = GridCell::from_pos(*old_pos);
                if let Some(cell_pins) = self.spatial_index.get_mut(&old_cell) {
                    cell_pins.retain(|&p| p != pin);
                    if cell_pins.is_empty() {
                        self.spatial_index.remove(&old_cell);
                    }
                }
            }

            // Add to new cell
            let new_cell = GridCell::from_pos(new_pos);
            self.spatial_index.entry(new_cell).or_default().push(pin);
        }
    }

    /// Find potential connections for a pin using spatial indexing
    pub fn find_connections_for_pin(&self, db: &Circuit, pin: Pin) -> Vec<Connection> {
        let mut wire_connections = Vec::new();
        let mut non_wire_connections = Vec::new();
        let pin_pos = db.pin_position(pin, &self.canvas_config);
        let cell = GridCell::from_pos(pin_pos);

        // Check this cell and neighboring cells
        for neighbor_cell in cell.neighbors() {
            if let Some(nearby_pins) = self.spatial_index.get(&neighbor_cell) {
                for &other_pin in nearby_pins {
                    if other_pin == pin {
                        continue;
                    }

                    let other_pos = db.pin_position(other_pin, &self.canvas_config);
                    let distance = (pin_pos - other_pos).length();

                    // First pin will move to attach
                    let connection = if self.dirty_instances.contains(&pin.ins) {
                        Connection::new(other_pin, pin)
                    } else {
                        Connection::new(pin, other_pin)
                    };

                    if distance <= SNAP_THRESHOLD && self.validate_connection(connection) {
                        let is_wire = matches!(db.ty(other_pin.ins), InstanceKind::Wire);
                        if is_wire {
                            wire_connections.push(connection);
                        } else {
                            non_wire_connections.push(connection);
                        }
                    }
                }
            }
        }

        // For wire pins, prefer non-wire connections over wire connections
        if matches!(db.ty(pin.ins), InstanceKind::Wire) && !non_wire_connections.is_empty() {
            non_wire_connections
        } else {
            let mut all = non_wire_connections;
            all.extend(wire_connections);
            all
        }
    }

    /// Validate if a connection between two pins is allowed
    fn validate_connection(&self, c: Connection) -> bool {
        if c.a == c.b {
            return false;
        }

        if c.a.ins == c.b.ins {
            return false;
        }

        // One pin must be input, the other output
        if c.a.kind == c.b.kind {
            return false;
        }
        true
    }

    /// Snap a pin to match the position of another pin
    fn snap_pin_to_other(&self, db: &mut Circuit, src: Pin, dst: Pin) {
        let target = db.pin_position(dst, &self.canvas_config);
        match db.ty(src.ins) {
            InstanceKind::Wire => {
                if src.index == 0 {
                    let w = db.get_wire_mut(src.ins);
                    w.start = target;
                } else if src.index == 1 {
                    let w = db.get_wire_mut(src.ins);
                    w.end = target;
                } else {
                    unreachable!();
                }
            }
            InstanceKind::Gate(gk) => {
                let g = db.get_gate_mut(src.ins);
                let info = gk.graphics().pins[src.index as usize];
                let pin_offset = info.offset;
                let current = g.pos + pin_offset;
                let desired = target - current;
                db.move_instance_and_propagate(src.ins, desired, &self.canvas_config);
            }
            InstanceKind::Power => {
                let p = db.get_power_mut(src.ins);
                let info = assets::POWER_ON_GRAPHICS.pins[src.index as usize];
                let pin_offset = info.offset;
                let current = p.pos + pin_offset;
                let desired = target - current;
                db.move_instance_and_propagate(src.ins, desired, &self.canvas_config);
            }
            InstanceKind::Lamp => {
                let l = db.get_lamp_mut(src.ins);
                let info = assets::LAMP_GRAPHICS.pins[src.index as usize];
                let pin_offset = info.offset;
                let current = l.pos + pin_offset;
                let desired = target - current;
                db.move_instance_and_propagate(src.ins, desired, &self.canvas_config);
            }
            InstanceKind::Clock => {
                let c = db.get_clock_mut(src.ins);
                let info = assets::CLOCK_GRAPHICS.pins[src.index as usize];
                let pin_offset = info.offset;
                let current = c.pos + pin_offset;
                let desired = target - current;
                db.move_instance_and_propagate(src.ins, desired, &self.canvas_config);
            }
            InstanceKind::Module(_) => {
                let pin_offset = db.pin_offset(src, &self.canvas_config);
                let cc = db.get_module_mut(src.ins);
                let current = cc.pos + pin_offset;
                let desired = target - current;
                db.move_instance_and_propagate(src.ins, desired, &self.canvas_config);
            }
        }
    }

    pub fn pins_to_update(&mut self, db: &Circuit) -> Vec<Pin> {
        let mut pins_to_update = Vec::new();

        for &instance_id in &self.dirty_instances {
            for pin in db.pins_of(instance_id) {
                pins_to_update.push(pin);
            }
        }
        pins_to_update.sort_unstable();
        pins_to_update.dedup();

        if pins_to_update.len() > db.types.len() / 4 {
            self.rebuild_spatial_index(db);
        } else {
            self.update_spatial_index_for_pins(db, &pins_to_update);
        }
        pins_to_update
    }

    /// Process all dirty entities and update connections
    pub fn update_connections(&mut self, db: &mut Circuit) -> bool {
        if self.dirty_instances.is_empty() {
            return false;
        }
        let pins_to_update = self.pins_to_update(db);
        let mut new_connections = Vec::new();
        for &pin in &pins_to_update {
            new_connections.extend(self.find_connections_for_pin(db, pin));
        }

        let mut connections_to_keep = HashSet::new();
        for connection in &db.connections {
            let keep_connection = !self.dirty_instances.contains(&connection.a.ins)
                && !self.dirty_instances.contains(&connection.b.ins);

            if keep_connection {
                let p1 = db.pin_position(connection.a, &self.canvas_config);
                let p2 = db.pin_position(connection.b, &self.canvas_config);
                if (p1 - p2).length() <= SNAP_THRESHOLD {
                    connections_to_keep.insert(*connection);
                }
            }
        }

        for connection in &new_connections {
            self.snap_pin_to_other(db, connection.a, connection.b);
            connections_to_keep.insert(*connection);
        }

        // Check if connections actually changed
        let connections_changed = db.connections != connections_to_keep;

        db.connections = connections_to_keep;

        self.dirty_instances.clear();

        connections_changed
    }

    /// Get debug information about the connection manager
    pub fn debug_info(&self) -> String {
        format!(
            "ConnectionManager:\n  dirty_instances: {:?}",
            self.dirty_instances
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Connection;
    use crate::{
        assets::PinKind,
        db::{InstanceId, Pin},
    };
    use std::collections::HashSet;

    fn create_test_pins() -> (Pin, Pin, Pin) {
        let id1 = InstanceId::from(1);
        let id2 = InstanceId::from(2);
        let id3 = InstanceId::from(3);
        let pin1 = Pin::new(id1, 1, PinKind::Input);
        let pin2 = Pin::new(id2, 2, PinKind::Input);
        let pin3 = Pin::new(id3, 3, PinKind::Input);
        (pin1, pin2, pin3)
    }

    #[test]
    fn connection_equality_order_independent() {
        let (pin1, pin2, _) = create_test_pins();

        let conn1 = Connection::new(pin1, pin2);
        let conn2 = Connection::new(pin2, pin1);

        assert_eq!(conn1, conn2);
    }

    #[test]
    fn connection_hash_order_independent() {
        let (pin1, pin2, _) = create_test_pins();

        let conn1 = Connection::new(pin1, pin2);
        let conn2 = Connection::new(pin2, pin1);

        let mut set = HashSet::new();
        set.insert(conn1);
        assert!(set.contains(&conn2));
    }

    #[test]
    fn connection_different_pins_not_equal() {
        let (pin1, pin2, pin3) = create_test_pins();

        let conn1 = Connection::new(pin1, pin2);
        let conn2 = Connection::new(pin1, pin3);

        assert_ne!(conn1, conn2);
    }
}
