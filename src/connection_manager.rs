use crate::app::{Connection, DB, InstanceId, InstanceKind, Pin, SNAP_THRESHOLD};
use crate::assets;
use egui::Pos2;
use std::collections::{HashMap, HashSet};

const GRID_SIZE: f32 = 100.0; // Size of each grid cell for spatial indexing

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridCell(i32, i32);

impl GridCell {
    fn from_pos(pos: Pos2) -> Self {
        Self(
            (pos.x / GRID_SIZE).floor() as i32,
            (pos.y / GRID_SIZE).floor() as i32,
        )
    }

    /// Get neighboring cells (including this cell) for connection searching
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
    /// Instances that have moved, resized, or changed and need connection updates
    pub(crate) dirty_instances: HashSet<InstanceId>,

    /// Specific pins that need connection updates
    pub(crate) dirty_pins: HashSet<Pin>,

    /// Spatial index mapping grid cells to pins in that region
    spatial_index: HashMap<GridCell, Vec<Pin>>,

    /// Cache of pin positions to detect when pins move
    pin_position_cache: HashMap<Pin, Pos2>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark an instance as needing connection updates
    pub fn mark_instance_dirty(&mut self, instance_id: InstanceId) {
        self.dirty_instances.insert(instance_id);
    }

    /// Mark a specific pin as needing connection updates
    pub fn mark_pin_dirty(&mut self, pin: Pin) {
        self.dirty_pins.insert(pin);
    }

    /// Mark multiple instances as dirty (useful for group operations)
    pub fn mark_instances_dirty(&mut self, instance_ids: &[InstanceId]) {
        for &id in instance_ids {
            self.dirty_instances.insert(id);
        }
    }

    /// Update the spatial index for all pins in the database
    fn rebuild_spatial_index(&mut self, db: &DB) {
        self.spatial_index.clear();
        self.pin_position_cache.clear();

        // Index all pins by their grid cell
        for (instance_id, _) in &db.instances {
            for pin in db.pins_of(instance_id) {
                let pos = db.pin_position(pin);
                let cell = GridCell::from_pos(pos);

                self.spatial_index.entry(cell).or_default().push(pin);

                self.pin_position_cache.insert(pin, pos);
            }
        }
    }

    /// Update spatial index for specific pins that have moved
    fn update_spatial_index_for_pins(&mut self, db: &DB, pins: &[Pin]) {
        for &pin in pins {
            let new_pos = db.pin_position(pin);

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

            self.pin_position_cache.insert(pin, new_pos);
        }
    }

    /// Find potential connections for a pin using spatial indexing
    fn find_potential_connections_for_pin(&self, db: &DB, pin: Pin) -> Vec<Connection> {
        let mut connections = Vec::new();
        let pin_pos = db.pin_position(pin);
        let cell = GridCell::from_pos(pin_pos);

        // Check this cell and neighboring cells
        for neighbor_cell in cell.neighbors() {
            if let Some(nearby_pins) = self.spatial_index.get(&neighbor_cell) {
                for &other_pin in nearby_pins {
                    if other_pin == pin {
                        continue;
                    }

                    let other_pos = db.pin_position(other_pin);
                    let distance = (pin_pos - other_pos).length();

                    if distance <= SNAP_THRESHOLD
                        && let Some(connection) = self.validate_connection(db, pin, other_pin)
                    {
                        connections.push(connection);
                    }
                }
            }
        }

        connections
    }

    /// Validate if a connection between two pins is allowed
    fn validate_connection(&self, db: &DB, pin1: Pin, pin2: Pin) -> Option<Connection> {
        // Don't connect pin to itself
        if pin1 == pin2 {
            return None;
        }

        // Don't connect instance to itself
        if pin1.ins == pin2.ins {
            return None;
        }

        let pin1_kind = self.get_pin_kind(db, pin1);
        let pin2_kind = self.get_pin_kind(db, pin2);

        // Don't connect output to output
        if matches!(pin1_kind, Some(assets::PinKind::Output))
            && matches!(pin2_kind, Some(assets::PinKind::Output))
        {
            return None;
        }

        Some(Connection::new(pin1, pin2))
    }

    /// Get the kind (Input/Output) of a pin
    fn get_pin_kind(&self, db: &DB, pin: Pin) -> Option<assets::PinKind> {
        match db.ty(pin.ins) {
            InstanceKind::Gate(gate_kind) => {
                let graphics = gate_kind.graphics();
                graphics.pins.get(pin.index as usize).map(|p| p.kind)
            }
            InstanceKind::Power => {
                let graphics = &assets::POWER_ON_GRAPHICS;
                graphics.pins.get(pin.index as usize).map(|p| p.kind)
            }
            InstanceKind::Wire => {
                // Wires can connect to anything
                Some(assets::PinKind::Input) // Treat as input for validation purposes
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = db.get_custom_circuit(pin.ins);
                if let Some(def) = db.custom_circuit_definitions.get(cc.definition_index) {
                    def.external_pins.get(pin.index as usize).map(|p| p.kind)
                } else {
                    None
                }
            }
        }
    }

