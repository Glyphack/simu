use std::collections::{HashMap, HashSet};

use log;

use crate::{
    assets::PinKind,
    db::{Circuit, DB, GateKind, InstanceId, InstanceKind, Pin},
};

const MAX_ITERATIONS: usize = 10;
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
            InstanceKind::Module(module_def_id) => {
                for pin in circuit.get_module(id).pins() {
                    let v = self.get_pin_value(db, circuit, pin);
                    self.current.insert(pin, v);
                    let mapped_pin = pin.is_passthrough(db).unwrap_or(pin);
                    self.current.insert(mapped_pin, v);
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

        // Not has one input so handle specially
        if matches!(kind, GateKind::Not) {
            let inp1 = gate_inp1(id);
            let out = Pin::new(id, 1, PinKind::Output);
            let a = self.get_pin_value(db, circuit, inp1);
            let out_val = a.not();
            self.current.insert(out, out_val);
            return;
        }

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
            GateKind::Not => unreachable!("Handled above"),
        };

        self.current.insert(out, out_val);
    }

    fn evaluate_lamp(&mut self, db: &DB, circuit: &Circuit, id: InstanceId) {
        let inp = lamp_input(id);
        let val = self.get_pin_value(db, circuit, inp);
        self.current.insert(inp, val);
    }

    fn get_pin_value(&self, db: &DB, circuit: &Circuit, pin: Pin) -> Value {
        let mapped_pin = pin.is_passthrough(db).unwrap_or(pin);
        let conns = circuit.connections_containing(mapped_pin);

        let mut result = Value::Zero;
        for conn in conns {
            let connected_pin = conn.get_other_pin(mapped_pin);
            if connected_pin.kind != PinKind::Output {
                continue;
            }

            if let Some(&val) = self.current.get(&connected_pin) {
                result = result.or(val);
            }
        }

        result
    }
}

pub fn gate_output_n(id: InstanceId, n: u32) -> Pin {
    assert!(n == 0, "Gates only have 1 output");
    Pin::new(id, 2, PinKind::Output)
}

pub fn gate_inp1(id: InstanceId) -> Pin {
    Pin::new(id, 0, PinKind::Input)
}

pub fn gate_inp2(id: InstanceId) -> Pin {
    Pin::new(id, 1, PinKind::Input)
}

pub fn gate_output(id: InstanceId) -> Pin {
    gate_output_n(id, 0)
}

pub fn wire_start(id: InstanceId) -> Pin {
    Pin::new(id, 0, PinKind::Input)
}

pub fn wire_end(id: InstanceId) -> Pin {
    Pin::new(id, 1, PinKind::Output)
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
