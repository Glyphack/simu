use std::collections::HashSet;

use crate::app::{DB, GateKind, InstanceId, InstanceKind, Pin};

pub struct Simulator {
    db: DB,
    /// Final result
    current: HashSet<Pin>,
    /// Previous iteration state for fixed-point iteration
    previous: HashSet<Pin>,
}

impl Simulator {
    pub(crate) fn new(db: DB) -> Self {
        Self {
            db,
            current: Default::default(),
            previous: Default::default(),
        }
    }
    pub fn compute(&mut self) -> HashSet<Pin> {
        const MAX_ITERATIONS: usize = 100;

        let mut converged = false;
        for iteration in 0..MAX_ITERATIONS {
            self.previous = self.current.clone();
            self.current.clear();

            let mut evaluated = HashSet::new();
            let ids: Vec<InstanceId> = self.db.types.keys().collect();
            for id in ids {
                if !evaluated.contains(&id) {
                    self.eval_instance_lazy(id, &mut evaluated, &mut HashSet::new());
                }
            }

            // Check for convergence (fixed point reached)
            if self.current == self.previous {
                converged = true;
                break;
            }

            if iteration == MAX_ITERATIONS - 1 {
                log::warn!(
                    "Simulator reached max iterations ({MAX_ITERATIONS}) without converging"
                );
            }
        }

        if !converged {
            log::warn!("Fixed-point iteration did not converge");
        }

        self.current.clone()
    }

    fn eval_instance_lazy(
        &mut self,
        id: InstanceId,
        evaluated: &mut HashSet<InstanceId>,
        evaluating: &mut HashSet<InstanceId>,
    ) {
        if evaluated.contains(&id) {
            return;
        }
        if evaluating.contains(&id) {
            // Cycle detected
            return;
        }

        evaluating.insert(id);

        match self.db.ty(id) {
            InstanceKind::Power => {
                let p = self.db.get_power(id);
                if p.on {
                    self.current.insert(self.db.power_output(id));
                }
            }
            InstanceKind::Wire => {
                let a = self.db.wire_start(id);
                let b = self.db.wire_end(id);
                let a_on = self.eval_pin_lazy(a, evaluated, evaluating);
                let b_on = self.eval_pin_lazy(b, evaluated, evaluating);
                if a_on || b_on {
                    self.current.insert(a);
                    self.current.insert(b);
                }
            }
            InstanceKind::Gate(kind) => {
                let a = self.db.gate_inp1(id);
                let b = self.db.gate_inp2(id);
                let out = self.db.gate_output(id);
                let a_on = self.eval_pin_lazy(a, evaluated, evaluating);
                let b_on = self.eval_pin_lazy(b, evaluated, evaluating);
                let out_on = match kind {
                    GateKind::And => a_on && b_on,
                    GateKind::Nand => !(a_on && b_on),
                    GateKind::Or => a_on || b_on,
                    GateKind::Nor => !(a_on || b_on),
                    GateKind::Xor => (a_on && !b_on) || (!a_on && b_on),
                    GateKind::Xnor => a_on == b_on,
                };
                if out_on {
                    self.current.insert(out);
                }
            }
            InstanceKind::CustomCircuit(_) => {
                // TODO
            }
            InstanceKind::Lamp => {
                let a = self.db.lamp_input(id);
                let a_on = self.eval_pin_lazy(a, evaluated, evaluating);
                if a_on {
                    self.current.insert(a);
                }
            }
        }

        evaluating.remove(&id);
        evaluated.insert(id);
    }

    fn eval_pin_lazy(
        &mut self,
        pin: Pin,
        evaluated: &mut HashSet<InstanceId>,
        evaluating: &mut HashSet<InstanceId>,
    ) -> bool {
        // Check if already computed this iteration
        if self.current.contains(&pin) {
            return true;
        }

        // Check connected pins, evaluating sources on-demand
        for other in self.db.connected_pins(pin) {
            // Try to evaluate the source if not yet evaluated and not currently evaluating
            if !evaluated.contains(&other.ins) && !evaluating.contains(&other.ins) {
                self.eval_instance_lazy(other.ins, evaluated, evaluating);
            }

            // Check if it's in current after evaluation
            if self.current.contains(&other) {
                return true;
            }
        }

        // Fall back to previous (for cycle cases or unconnected pins)
        self.previous.contains(&pin)
    }
}
