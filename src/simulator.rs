use std::collections::{HashMap, HashSet};

use log;

use crate::{
    assets::PinKind,
    db::{Circuit, DB, GateKind, InstanceId, InstanceKind, Pin},
};

const MAX_ITERATIONS: usize = 1000;
const STABILIZATION_THRESHOLD: usize = 3;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Zero,
    One,
    X,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationStatus {
    Stable { iterations: usize },
    Unstable { max_reached: bool },
    Running,
}

impl Default for SimulationStatus {
    fn default() -> Self {
        Self::Running
    }
}

impl Value {
    fn is_one(self) -> bool {
        self == Self::One
    }

    fn not(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One => Self::Zero,
            Self::X => Self::X,
        }
    }

    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::One, Self::One) => Self::One,
            _ => Self::X,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::One, _) | (_, Self::One) => Self::One,
            (Self::Zero, Self::Zero) => Self::Zero,
            _ => Self::X,
        }
    }

    fn xor(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, v) | (v, Self::Zero) => v,
            (Self::One, Self::One) => Self::Zero,
            _ => Self::X,
        }
    }

    fn xnor(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, Self::Zero) | (Self::One, Self::One) => Self::One,
            (Self::Zero, Self::One) | (Self::One, Self::Zero) => Self::Zero,
            _ => Self::X,
        }
    }
}

#[derive(Default)]
pub struct Simulator {
    /// Final result - maps each pin to its current value
    pub current: HashMap<Pin, Value>,
    /// Assume the value for these pins always
    pub assumed_pins: HashMap<Pin, Value>,
    /// Keep what has been already evaluated
    pub evaluated: HashSet<InstanceId>,
    /// Number of iterations taken in last compute
    pub last_iterations: usize,
    /// Current status of the simulation
    pub status: SimulationStatus,
    /// Current iteration number
    pub current_iteration: usize,
    /// Are clocks on?
    pub clocks_on: bool,
}

