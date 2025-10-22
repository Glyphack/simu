use std::collections::{HashMap, HashSet};

use log;

use crate::{
    app::{DB, GateKind, InstanceId, InstanceKind, Pin},
    assets::PinKind,
};

const MAX_ITERATIONS: usize = 1000;
const STABILIZATION_THRESHOLD: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Number of iterations taken in last compute
    pub last_iterations: usize,
    /// Current status of the simulation
    pub status: SimulationStatus,
    /// Current iteration number (for debugging/UI)
    pub current_iteration: usize,
}

impl Simulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn rebuild_sorted_instances(&self, db: &DB) -> Vec<InstanceId> {
        let mut ids: Vec<InstanceId> = db.types.keys().collect();
        ids.sort_unstable();
        ids
    }

    pub fn compute(&mut self, db: &DB) -> HashSet<Pin> {
        log::info!("=== Begin simulation ===");

        self.current_iteration = 0;
        self.status = SimulationStatus::Running;
        self.current.clear();

        let mut previous_state: HashMap<Pin, Value>;
        let mut stable_count = 0;

        let power_ids: Vec<_> = db.powers.keys().collect();
        for &id in &power_ids {
            self.evaluate_power(db, id);
        }

        while self.current_iteration < MAX_ITERATIONS {
            previous_state = self.current.clone();

            let sorted_instances = self.rebuild_sorted_instances(db);
            for &id in &sorted_instances {
                match db.ty(id) {
                    InstanceKind::Wire => {
                        self.evaluate_wire(db, id);
                    }
                    InstanceKind::Gate(_) => {
                        self.evaluate_gate(db, id);
                    }
                    InstanceKind::Lamp => {
                        self.evaluate_lamp(db, id);
                    }
                    InstanceKind::Power | InstanceKind::CustomCircuit(_) => {}
                }
            }

            self.current_iteration += 1;

            if self.current == previous_state {
                stable_count += 1;
                if stable_count >= STABILIZATION_THRESHOLD {
                    self.last_iterations = self.current_iteration;
                    self.status = SimulationStatus::Stable {
                        iterations: self.current_iteration,
                    };
                    log::info!(
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

    fn evaluate_power(&mut self, db: &DB, id: InstanceId) {
        let p = db.get_power(id);
        let out = db.power_output(id);
        let val = if p.on { Value::One } else { Value::Zero };
        self.current.insert(out, val);
    }

    fn evaluate_wire(&mut self, db: &DB, id: InstanceId) {
        let input = {
            let start = db.wire_start(id);
            let end = db.wire_end(id);

            if db.pin_info(start).kind == PinKind::Input {
                start
            } else {
                end
            }
        };
        let other = if db.wire_start(id) == input {
            db.wire_end(id)
        } else {
            db.wire_start(id)
        };

        let result = self.get_pin_value(db, input);

        self.current.insert(input, result);
        self.current.insert(other, result);
    }

    fn evaluate_gate(&mut self, db: &DB, id: InstanceId) {
        let InstanceKind::Gate(kind) = db.ty(id) else {
            return;
        };

        let inp1 = db.gate_inp1(id);
        let inp2 = db.gate_inp2(id);
        let out = db.gate_output(id);

        let a = self.get_pin_value(db, inp1);
        let b = self.get_pin_value(db, inp2);

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

    fn evaluate_lamp(&mut self, db: &DB, id: InstanceId) {
        let inp = db.lamp_input(id);
        let val = self.get_pin_value(db, inp);
        self.current.insert(inp, val);
    }

    fn get_pin_value(&self, db: &DB, pin: Pin) -> Value {
        let mut connected = db.connected_pins(pin);
        connected.push(pin);
        connected.sort_unstable();
        connected.dedup();

        let mut result = Value::Zero;
        for other in connected {
            if db.pin_info(other).kind != PinKind::Output {
                continue;
            }
            if let Some(&val) = self.current.get(&other) {
                result = result.or(val);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Connection, Gate, GateKind, Lamp, Power};
    use egui::{Pos2, pos2};

    fn create_test_db() -> DB {
        DB::new()
    }

    fn new_lamp(db: &mut DB) -> InstanceId {
        db.new_lamp(Lamp { pos: Pos2::ZERO })
    }

    fn new_power(db: &mut DB) -> InstanceId {
        db.new_power(Power {
            pos: Pos2::ZERO,
            on: true,
        })
    }

    fn new_power_off(db: &mut DB) -> InstanceId {
        db.new_power(Power {
            pos: Pos2::ZERO,
            on: false,
        })
    }

    fn new_gate(db: &mut DB, kind: GateKind) -> InstanceId {
        db.new_gate(Gate {
            pos: Pos2::ZERO,
            kind,
        })
    }

    fn add_connection(db: &mut DB, pin_a: Pin, pin_b: Pin) {
        db.connections.insert(Connection::new(pin_a, pin_b));
    }

    #[test]
    fn test_power_to_lamp() {
        let mut db = create_test_db();
        let power = new_power(&mut db);
        let lamp = new_lamp(&mut db);

        let power_out = db.power_output(power);
        let lamp_in = db.lamp_input(lamp);

        add_connection(&mut db, power_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db);

        assert!(result.contains(&power_out), "Power output should be on");
        assert!(result.contains(&lamp_in), "Lamp input should be on");
    }

    #[test]
    fn test_power_off_to_lamp() {
        let mut db = create_test_db();
        let power = new_power_off(&mut db);
        let lamp = new_lamp(&mut db);

        let power_out = db.power_output(power);
        let lamp_in = db.lamp_input(lamp);

        add_connection(&mut db, power_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db);

        assert!(!result.contains(&power_out), "Power output should be off");
        assert!(!result.contains(&lamp_in), "Lamp input should be off");
    }

    #[test]
    fn test_power_gate_lamp() {
        let mut db = create_test_db();
        let power1 = new_power(&mut db);
        let power2 = new_power(&mut db);
        let gate = new_gate(&mut db, GateKind::And);
        let lamp = new_lamp(&mut db);

        let power1_out = db.power_output(power1);
        let power2_out = db.power_output(power2);
        let gate_in1 = db.gate_inp1(gate);
        let gate_in2 = db.gate_inp2(gate);
        let gate_out = db.gate_output(gate);
        let lamp_in = db.lamp_input(lamp);

        add_connection(&mut db, power1_out, gate_in1);
        add_connection(&mut db, power2_out, gate_in2);
        add_connection(&mut db, gate_out, lamp_in);

        let mut sim = Simulator::new();
        let result = sim.compute(&db);

        assert!(result.contains(&gate_out), "AND gate output should be on");
        assert!(result.contains(&lamp_in), "Lamp should be on");
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
        let s_power = db.new_power(Power {
            pos: Pos2::ZERO,
            on: false,
        });
        let r_power = db.new_power(Power {
            pos: pos2(0.0, 50.0),
            on: false,
        });

        let nor1 = new_gate(db, GateKind::Nor); // Q gate: Q = NOR(R, Q̄)
        let nor2 = new_gate(db, GateKind::Nor); // Q̄ gate: Q̄ = NOR(S, Q)

        // R connects to NOR1 input 1 (Q gate)
        let r_out = db.power_output(r_power);
        let nor1_in1 = db.gate_inp1(nor1);
        add_connection(db, r_out, nor1_in1);

        // S connects to NOR2 input 1 (Q̄ gate)
        let s_out = db.power_output(s_power);
        let nor2_in1 = db.gate_inp1(nor2);
        add_connection(db, s_out, nor2_in1);

        // Cross-couple: NOR1 output (Q) -> NOR2 input 2
        let nor1_out = db.gate_output(nor1);
        let nor2_in2 = db.gate_inp2(nor2);
        add_connection(db, nor1_out, nor2_in2);

        // Cross-couple: NOR2 output (Q̄) -> NOR1 input 2
        let nor2_out = db.gate_output(nor2);
        let nor1_in2 = db.gate_inp2(nor1);
        add_connection(db, nor2_out, nor1_in2);

        (s_power, r_power, nor1, nor2)
    }

    #[test]
    fn sr_latch_set_state() {
        let mut db = create_test_db();
        let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=1, R=0
        db.get_power_mut(s_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db);

        // Q should be high (1)
        let q_output = db.gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::One, "Q should be 1 in SET state");

        // Q̄ should be low (0)
        let q_bar_output = db.gate_output(nor2);
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
        let mut db = create_test_db();
        let (_s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=0, R=1
        db.get_power_mut(r_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db);

        // Q should be low (0)
        let q_output = db.gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::Zero, "Q should be 0 in RESET state");

        // Q̄ should be high (1)
        let q_bar_output = db.gate_output(nor2);
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
        let mut db = create_test_db();
        let (s_power, _r_power, nor1, nor2) = create_sr_latch(&mut db);

        // First set the latch to SET state (S=1, R=0)
        db.get_power_mut(s_power).on = true;
        let mut sim = Simulator::new();
        sim.compute(&db);

        // Now change to HOLD state (S=0, R=0)
        db.get_power_mut(s_power).on = false;
        let mut sim2 = Simulator::new();
        let _result = sim2.compute(&db);

        // Q should still be high (1) - holding previous SET state
        let q_output = db.gate_output(nor1);
        let q_value = sim2.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::One, "Q should remain 1 in HOLD state");

        // Q̄ should still be low (0)
        let q_bar_output = db.gate_output(nor2);
        let q_bar_value = sim2.current.get(&q_bar_output).copied().unwrap_or(Value::X);
        assert_eq!(q_bar_value, Value::Zero, "Q̄ should remain 0 in HOLD state");
    }

    #[test]
    fn sr_latch_forbidden_state() {
        let mut db = create_test_db();
        let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Set S=1, R=1 (forbidden state)
        db.get_power_mut(s_power).on = true;
        db.get_power_mut(r_power).on = true;

        let mut sim = Simulator::new();
        let _result = sim.compute(&db);

        // Both outputs should be 0 (since NOR with any input 1 gives 0)
        let q_output = db.gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);

        let q_bar_output = db.gate_output(nor2);
        let q_bar_value = sim.current.get(&q_bar_output).copied().unwrap_or(Value::X);

        // In forbidden state, both Q and Q̄ are 0 (violates Q = !Q̄)
        assert_eq!(q_value, Value::Zero, "Q should be 0 in forbidden state");
        assert_eq!(q_bar_value, Value::Zero, "Q̄ should be 0 in forbidden state");
    }

    #[test]
    fn sr_latch_state_transition_set_to_reset() {
        let mut db = create_test_db();
        let (s_power, r_power, nor1, nor2) = create_sr_latch(&mut db);

        // Start in SET state (S=1, R=0)
        db.get_power_mut(s_power).on = true;
        let mut sim = Simulator::new();
        sim.compute(&db);

        let q_output = db.gate_output(nor1);
        let q_value = sim.current.get(&q_output).copied().unwrap_or(Value::X);
        assert_eq!(q_value, Value::One, "Q should be 1 after SET");

        // Transition to RESET state (S=0, R=1)
        db.get_power_mut(s_power).on = false;
        db.get_power_mut(r_power).on = true;
        let mut sim2 = Simulator::new();
        sim2.compute(&db);

        let q_value_after = sim2.current.get(&q_output).copied().unwrap_or(Value::X);
        let q_bar_output = db.gate_output(nor2);
        let q_bar_value = sim2.current.get(&q_bar_output).copied().unwrap_or(Value::X);

        assert_eq!(q_value_after, Value::Zero, "Q should be 0 after RESET");
        assert_eq!(q_bar_value, Value::One, "Q̄ should be 1 after RESET");
    }

    #[test]
    fn sr_latch_stabilization_iterations() {
        let mut db = create_test_db();
        let (s_power, _r_power, _nor1, _nor2) = create_sr_latch(&mut db);

        // Set S=1, R=0
        db.get_power_mut(s_power).on = true;

        let mut sim = Simulator::new();
        sim.compute(&db);

        // Should stabilize in reasonable number of iterations
        // For a simple SR latch, should be < 10 iterations
        assert!(
            sim.last_iterations < 10,
            "SR latch should stabilize quickly, took {} iterations",
            sim.last_iterations
        );
        assert!(sim.last_iterations > 0, "Should take at least 1 iteration");
    }

    #[test]
    fn sr_latch_cycle_detection() {
        let mut db = create_test_db();
        let (s_power, _r_power, _nor1, _nor2) = create_sr_latch(&mut db);

        // Set S=1, R=0
        db.get_power_mut(s_power).on = true;

        let mut sim = Simulator::new();
        sim.compute(&db);

        // The simulator should handle the cycle without infinite recursion
        // If we get here, cycle detection worked (test would hang/crash otherwise)
        assert!(
            sim.last_iterations < 1000,
            "Should not take excessive iterations"
        );
    }
}