    /// Snap a pin to match the position of another pin
    fn snap_pin_to_other(&self, db: &mut DB, src: Pin, dst: Pin) {
        let target = db.pin_position(dst);
        match db.ty(src.ins) {
            InstanceKind::Wire => {
                if src.index == 0 {
                    let w = db.get_wire_mut(src.ins);
                    w.start = target;
                } else if src.index == 1 {
                    let w = db.get_wire_mut(src.ins);
                    w.end = target;
                } else {
                    // Handle extra pin snapping - move entire wire to preserve parametric position
                    let current_pos = db.pin_position(src);
                    let delta = target - current_pos;
                    let w = db.get_wire_mut(src.ins);
                    w.start += delta;
                    w.end += delta;
                }
            }
            InstanceKind::Gate(gk) => {
                let g = db.get_gate_mut(src.ins);
                let info = gk.graphics().pins[src.index as usize];
                let pin_offset = info.offset;
                let current = g.pos + pin_offset;
                let desired = target - current;
                g.pos += desired;
            }
            InstanceKind::Power => {
                let p = db.get_power_mut(src.ins);
                let info = assets::POWER_ON_GRAPHICS.pins[src.index as usize];
                let pin_offset = info.offset;
                let current = p.pos + pin_offset;
                let desired = target - current;
                p.pos += desired;
            }
            InstanceKind::CustomCircuit(_) => {
                let pin_offset = db.pin_offset(src);
                let cc = db.get_custom_circuit_mut(src.ins);
                let current = cc.pos + pin_offset;
                let desired = target - current;
                cc.pos += desired;
            }
        }
    }

    /// Process all dirty entities and update connections
    pub fn update_connections(&mut self, db: &mut DB) -> bool {
        if self.dirty_instances.is_empty() && self.dirty_pins.is_empty() {
            return false; // No updates needed
        }

        // Collect all pins that need updates
        let mut pins_to_update = Vec::new();

        // Add pins from dirty instances
        for &instance_id in &self.dirty_instances {
            for pin in db.pins_of(instance_id) {
                pins_to_update.push(pin);
            }
        }

        // Add explicitly dirty pins
        pins_to_update.extend(self.dirty_pins.iter().copied());

        // Remove duplicates
        pins_to_update.sort_unstable();
        pins_to_update.dedup();

        // If we have too many pins to update, just rebuild the entire spatial index
        if pins_to_update.len() > db.instances.len() / 4 {
            self.rebuild_spatial_index(db);
        } else {
            self.update_spatial_index_for_pins(db, &pins_to_update);
        }

        // Find potential connections for all dirty pins
        let mut new_connections = Vec::new();
        for &pin in &pins_to_update {
            new_connections.extend(self.find_potential_connections_for_pin(db, pin));
        }

        // Remove old connections involving dirty pins/instances
        let mut connections_to_keep = HashSet::new();
        for connection in &db.connections {
            let keep_connection =
                // Keep connections that don't involve any dirty instances
                !self.dirty_instances.contains(&connection.a.ins)
                && !self.dirty_instances.contains(&connection.b.ins)
                && !self.dirty_pins.contains(&connection.a)
                && !self.dirty_pins.contains(&connection.b);

            if keep_connection {
                // But only keep them if they're still close enough
                let p1 = db.pin_position(connection.a);
                let p2 = db.pin_position(connection.b);
                if (p1 - p2).length() <= SNAP_THRESHOLD {
                    connections_to_keep.insert(*connection);
                }
            }
        }

        // Snap pins and add new connections
        for connection in &new_connections {
            // Determine which pin should move (prefer wires and newly created instances)
            let (moving_pin, target_pin) = if self.dirty_instances.contains(&connection.a.ins)
                && !self.dirty_instances.contains(&connection.b.ins)
            {
                (connection.a, connection.b)
            } else if self.dirty_instances.contains(&connection.b.ins)
                && !self.dirty_instances.contains(&connection.a.ins)
            {
                (connection.b, connection.a)
            } else {
                // Both dirty or both clean, prefer wires to move
                match (db.ty(connection.a.ins), db.ty(connection.b.ins)) {
                    (InstanceKind::Wire, _) => (connection.a, connection.b),
                    (_, InstanceKind::Wire) => (connection.b, connection.a),
                    _ => (connection.a, connection.b), // Default to first
                }
            };

            self.snap_pin_to_other(db, moving_pin, target_pin);
            connections_to_keep.insert(*connection);
        }

        // Update the database connections
        db.connections = connections_to_keep;

        // Clear dirty sets
        self.dirty_instances.clear();
        self.dirty_pins.clear();

        true // Connections were updated
    }

    /// Get debug information about the connection manager
    pub fn debug_info(&self) -> String {
        format!(
            "ConnectionManager: {} dirty instances, {} dirty pins, {} grid cells",
            self.dirty_instances.len(),
            self.dirty_pins.len(),
            self.spatial_index.len()
        )
    }
}