impl Simulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn rebuild_sorted_instances(&self, circuit: &Circuit) -> Vec<InstanceId> {
        let mut ids: Vec<InstanceId> = circuit.types.keys().collect();
        ids.sort_unstable();
        ids
    }

    pub fn compute(&mut self, db: &DB, circuit: &Circuit) -> HashSet<Pin> {
        log::debug!("=== Begin simulation ===");

        self.current_iteration = 0;
        self.status = SimulationStatus::Running;

        let mut previous_state: HashMap<Pin, Value>;
        let mut stable_count = 0;

        let power_ids: Vec<_> = circuit.powers.keys().collect();
        for &id in &power_ids {
            self.evaluate_power(circuit, id);
        }

        let sorted_instances = self.rebuild_sorted_instances(circuit);
        while self.current_iteration < MAX_ITERATIONS {
            previous_state = self.current.clone();

            for &id in &sorted_instances {
                self.evaluate(db, circuit, id);
            }

            self.current_iteration += 1;

            if self.current == previous_state {
                stable_count += 1;
                if stable_count >= STABILIZATION_THRESHOLD {
                    self.last_iterations = self.current_iteration;
                    self.status = SimulationStatus::Stable {
                        iterations: self.current_iteration,
                    };
                    log::debug!(
                        "Simulation stabilized after {} iterations",
                        self.current_iteration
                    );
                    break;
                }
            } else {
                stable_count = 0;
            }
        }

        if self.current_iteration >= MAX_ITERATIONS {
            self.last_iterations = MAX_ITERATIONS;
            self.status = SimulationStatus::Unstable { max_reached: true };
            log::warn!("Simulation reached max iterations without stabilizing");
        }

        self.current
            .iter()
            .filter_map(|(pin, val)| if val.is_one() { Some(*pin) } else { None })
            .collect()
    }

    fn evaluate(&mut self, db: &DB, circuit: &Circuit, id: InstanceId) {
        self.evaluated.insert(id);

        // Debug: log evaluation of hidden instances
        if db.is_hidden(id) {
            log::debug!(
                "Evaluating hidden instance: {:?}, type: {:?}",
                id,
                circuit.ty(id)
            );
        }

        match circuit.ty(id) {
            InstanceKind::Wire => {
                self.evaluate_wire(db, circuit, id);
            }
            InstanceKind::Gate(_) => {
                self.evaluate_gate(db, circuit, id);
            }
            InstanceKind::Lamp => {
                self.evaluate_lamp(db, circuit, id);
            }
            InstanceKind::Power => {}
            InstanceKind::Clock => {
                if self.clocks_on {
                    self.current.insert(clock_output(id), Value::One);
                } else {
                    self.current.insert(clock_output(id), Value::Zero);
                }
            }
            InstanceKind::Module(_) => {
                // Module evaluation: propagate values between external and internal pins
                // External pins are connected to the outside world
                // Internal pins are connected to the flattened components
                if let Some(pin_mapping) = circuit.module_pin_mappings.get(id) {
                    for (external_pin, internal_pin) in pin_mapping {
                        // Propagate values based on pin direction
                        match external_pin.kind {
                            crate::assets::PinKind::Input => {
                                // Input: external -> internal
                                let external_value = self.get_pin_value(db, circuit, *external_pin);
                                self.current.insert(*internal_pin, external_value);
                            }
                            crate::assets::PinKind::Output => {
                                // Output: internal -> external
                                // Read directly from self.current to avoid circular redirect
                                let internal_value = self
                                    .current
                                    .get(internal_pin)
                                    .copied()
                                    .unwrap_or(Value::Zero);
                                self.current.insert(*external_pin, internal_value);
                            }
                        }
                    }
                }
            }
        }
    }

    fn evaluate_power(&mut self, circuit: &Circuit, id: InstanceId) {
        let p = circuit.get_power(id);
        let out = power_output(id);
        let val = if p.on { Value::One } else { Value::Zero };
        self.current.insert(out, val);
    }

    fn evaluate_wire(&mut self, db: &DB, circuit: &Circuit, id: InstanceId) {
        let input = {
            let start = wire_start(id);
            let end = wire_end(id);

            if start.kind == PinKind::Input {
                start
            } else {
                end
            }
        };
        let other = if wire_start(id) == input {
            wire_end(id)
        } else {
            wire_start(id)
        };

        let result = self.get_pin_value(db, circuit, input);

        self.current.insert(input, result);
        self.current.insert(other, result);
    }

    fn evaluate_gate(&mut self, db: &DB, circuit: &Circuit, id: InstanceId) {
        let InstanceKind::Gate(kind) = circuit.ty(id) else {
            return;
        };

        let inp1 = gate_inp1(id);
        let inp2 = gate_inp2(id);
        let out = gate_output(id);

        let a = self.get_pin_value(db, circuit, inp1);
        let b = self.get_pin_value(db, circuit, inp2);

        let out_val = match kind {
            GateKind::And => a.and(b),
            GateKind::Nand => a.and(b).not(),
            GateKind::Or => a.or(b),
            GateKind::Nor => a.or(b).not(),
            GateKind::Xor => a.xor(b),
            GateKind::Xnor => a.xnor(b),
        };

        self.current.insert(out, out_val);
    }

    fn evaluate_lamp(&mut self, db: &DB, circuit: &Circuit, id: InstanceId) {
        let inp = lamp_input(id);
        let val = self.get_pin_value(db, circuit, inp);
        self.current.insert(inp, val);
    }

    fn get_pin_value(&self, db: &DB, circuit: &Circuit, pin: Pin) -> Value {
        // Check if this is an internal hidden pin that maps to a module boundary
        let lookup_pin = if db.is_hidden(pin.ins) {
            // Find if there's a module that has this as an internal pin
            // Look through all module_pin_mappings to find reverse mapping
            circuit
                .module_pin_mappings
                .iter()
                .find_map(|(_module_id, mappings)| {
                    mappings
                        .iter()
                        .find_map(|(ext, int)| if int == &pin { Some(*ext) } else { None })
                })
                .unwrap_or(pin) // If not found, use original pin
        } else {
            pin
        };

        let mut connected = circuit.connected_pins(lookup_pin);
        connected.push(lookup_pin);
        connected.sort_unstable();
        connected.dedup();

        if let Some(v) = self.assumed_pins.get(&lookup_pin) {
            return *v;
        }

        let mut result = Value::Zero;
        for other in connected {
            if other.kind != PinKind::Output {
                continue;
            }

            if let Some(&val) = self.current.get(&other) {
                result = result.or(val);
            }
        }

        result
    }
}

