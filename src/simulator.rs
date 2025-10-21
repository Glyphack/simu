use std::collections::{HashMap, HashSet};

use log;

use crate::{
    app::{DB, GateKind, InstanceId, InstanceKind, Pin},
    assets::PinKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Zero,
    One,
    X,
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

pub struct Simulator {
    db: DB,
    /// Final result
    pub current: HashMap<Pin, Value>,
    /// Number of iterations taken in last compute
    pub last_iterations: usize,
}

impl Simulator {
    pub(crate) fn new(db: DB) -> Self {
        Self {
            db,
            current: Default::default(),
            last_iterations: 0,
        }
    }

    fn rebuild_sorted_instances(&self) -> Vec<InstanceId> {
        let mut ids: Vec<InstanceId> = self.db.types.keys().collect();
        ids.sort_unstable();
        ids
    }

    pub fn compute(&mut self) -> HashSet<Pin> {
        log::info!("=== Begin simulation ===");
        let sorted_instances = self.rebuild_sorted_instances();

        for &id in &sorted_instances {
            let mut eval_instance = EvalInstance::new();
            eval_instance.evaluate(self, id);
        }

        self.current
            .iter()
            .filter_map(|(pin, val)| if val.is_one() { Some(*pin) } else { None })
            .collect()
    }
}

pub struct EvalInstance {
    eval_stack: Vec<InstanceId>,
}

impl EvalInstance {
    fn new() -> Self {
        Self {
            eval_stack: Vec::new(),
        }
    }

    fn evaluate(&mut self, simulator: &mut Simulator, id: InstanceId) {
        self.eval_instance(simulator, id);
    }

    fn eval_instance(&mut self, simulator: &mut Simulator, id: InstanceId) {
        log::info!("eval instance: {:?} {}", simulator.db.ty(id), id);
        if self.eval_stack.contains(&id) {
            // TODO: Cycles are not handled
            return;
        }
        self.eval_stack.push(id);

        match simulator.db.ty(id) {
            InstanceKind::Power => {
                let p = simulator.db.get_power(id);
                let out = simulator.db.power_output(id);
                let val = if p.on { Value::One } else { Value::Zero };
                simulator.current.insert(out, val);
            }
            InstanceKind::Wire => {
                let input = wire_input(&simulator.db, id);
                let other = if simulator.db.wire_start(id) == input {
                    simulator.db.wire_end(id)
                } else {
                    simulator.db.wire_start(id)
                };

                let result = self.eval_pin(simulator, input);

                simulator.current.insert(input, result);
                simulator.current.insert(other, result);
            }
            InstanceKind::Gate(kind) => {
                let inp1 = simulator.db.gate_inp1(id);
                let inp2 = simulator.db.gate_inp2(id);
                let out = simulator.db.gate_output(id);

                let a = self.eval_pin(simulator, inp1);
                let b = self.eval_pin(simulator, inp2);
                let out_val = match kind {
                    GateKind::And => a.and(b),
                    GateKind::Nand => a.and(b).not(),
                    GateKind::Or => a.or(b),
                    GateKind::Nor => a.or(b).not(),
                    GateKind::Xor => a.xor(b),
                    GateKind::Xnor => a.xnor(b),
                };

                simulator.current.insert(out, out_val);
            }
            InstanceKind::CustomCircuit(_) => {}
            InstanceKind::Lamp => {
                let inp = simulator.db.lamp_input(id);
                let val = self.eval_pin(simulator, inp);
                simulator.current.insert(inp, val);
            }
        }
    }

    fn eval_pin(&mut self, simulator: &mut Simulator, pin: Pin) -> Value {
        log::info!("eval pin {}", pin.display(&simulator.db));
        let mut connected = simulator.db.connected_pins(pin);
        connected.push(pin);
        connected.sort_unstable();
        connected.dedup();

        let mut result = Value::Zero;
        for other in connected {
            if simulator.db.pin_info(other).kind != PinKind::Output {
                continue;
            }
            self.eval_instance(simulator, other.ins);
            if let Some(&val) = simulator.current.get(&other) {
                log::info!(
                    "connection: {} value: {:?}",
                    other.display(&simulator.db),
                    val
                );
                result = result.or(val);
            }
        }

        log::info!("result of {}: {:?}", pin.display(&simulator.db), result);
        result
    }
}

// Returns the input of the wire
// The head of wire that is connected to another output is the input pin of the wire.
// When both heads are not connected start is the input.
fn wire_input(db: &DB, w_id: InstanceId) -> Pin {
    let start = db.wire_start(w_id);
    let end = db.wire_end(w_id);

    // Use pin_info which now contains the wire input detection logic
    if db.pin_info(start).kind == PinKind::Input {
        start
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Connection, Gate, GateKind, Lamp, Power};
    use egui::Pos2;

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

        let mut sim = Simulator::new(db.clone());
        let result = sim.compute();

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

        let mut sim = Simulator::new(db.clone());
        let result = sim.compute();

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

        let mut sim = Simulator::new(db.clone());
        let result = sim.compute();

        assert!(result.contains(&gate_out), "AND gate output should be on");
        assert!(result.contains(&lamp_in), "Lamp should be on");
    }
}
