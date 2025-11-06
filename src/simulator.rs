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
        match circuit.ty(id) {
            InstanceKind::Wire => {
                self.evaluate_wire(circuit, id);
            }
            InstanceKind::Gate(_) => {
                self.evaluate_gate(circuit, id);
            }
            InstanceKind::Lamp => {
                self.evaluate_lamp(circuit, id);
            }
            InstanceKind::Power => {}
            InstanceKind::Clock => {
                if self.clocks_on {
                    self.current.insert(clock_output(id), Value::One);
                } else {
                    self.current.insert(clock_output(id), Value::Zero);
                }
            }
            InstanceKind::Module(def_id) => {}
        }
    }

    fn evaluate_power(&mut self, circuit: &Circuit, id: InstanceId) {
        let p = circuit.get_power(id);
        let out = power_output(id);
        let val = if p.on { Value::One } else { Value::Zero };
        self.current.insert(out, val);
    }

    fn evaluate_wire(&mut self, circuit: &Circuit, id: InstanceId) {
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

        let result = self.get_pin_value(circuit, input);

        self.current.insert(input, result);
        self.current.insert(other, result);
    }

    fn evaluate_gate(&mut self, circuit: &Circuit, id: InstanceId) {
        let InstanceKind::Gate(kind) = circuit.ty(id) else {
            return;
        };

        log::debug!("evaluating AND {id}");

        let inp1 = gate_inp1(id);
        let inp2 = gate_inp2(id);
        let out = gate_output(id);

        let a = self.get_pin_value(circuit, inp1);
        let b = self.get_pin_value(circuit, inp2);

        log::debug!("inputs {a:?} {b:?}");

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

    fn evaluate_lamp(&mut self, circuit: &Circuit, id: InstanceId) {
        let inp = lamp_input(id);
        let val = self.get_pin_value(circuit, inp);
        self.current.insert(inp, val);
    }

    fn get_pin_value(&self, circuit: &Circuit, pin: Pin) -> Value {
        let mut connected = circuit.connected_pins(pin);
        connected.push(pin);
        connected.sort_unstable();
        connected.dedup();

        if let Some(v) = self.assumed_pins.get(&pin) {
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::connection_manager::Connection;
//     use crate::db::{Clock, DB, Gate, GateKind, Lamp, Power};
//     use egui::{Pos2, pos2};
//
//     fn create_test_db() -> DB {
//         DB::default()
//     }
//
//     fn new_lamp(db: &mut DB) -> InstanceId {
//         db.circuit.new_lamp(Lamp { pos: Pos2::ZERO })
//     }
//
//     fn new_power(db: &mut DB) -> InstanceId {
//         db.circuit.new_power(Power {
//             pos: Pos2::ZERO,
//             on: true,
//         })
//     }
//
//     fn new_power_off(db: &mut DB) -> InstanceId {
//         db.circuit.new_power(Power {
//             pos: Pos2::ZERO,
//             on: false,
//         })
//     }
//
//     fn new_gate(db: &mut DB, kind: GateKind) -> InstanceId {
//         db.circuit.new_gate(Gate {
//             pos: Pos2::ZERO,
//             kind,
//         })
//     }
//
//     #[expect(dead_code)]
//     fn new_clock(db: &mut DB) -> InstanceId {
//         db.circuit.new_clock(Clock { pos: Pos2::ZERO })
//     }
//
//     fn add_connection(db: &mut DB, pin_a: Pin, pin_b: Pin) {
//         db.circuit.connections.insert(Connection::new(pin_a, pin_b));
//     }
//
//     // SR Latch Test Helpers
//     // Creates an SR latch using two NOR gates with cross-coupled feedback
//     // Standard SR NOR latch:
//     //   Q = NOR(R, Q̄)
//     //   Q̄ = NOR(S, Q)
//     //
//     // R (Reset)--[NOR1]--- Q
//     //            ^  |
//     //            |  +------+
//     //            |         |
//     //            +---------v
//     // S (Set) ---[NOR2]--- Q̄
//     fn create_sr_latch(db: &mut DB) -> (InstanceId, InstanceId, InstanceId, InstanceId) {
//         let s_power = db.circuit.new_power(Power {
//             pos: Pos2::ZERO,
//             on: false,
//         });
//         let r_power = db.circuit.new_power(Power {
//             pos: pos2(0.0, 50.0),
//             on: false,
//         });
//
//         let nor1 = new_gate(db, GateKind::Nor);
//         let nor2 = new_gate(db, GateKind::Nor);
//
//         // R connects to NOR1 input 1 (Q gate)
//         let r_out = power_output(r_power);
//         let nor1_in1 = gate_inp1(nor1);
//         add_connection(db, r_out, nor1_in1);
//
//         // S connects to NOR2 input 1 (Q̄ gate)
//         let s_out = power_output(s_power);
//         let nor2_in1 = gate_inp1(nor2);
//         add_connection(db, s_out, nor2_in1);
//
//         // Cross-couple: NOR1 output (Q) -> NOR2 input 2
//         let nor1_out = gate_output(nor1);
//         let nor2_in2 = gate_inp2(nor2);
//         add_connection(db, nor1_out, nor2_in2);
//
//         // Cross-couple: NOR2 output (Q̄) -> NOR1 input 2
//         let nor2_out = gate_output(nor2);
//         let nor1_in2 = gate_inp2(nor1);
//         add_connection(db, nor2_out, nor1_in2);
//
//         (s_power, r_power, nor1, nor2)
//     }
//
//     fn get_output(db: &DB, sim: &Simulator, id: InstanceId) -> Value {
//         sim.current[&gate_output(id)]
//     }
//
//     #[test]
//     fn test_power_to_lamp() {
//         let mut db = create_test_db();
//         let power = new_power(&mut db);
//         let lamp = new_lamp(&mut db);
//
//         let power_out = power_output(power);
//         let lamp_in = lamp_input(lamp);
//
//         add_connection(&mut db, power_out, lamp_in);
//
//         let mut sim = Simulator::new();
//         let result = sim.compute(&db.circuit);
//
//         assert!(result.contains(&power_out), "Power output should be on");
//         assert!(result.contains(&lamp_in), "Lamp input should be on");
//     }
//
//     #[test]
//     fn test_power_off_to_lamp() {
//         let mut db = create_test_db();
//         let power = new_power_off(&mut db);
//         let lamp = new_lamp(&mut db);
//
//         let power_out = power_output(power);
//         let lamp_in = lamp_input(lamp);
//
//         add_connection(&mut db, power_out, lamp_in);
//
//         let mut sim = Simulator::new();
//         let result = sim.compute(&db.circuit);
//
//         assert!(!result.contains(&power_out), "Power output should be off");
//         assert!(!result.contains(&lamp_in), "Lamp input should be off");
//     }
//
//     #[test]
//     fn test_power_gate_lamp() {
//         let mut db = create_test_db();
//         let power1 = new_power(&mut db);
//         let power2 = new_power(&mut db);
//         let gate = new_gate(&mut db, GateKind::And);
//         let lamp = new_lamp(&mut db);
//
//         let power1_out = power_output(power1);
//         let power2_out = power_output(power2);
//         let gate_in1 = gate_inp1(gate);
//         let gate_in2 = gate_inp2(gate);
//         let gate_out = gate_output(gate);
//         let lamp_in = lamp_input(lamp);
//
//         add_connection(&mut db, power1_out, gate_in1);
//         add_connection(&mut db, power2_out, gate_in2);
//         add_connection(&mut db, gate_out, lamp_in);
//
//         let mut sim = Simulator::new();
//         let result = sim.compute(&db.circuit);
//
//         assert!(result.contains(&gate_out), "AND gate output should be on");
//         assert!(result.contains(&lamp_in), "Lamp should be on");
//     }
//
//     #[test]
//     fn sr_latch_set_state() {
//         let mut db = create_test_db();
//         let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);
//
//         // Set S=1, R=0
//         db.circuit.get_power_mut(s_power).on = true;
//
//         let mut sim = Simulator::new();
//         let _result = sim.compute(&db.circuit);
//
//         // Q should be high (1)
//         let q_output = gate_output(nor1);
//         let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
//         assert_eq!(q_value, Value::One, "Q should be 1 in SET state");
//
//         // Q̄ should be low (0)
//         let q_bar_output = gate_output(nor2);
//         let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);
//         assert_eq!(q_bar_value, Value::Zero, "Q̄ should be 0 in SET state");
//
//         // Should stabilize quickly
//         assert!(
//             sim.last_iterations < 20,
//             "SR latch should stabilize in < 20 iterations, took {}",
//             sim.last_iterations
//         );
//     }
//
//     #[test]
//     fn sr_latch_reset_state() {
//         let mut db = create_test_db();
//         let (_s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);
//
//         // Set S=0, R=1
//         db.circuit.get_power_mut(r_power).on = true;
//
//         let mut sim = Simulator::new();
//         let _result = sim.compute(&db.circuit);
//
//         // Q should be low (0)
//         let q_output = gate_output(nor1);
//         let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
//         assert_eq!(q_value, Value::Zero, "Q should be 0 in RESET state");
//
//         // Q̄ should be high (1)
//         let q_bar_output = gate_output(nor2);
//         let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);
//         assert_eq!(q_bar_value, Value::One, "Q̄ should be 1 in RESET state");
//
//         // Should stabilize quickly
//         assert!(
//             sim.last_iterations < 20,
//             "SR latch should stabilize in < 20 iterations, took {}",
//             sim.last_iterations
//         );
//     }
//
//     #[test]
//     fn sr_latch_hold_state() {
//         let mut db = create_test_db();
//         let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);
//
//         // First set the latch to SET state (S=1, R=0)
//         db.circuit.get_power_mut(s_power).on = true;
//         let mut sim = Simulator::new();
//         sim.compute(&db.circuit);
//
//         assert_eq!(get_output(&db, &sim, nor1), Value::One);
//         assert_eq!(get_output(&db, &sim, nor2), Value::Zero);
//
//         db.circuit.get_power_mut(s_power).on = false;
//         let _result = sim.compute(&db.circuit);
//
//         assert_eq!(
//             get_output(&db, &sim, nor1),
//             Value::One,
//             "Q should hold state"
//         );
//
//         assert_eq!(
//             get_output(&db, &sim, nor2),
//             Value::Zero,
//             "Q inverse should hold state"
//         );
//     }
//
//     #[test]
//     fn sr_latch_forbidden_state() {
//         let mut db = create_test_db();
//         let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);
//
//         // Set S=1, R=1 (forbidden state)
//         db.circuit.get_power_mut(s_power).on = true;
//         db.circuit.get_power_mut(r_power).on = true;
//
//         let mut sim = Simulator::new();
//         let _result = sim.compute(&db.circuit);
//
//         // Both outputs should be 0 (since NOR with any input 1 gives 0)
//         let q_output = gate_output(nor1);
//         let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
//
//         let q_bar_output = gate_output(nor2);
//         let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);
//
//         // In forbidden state, both Q and Q̄ are 0 (violates Q = !Q̄)
//         assert_eq!(q_value, Value::Zero, "Q should be 0 in forbidden state");
//         assert_eq!(q_bar_value, Value::Zero, "Q̄ should be 0 in forbidden state");
//     }
//
//     #[test]
//     fn sr_latch_state_transition_set_to_reset() {
//         let mut db = create_test_db();
//         let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);
//
//         // Start in SET state (S=1, R=0)
//         db.circuit.get_power_mut(s_power).on = true;
//         let mut sim = Simulator::new();
//         sim.compute(&db.circuit);
//
//         let q_output = gate_output(nor1);
//         let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
//         assert_eq!(q_value, Value::One, "Q should be 1 after SET");
//
//         // Transition to RESET state (S=0, R=1)
//         db.circuit.get_power_mut(s_power).on = false;
//         db.circuit.get_power_mut(r_power).on = true;
//         let mut sim2 = Simulator::new();
//         sim2.compute(&db.circuit);
//
//         let q_value_after = sim2.current.get(&q_output).copied().unwrap_or(Value::X);
//         let q_bar_output = gate_output(nor2);
//         let q_bar_value = sim2.current.get(&q_bar_output).copied().unwrap_or(Value::X);
//
//         assert_eq!(q_value_after, Value::Zero, "Q should be 0 after RESET");
//         assert_eq!(q_bar_value, Value::One, "Q̄ should be 1 after RESET");
//     }
//
//     #[test]
//     fn sr_latch_stabilization_iterations() {
//         let mut db = create_test_db();
//         let (s_power, _r_power, _nor1, _nor2) = create_sr_latch(&mut db);
//
//         // Set S=1, R=0
//         db.circuit.get_power_mut(s_power).on = true;
//
//         let mut sim = Simulator::new();
//         sim.compute(&db.circuit);
//
//         // Should stabilize in reasonable number of iterations
//         // For a simple SR latch, should be < 10 iterations
//         assert!(
//             sim.last_iterations < 10,
//             "SR latch should stabilize quickly, took {} iterations",
//             sim.last_iterations
//         );
//         assert!(sim.last_iterations > 0, "Should take at least 1 iteration");
//     }
//
//     // #[test]
//     // fn clock_toggles_over_two_ticks() {
//     //     let mut db = create_test_db();
//     //     let clock = new_clock(&mut db);
//     //     let lamp = new_lamp(&mut db);
//     //
//     //     let clock_out = db.clock_output(clock);
//     //     let lamp_in = db.lamp_input(lamp);
//     //
//     //     add_connection(&mut db, clock_out, lamp_in);
//     //
//     //     let mut sim = Simulator::new(db.clone());
//     //
//     //     // First compute: clock should be Zero (initial state)
//     //     sim.compute();
//     //     let first_value = sim.current.get(&clock_out).copied().unwrap_or(Value::X);
//     //     assert_eq!(first_value, Value::Zero, "Clock should start at Zero");
//     //     assert_eq!(sim.clock_tick, 0, "Clock tick should be 0 initially");
//     //
//     //     // Advance clocks and compute again
//     //     sim.advance_clocks();
//     //     sim.compute();
//     //     let second_value = sim.current.get(&clock_out).copied().unwrap_or(Value::X);
//     //     assert_eq!(second_value, Value::One, "Clock should toggle to One");
//     //     assert_eq!(sim.clock_tick, 1, "Clock tick should be 1 after advance");
//     //
//     //     // Advance clocks and compute again
//     //     sim.advance_clocks();
//     //     sim.compute();
//     //     let third_value = sim.current.get(&clock_out).copied().unwrap_or(Value::X);
//     //     assert_eq!(third_value, Value::Zero, "Clock should toggle back to Zero");
//     //     assert_eq!(
//     //         sim.clock_tick, 2,
//     //         "Clock tick should be 2 after second advance"
//     //     );
//     // }
//     //
//     // #[test]
//     // fn clock_drives_gate() {
//     //     let mut db = create_test_db();
//     //     let clock = new_clock(&mut db);
//     //     let power = new_power(&mut db);
//     //     let gate = new_gate(&mut db, GateKind::And);
//     //     let lamp = new_lamp(&mut db);
//     //
//     //     let clock_out = db.clock_output(clock);
//     //     let power_out = db.power_output(power);
//     //     let gate_in1 = db.gate_inp1(gate);
//     //     let gate_in2 = db.gate_inp2(gate);
//     //     let gate_out = db.gate_output(gate);
//     //     let lamp_in = db.lamp_input(lamp);
//     //
//     //     // Connect: clock -> gate_in1, power -> gate_in2, gate_out -> lamp
//     //     add_connection(&mut db, clock_out, gate_in1);
//     //     add_connection(&mut db, power_out, gate_in2);
//     //     add_connection(&mut db, gate_out, lamp_in);
//     //
//     //     let mut sim = Simulator::new(db.clone());
//     //
//     //     // First compute: clock is Zero, power is One, AND gate output should be Zero
//     //     sim.compute();
//     //     let lamp_val = sim.current.get(&lamp_in).copied().unwrap_or(Value::X);
//     //     assert_eq!(
//     //         lamp_val,
//     //         Value::Zero,
//     //         "Lamp should be off when clock is Zero"
//     //     );
//     //
//     //     // Advance clock and compute: clock is One, power is One, AND gate output should be One
//     //     sim.advance_clocks();
//     //     sim.compute();
//     //     let lamp_val = sim.current.get(&lamp_in).copied().unwrap_or(Value::X);
//     //     assert_eq!(lamp_val, Value::One, "Lamp should be on when clock is One");
//     //
//     //     // Advance clock again: clock is Zero, power is One, AND gate output should be Zero
//     //     sim.advance_clocks();
//     //     sim.compute();
//     //     let lamp_val = sim.current.get(&lamp_in).copied().unwrap_or(Value::X);
//     //     assert_eq!(
//     //         lamp_val,
//     //         Value::Zero,
//     //         "Lamp should be off when clock toggles back to Zero"
//     //     );
//     // }
//     //
//     // #[test]
//     // fn clock_state_resets_on_new_simulator() {
//     //     let mut db = create_test_db();
//     //     let clock = new_clock(&mut db);
//     //
//     //     let clock_out = db.clock_output(clock);
//     //
//     //     // Create first simulator and advance clock
//     //     let mut sim1 = Simulator::new(db.clone());
//     //     sim1.advance_clocks();
//     //     sim1.compute();
//     //     let first_value = sim1.current.get(&clock_out).copied().unwrap_or(Value::X);
//     //     assert_eq!(first_value, Value::One, "Clock should be One after advance");
//     //
//     //     // Create new simulator - clock should reset to Zero
//     //     let mut sim2 = Simulator::new(db.clone());
//     //     sim2.compute();
//     //     let second_value = sim2.current.get(&clock_out).copied().unwrap_or(Value::X);
//     //     assert_eq!(
//     //         second_value,
//     //         Value::Zero,
//     //         "Clock should reset to Zero in new simulator"
//     //     );
//     //     assert_eq!(
//     //         sim2.clock_tick, 0,
//     //         "Clock tick should reset to 0 in new simulator"
//     //     );
//     // }
// }