pub fn gate_inp_n(id: InstanceId, n: u32) -> Pin {
    assert!(n < 2, "Gates only have 2 inputs (0 and 1)");
    Pin::new(id, if n == 0 { 0 } else { 2 }, PinKind::Input)
}

pub fn gate_output_n(id: InstanceId, n: u32) -> Pin {
    assert!(n == 0, "Gates only have 1 output");
    Pin::new(id, 1, PinKind::Output)
}

pub fn gate_inp1(id: InstanceId) -> Pin {
    gate_inp_n(id, 0)
}

pub fn gate_inp2(id: InstanceId) -> Pin {
    gate_inp_n(id, 1)
}

pub fn gate_output(id: InstanceId) -> Pin {
    gate_output_n(id, 0)
}

pub fn wire_pin_n(id: InstanceId, n: u32) -> Pin {
    assert!(n < 2, "Wires only have 2 pins (0 and 1)");
    Pin::new(
        id,
        n,
        if n == 0 {
            PinKind::Input
        } else {
            PinKind::Output
        },
    )
}

pub fn wire_start(id: InstanceId) -> Pin {
    wire_pin_n(id, 0)
}

pub fn wire_end(id: InstanceId) -> Pin {
    wire_pin_n(id, 1)
}

pub fn power_output(id: InstanceId) -> Pin {
    Pin::new(id, 0, PinKind::Output)
}

pub fn lamp_input(id: InstanceId) -> Pin {
    Pin::new(id, 0, PinKind::Input)
}

pub fn clock_output(id: InstanceId) -> Pin {
    Pin::new(id, 0, PinKind::Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_manager::Connection;
    use crate::db::{DB, Gate, GateKind, Lamp, Power};
    use crate::module::{Module, ModuleDefinition};
    use egui::Pos2;

    // Helper functions
    fn new_lamp(db: &mut DB) -> InstanceId {
        db.circuit.new_lamp(Lamp { pos: Pos2::ZERO })
    }

    fn new_power(db: &mut DB) -> InstanceId {
        db.circuit.new_power(Power {
            pos: Pos2::ZERO,
            on: true,
        })
    }

    fn new_power_off(db: &mut DB) -> InstanceId {
        db.circuit.new_power(Power {
            pos: Pos2::ZERO,
            on: false,
        })
    }

    fn new_gate(db: &mut DB, kind: GateKind) -> InstanceId {
        db.circuit.new_gate(Gate {
            pos: Pos2::ZERO,
            kind,
        })
    }

    fn add_connection(db: &mut DB, pin_a: Pin, pin_b: Pin) {
        db.circuit.connections.insert(Connection::new(pin_a, pin_b));
    }

    // SR Latch Test Helpers
    // Creates an SR latch using two NOR gates with cross-coupled feedback
    // Standard SR NOR latch:
    //   Q = NOR(R, Q̄)
    //   Q̄ = NOR(S, Q)
    //
    // R (Reset)--[NOR1]--- Q
    //            ^  |
    //            |  +------+
    //            |         |
    //            +---------v
    // S (Set) ---[NOR2]--- Q̄
    fn create_sr_latch(db: &mut DB) -> (InstanceId, InstanceId, InstanceId, InstanceId) {
        let s_power = db.circuit.new_power(Power {
            pos: Pos2::ZERO,
            on: false,
        });
        let r_power = db.circuit.new_power(Power {
            pos: Pos2::new(0.0, 50.0),
            on: false,
        });

        let nor1 = new_gate(db, GateKind::Nor);
        let nor2 = new_gate(db, GateKind::Nor);

        // R connects to NOR1 input 1 (Q gate)
        let r_out = power_output(r_power);
        let nor1_in1 = gate_inp1(nor1);
        add_connection(db, r_out, nor1_in1);

        // S connects to NOR2 input 1 (Q̄ gate)
        let s_out = power_output(s_power);
        let nor2_in1 = gate_inp1(nor2);
        add_connection(db, s_out, nor2_in1);

        // Cross-couple: NOR1 output (Q) -> NOR2 input 2
        let nor1_out = gate_output(nor1);
        let nor2_in2 = gate_inp2(nor2);
        add_connection(db, nor1_out, nor2_in2);

        // Cross-couple: NOR2 output (Q̄) -> NOR1 input 2
        let nor2_out = gate_output(nor2);
        let nor1_in2 = gate_inp2(nor1);
        add_connection(db, nor2_out, nor1_in2);

        (s_power, r_power, nor1, nor2)
    }

    fn get_output(sim: &Simulator, id: InstanceId) -> Value {
        sim.current[&gate_output(id)]
    }

    #[test]
    fn test_module_simulation_simple() {
        // Create a module definition with a single AND gate
        let mut def_circuit = crate::db::Circuit::default();
        let gate = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let gate_id = def_circuit.new_gate(gate);

        let module_def = ModuleDefinition {
            name: "SimpleAND".to_owned(),
            circuit: def_circuit,
        };

        // Create DB and add definition
        let mut db = DB::default();
        let def_id = db.module_definitions.insert(module_def);

        // Create a module instance and flatten it
        let module = Module {
            pos: Pos2::ZERO,
            definition_index: def_id,
        };
        let module_id = db.new_module_with_flattening(module);
        eprintln!("Module ID: {module_id:?}");

        // Check internal instances
        let internal_instances = db.get_instances_for_module(module_id);
        eprintln!("Internal instances for module: {internal_instances:?}");
        for &hid in &internal_instances {
            let ty = db.circuit.ty(hid);
            eprintln!("  Hidden instance {hid:?} is type {ty:?}");
        }

        // Create two power sources and connect them to the module inputs
        let power1 = Power {
            pos: Pos2::ZERO,
            on: true,
        };
        let power1_id = db.circuit.new_power(power1);

        let power2 = Power {
            pos: Pos2::ZERO,
            on: true,
        };
        let power2_id = db.circuit.new_power(power2);

        // Create a lamp and connect it to the module output
        let lamp = Lamp { pos: Pos2::ZERO };
        let lamp_id = db.circuit.new_lamp(lamp);

        // Get module pins (should be 3: input, input, output for AND gate)
        let module_pins = db.circuit.pins_of(module_id, &db);
        assert_eq!(module_pins.len(), 3, "Module should have 3 pins");

        // Connect power1 to module input 0
        let module_in0 = module_pins
            .iter()
            .find(|p| p.index == 0 && p.kind == PinKind::Input)
            .expect("module should have input pin 0");
        db.circuit
            .connections
            .insert(Connection::new(power_output(power1_id), *module_in0));

        // Connect power2 to module input 2 (second input of AND gate)
        let module_in1 = module_pins
            .iter()
            .find(|p| p.index == 2 && p.kind == PinKind::Input)
            .expect("module should have input pin 2");
        db.circuit
            .connections
            .insert(Connection::new(power_output(power2_id), *module_in1));

        // Connect module output to lamp
        let module_out = module_pins
            .iter()
            .find(|p| p.kind == PinKind::Output)
            .expect("module should have output pin");
        db.circuit
            .connections
            .insert(Connection::new(*module_out, lamp_input(lamp_id)));

        // Debug: print all connections
        eprintln!("\n=== Connections ===");
        for conn in &db.circuit.connections {
            let a = conn.a;
            let b = conn.b;
            eprintln!("Connection: {a:?} <-> {b:?}");
        }

        // Run simulation
        let mut sim = Simulator::new();
        sim.compute(&db, &db.circuit);

        // Debug: print all pin values
        eprintln!("=== Simulation Results ===");
        for (pin, value) in &sim.current {
            eprintln!("Pin {pin:?}: {value:?}");
        }

        // Debug: print module pin mappings
        eprintln!("\n=== Module Pin Mappings ===");
        if let Some(mappings) = db.circuit.module_pin_mappings.get(module_id) {
            for (ext, int) in mappings {
                eprintln!("External {ext:?} -> Internal {int:?}");
            }
        }

        // Check that the lamp is on (both inputs are 1, so AND gate outputs 1)
        let lamp_value = sim
            .current
            .get(&lamp_input(lamp_id))
            .copied()
            .unwrap_or(Value::X);
        eprintln!("\nLamp value: {lamp_value:?}");
        assert_eq!(
            lamp_value,
            Value::One,
            "Lamp should be on when both module inputs are high"
        );

        // Check that the simulation stabilized
        assert!(
            matches!(sim.status, SimulationStatus::Stable { .. }),
            "Simulation should stabilize"
        );
    }

    #[test]
    fn test_power_to_lamp() {
        let mut db = DB::default();
        let power = new_power(&mut db);
        let lamp = new_lamp(&mut db);

        let power_out = power_output(power);
        let lamp_in = lamp_input(lamp);

        add_connection(&mut db, power_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db, &db.circuit);

        assert!(result.contains(&power_out), "Power output should be on");
        assert!(result.contains(&lamp_in), "Lamp input should be on");
    }

    #[test]
    fn test_power_off_to_lamp() {
        let mut db = DB::default();
        let power = new_power_off(&mut db);
        let lamp = new_lamp(&mut db);

        let power_out = power_output(power);
        let lamp_in = lamp_input(lamp);

        add_connection(&mut db, power_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db, &db.circuit);

        assert!(!result.contains(&power_out), "Power output should be off");
        assert!(!result.contains(&lamp_in), "Lamp input should be off");
    }

    #[test]
    fn test_power_gate_lamp() {
        let mut db = DB::default();
        let power1 = new_power(&mut db);
        let power2 = new_power(&mut db);
        let gate = new_gate(&mut db, GateKind::And);
        let lamp = new_lamp(&mut db);

        let power1_out = power_output(power1);
        let power2_out = power_output(power2);
        let gate_in1 = gate_inp1(gate);
        let gate_in2 = gate_inp2(gate);
        let gate_out = gate_output(gate);
        let lamp_in = lamp_input(lamp);

        add_connection(&mut db, power1_out, gate_in1);
        add_connection(&mut db, power2_out, gate_in2);
        add_connection(&mut db, gate_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db, &db.circuit);

        assert!(result.contains(&gate_out), "AND gate output should be on");
        assert!(result.contains(&lamp_in), "Lamp should be on");
    }

    #[test]
    fn sr_latch_set_state() {
        let mut db = DB::default();
        let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=1, R=0
        db.circuit.get_power_mut(s_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db, &db.circuit);

        // Q should be high (1)
        let q_output = gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::One, "Q should be 1 in SET state");

        // Q̄ should be low (0)
        let q_bar_output = gate_output(nor2);
        let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);
        assert_eq!(q_bar_value, Value::Zero, "Q̄ should be 0 in SET state");

        // Should stabilize quickly
        assert!(
            sim.last_iterations < 20,
            "SR latch should stabilize in < 20 iterations, took {}",
            sim.last_iterations
        );
    }

    #[test]
    fn sr_latch_reset_state() {
        let mut db = DB::default();
        let (_s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=0, R=1
        db.circuit.get_power_mut(r_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db, &db.circuit);

        // Q should be low (0)
        let q_output = gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::Zero, "Q should be 0 in RESET state");

        // Q̄ should be high (1)
        let q_bar_output = gate_output(nor2);
        let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);
        assert_eq!(q_bar_value, Value::One, "Q̄ should be 1 in RESET state");

        // Should stabilize quickly
        assert!(
            sim.last_iterations < 20,
            "SR latch should stabilize in < 20 iterations, took {}",
            sim.last_iterations
        );
    }

    #[test]
    fn sr_latch_hold_state() {
        let mut db = DB::default();
        let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);

        // First set the latch to SET state (S=1, R=0)
        db.circuit.get_power_mut(s_power).on = true;
        let mut sim = Simulator::new();
        sim.compute(&db, &db.circuit);

        assert_eq!(get_output(&sim, nor1), Value::One);
        assert_eq!(get_output(&sim, nor2), Value::Zero);

        db.circuit.get_power_mut(s_power).on = false;
        let _result = sim.compute(&db, &db.circuit);

        assert_eq!(get_output(&sim, nor1), Value::One, "Q should hold state");

        assert_eq!(
            get_output(&sim, nor2),
            Value::Zero,
            "Q inverse should hold state"
        );
    }

    #[test]
    fn sr_latch_forbidden_state() {
        let mut db = DB::default();
        let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=1, R=1 (forbidden state)
        db.circuit.get_power_mut(s_power).on = true;
        db.circuit.get_power_mut(r_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db, &db.circuit);

        // Both outputs should be 0 (since NOR with any input 1 gives 0)
        let q_output = gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);

        let q_bar_output = gate_output(nor2);
        let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);

        // In forbidden state, both Q and Q̄ are 0 (violates Q = !Q̄)
        assert_eq!(q_value, Value::Zero, "Q should be 0 in forbidden state");
        assert_eq!(q_bar_value, Value::Zero, "Q̄ should be 0 in forbidden state");
    }

    #[test]
    fn sr_latch_state_transition_set_to_reset() {
        let mut db = DB::default();
        let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Start in SET state (S=1, R=0)
        db.circuit.get_power_mut(s_power).on = true;
        let mut sim = Simulator::new();
        sim.compute(&db, &db.circuit);

        let q_output = gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::One, "Q should be 1 after SET");

        // Transition to RESET state (S=0, R=1)
        db.circuit.get_power_mut(s_power).on = false;
        db.circuit.get_power_mut(r_power).on = true;
        let mut sim2 = Simulator::new();
        sim2.compute(&db, &db.circuit);

        let q_value_after = sim2.current.get(&q_output).copied().unwrap_or(Value::X);
        let q_bar_output = gate_output(nor2);
        let q_bar_value = sim2.current.get(&q_bar_output).copied().unwrap_or(Value::X);

        assert_eq!(q_value_after, Value::Zero, "Q should be 0 after RESET");
        assert_eq!(q_bar_value, Value::One, "Q̄ should be 1 after RESET");
    }

    #[test]
    fn sr_latch_stabilization_iterations() {
        let mut db = DB::default();
        let (s_power, _r_power, _nor1, _nor2) = create_sr_latch(&mut db);

        // Set S=1, R=0
        db.circuit.get_power_mut(s_power).on = true;

        let mut sim = Simulator::new();
        sim.compute(&db, &db.circuit);

        // Should stabilize in reasonable number of iterations
        // For a simple SR latch, should be < 10 iterations
        assert!(
            sim.last_iterations < 10,
            "SR latch should stabilize quickly, took {} iterations",
            sim.last_iterations
        );
        assert!(sim.last_iterations > 0, "Should take at least 1 iteration");
    }
}
