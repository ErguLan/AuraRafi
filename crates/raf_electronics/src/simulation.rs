//! DC circuit simulation using nodal analysis and Norton source equivalents.
//!
//! Pure Rust, no external dependencies. Solves `Gv = i` using
//! Gaussian elimination for resistive DC circuits with LEDs.
//!
//! Technology based on Yoll AU - yoll.site

use crate::component::SimModel;
use crate::netlist::Netlist;
use crate::schematic::Schematic;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Results of a DC simulation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimulationResults {
    /// Voltage at each net node (net_id -> volts).
    pub node_voltages: HashMap<usize, f64>,
    /// Current through each component (component_index -> amps).
    /// Positive = conventional current direction (pin 0 -> pin 1).
    pub component_currents: HashMap<usize, f64>,
    /// Signed power (component_index -> watts): positive absorbed, negative delivered.
    pub component_power: HashMap<usize, f64>,
    /// Whether the simulation converged successfully.
    pub converged: bool,
    pub max_kcl_residual_amps: f64,
    pub power_balance_watts: f64,
    /// Human-readable messages / warnings.
    pub messages: Vec<String>,
}

impl Default for SimulationResults {
    fn default() -> Self {
        Self {
            node_voltages: HashMap::new(),
            component_currents: HashMap::new(),
            component_power: HashMap::new(),
            converged: false,
            max_kcl_residual_amps: 0.0,
            power_balance_watts: 0.0,
            messages: Vec::new(),
        }
    }
}

/// Run a DC simulation on a schematic.
///
/// Uses nodal analysis with finite source resistance:
/// 1. Build netlist to identify nodes.
/// 2. Set up conductance matrix G and current vector I.
/// 3. Stamp each component into G and I.
/// 4. Solve Gv = I with Gaussian elimination.
/// 5. Compute branch currents and power.
pub fn simulate_dc(schematic: &Schematic) -> SimulationResults {
    simulate_dc_cancellable(schematic, &AtomicBool::new(false)).unwrap_or_default()
}

/// Piecewise-linear LED DC analysis with cooperative cancellation. No partial
/// results are presented as converged, and the reported branch law is exactly
/// the one used to assemble the matrix.
pub fn simulate_dc_cancellable(
    schematic: &Schematic,
    cancel: &AtomicBool,
) -> Option<SimulationResults> {
    analyze_dc_cancellable(schematic, cancel).map(|(result, _)| result)
}

/// Return the exact netlist used for the solve so presentation can resolve
/// measurements without rebuilding topology on the editor thread.
pub fn analyze_dc_cancellable(
    schematic: &Schematic,
    cancel: &AtomicBool,
) -> Option<(SimulationResults, Netlist)> {
    let netlist = Netlist::from_schematic_cancellable(schematic, cancel)?;
    let result = solve_dc(schematic, &netlist, cancel)?;
    Some((result, netlist))
}

fn solve_dc(
    schematic: &Schematic,
    netlist: &Netlist,
    cancel: &AtomicBool,
) -> Option<SimulationResults> {
    const LED_R: f64 = 100.0;
    const LED_OFF_G: f64 = 1e-12;
    const SOURCE_R: f64 = 0.01;
    let mut results = SimulationResults::default();
    if netlist.nets.is_empty() || schematic.components.is_empty() {
        results
            .messages
            .push("Empty circuit - nothing to simulate.".into());
        return Some(results);
    }
    for comp in &schematic.components {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let valid = match comp.sim_model {
            SimModel::Resistor { ohms } => ohms.is_finite() && ohms > 0.0,
            SimModel::Capacitor { farads } => farads.is_finite() && farads >= 0.0,
            SimModel::Led { forward_voltage } => {
                forward_voltage.is_finite() && forward_voltage >= 0.0
            }
            SimModel::DcSource { voltage } => voltage.is_finite(),
            _ => true,
        };
        if !valid {
            results
                .messages
                .push(format!("Invalid simulation value on {}", comp.designator));
            return Some(results);
        }
        if matches!(comp.sim_model, SimModel::Magnet { .. }) {
            results.messages.push(format!(
                "{}: magnetic induction is not simulated by this DC solver.",
                comp.designator
            ));
        }
    }
    let explicit_ground = find_ground_node(schematic, &netlist);
    let supply_return = schematic
        .components
        .iter()
        .enumerate()
        .filter(|(_, component)| matches!(component.sim_model, SimModel::DcSource { .. }))
        .find_map(|(index, _)| netlist.net_for_pin(index, 1).map(|net| net.id));
    let ground_node = explicit_ground.or(supply_return).unwrap_or(0);
    if explicit_ground.is_none() {
        results.messages.push(format!(
            "No explicit ground; using '{}' as voltage reference.",
            netlist.nets[ground_node].name
        ));
    }
    let pin_nets: Vec<Vec<Option<usize>>> = schematic
        .components
        .iter()
        .enumerate()
        .map(|(ci, comp)| {
            comp.pins
                .iter()
                .enumerate()
                .map(|(pi, _)| netlist.net_for_pin(ci, pi).map(|n| n.id))
                .collect()
        })
        .collect();
    for (ci, comp) in schematic.components.iter().enumerate() {
        if matches!(comp.sim_model, SimModel::DcSource { voltage } if voltage.abs() > 1e-12)
            && pin_nets[ci].first() == pin_nets[ci].get(1)
        {
            results.messages.push(format!(
                "Voltage source {} has shorted terminals.",
                comp.designator
            ));
            return Some(results);
        }
    }
    let net_to_idx: HashMap<usize, usize> = netlist
        .nets
        .iter()
        .filter(|net| net.id != ground_node)
        .enumerate()
        .map(|(idx, net)| (net.id, idx))
        .collect();
    let dim = net_to_idx.len();
    // Keep the existing dense solver bounded instead of allocating gigabytes
    // for a document it cannot solve interactively. Cancellation remains live.
    if dim > 4096 {
        results
            .messages
            .push("DC analysis exceeds the dense solver limit of 4096 nodes.".into());
        return Some(results);
    }
    let mut active_leds = vec![false; schematic.components.len()];
    let mut solved = None;
    for _ in 0..64 {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut matrix = vec![vec![0.0; dim]; dim];
        let mut rhs = vec![0.0; dim];
        for (ci, comp) in schematic.components.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let a = pin_nets[ci].first().copied().flatten();
            let b = pin_nets[ci].get(1).copied().flatten();
            let (conductance, injection) = match comp.sim_model {
                SimModel::Resistor { ohms } => (1.0 / ohms, 0.0),
                SimModel::Wire if comp.pins.len() >= 2 => (1000.0, 0.0),
                SimModel::DcSource { voltage } => (1.0 / SOURCE_R, voltage / SOURCE_R),
                SimModel::Led { forward_voltage } if active_leds[ci] => {
                    (LED_OFF_G + 1.0 / LED_R, forward_voltage / LED_R)
                }
                SimModel::Led { .. } => (LED_OFF_G, 0.0),
                _ => continue,
            };
            stamp_resistor(&mut matrix, a, b, conductance, ground_node, &net_to_idx);
            stamp_current_source(&mut rhs, a, b, injection, ground_node, &net_to_idx);
        }
        let Some(voltages) = gaussian_solve_cancellable(&mut matrix, &mut rhs, cancel) else {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            results.messages.push("Simulation failed: singular or non-finite matrix (check floating circuit islands).".into());
            return Some(results);
        };
        let voltage = |net: Option<usize>| {
            net.and_then(|id| net_to_idx.get(&id))
                .map(|&idx| voltages[idx])
                .unwrap_or(0.0)
        };
        let mut changed = false;
        for (ci, comp) in schematic.components.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            if let SimModel::Led { forward_voltage } = comp.sim_model {
                let drop = voltage(pin_nets[ci].first().copied().flatten())
                    - voltage(pin_nets[ci].get(1).copied().flatten());
                let active = drop > forward_voltage;
                changed |= active != active_leds[ci];
                active_leds[ci] = active;
            }
        }
        if !changed {
            solved = Some(voltages);
            break;
        }
    }
    let Some(voltages) = solved else {
        results
            .messages
            .push("LED DC analysis did not converge within 64 iterations.".into());
        return Some(results);
    };
    results.node_voltages.insert(ground_node, 0.0);
    for (&net, &idx) in &net_to_idx {
        results.node_voltages.insert(net, voltages[idx]);
    }
    for (ci, comp) in schematic.components.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let voltage = |index| {
            pin_nets[ci]
                .get(index)
                .copied()
                .flatten()
                .and_then(|id| results.node_voltages.get(&id))
                .copied()
                .unwrap_or(0.0)
        };
        let drop = voltage(0) - voltage(1);
        let current = match comp.sim_model {
            SimModel::Resistor { ohms } => drop / ohms,
            SimModel::Led { forward_voltage } => {
                LED_OFF_G * drop + (drop - forward_voltage).max(0.0) / LED_R
            }
            SimModel::Wire if comp.pins.len() >= 2 => drop * 1000.0,
            SimModel::DcSource { voltage } => (drop - voltage) / SOURCE_R,
            _ => 0.0,
        };
        results.component_currents.insert(ci, current);
        results.component_power.insert(ci, current * drop);
    }
    let mut residual = HashMap::<usize, f64>::new();
    for (ci, current) in &results.component_currents {
        if let Some(net) = pin_nets[*ci].first().copied().flatten() {
            *residual.entry(net).or_default() += current;
        }
        if let Some(net) = pin_nets[*ci].get(1).copied().flatten() {
            *residual.entry(net).or_default() -= current;
        }
    }
    results.max_kcl_residual_amps = residual
        .values()
        .fold(0.0_f64, |max, value| max.max(value.abs()));
    results.power_balance_watts = results.component_power.values().sum();
    let total_current: f64 = results.component_currents.values().map(|i| i.abs()).sum();
    let total_power: f64 = results.component_power.values().map(|p| p.abs()).sum();
    results.converged = results.max_kcl_residual_amps <= 1e-8 + total_current * 1e-6
        && results.power_balance_watts.abs() <= 1e-8 + total_power * 1e-6;
    if !results.converged {
        results
            .messages
            .push("DC solution failed current or power balance verification.".into());
        return Some(results);
    }
    results.messages.push("DC simulation completed successfully. LEDs use a piecewise-linear 100 ohm forward model; sources have 0.01 ohm internal resistance.".into());
    Some(results)
}

/// Find the ground reference node from GND-direction pins.
fn find_ground_node(schematic: &Schematic, netlist: &Netlist) -> Option<usize> {
    use crate::component::PinDirection;

    for (ci, comp) in schematic.components.iter().enumerate() {
        for (pi, pin) in comp.pins.iter().enumerate() {
            if pin.direction == PinDirection::Ground {
                if let Some(net) = netlist.net_for_pin(ci, pi) {
                    return Some(net.id);
                }
            }
        }
    }
    None
}

/// Stamp a resistor (conductance) into the MNA matrix.
fn stamp_resistor(
    g: &mut [Vec<f64>],
    net_a: Option<usize>,
    net_b: Option<usize>,
    conductance: f64,
    ground: usize,
    net_to_idx: &HashMap<usize, usize>,
) {
    let idx_a = net_a.and_then(|n| {
        if n == ground {
            None
        } else {
            net_to_idx.get(&n).copied()
        }
    });
    let idx_b = net_b.and_then(|n| {
        if n == ground {
            None
        } else {
            net_to_idx.get(&n).copied()
        }
    });

    if let Some(a) = idx_a {
        g[a][a] += conductance;
    }
    if let Some(b) = idx_b {
        g[b][b] += conductance;
    }
    if let (Some(a), Some(b)) = (idx_a, idx_b) {
        g[a][b] -= conductance;
        g[b][a] -= conductance;
    }
}

/// Stamp a current source into the MNA current vector.
fn stamp_current_source(
    i_vec: &mut [f64],
    net_plus: Option<usize>,
    net_minus: Option<usize>,
    current: f64,
    ground: usize,
    net_to_idx: &HashMap<usize, usize>,
) {
    let idx_plus = net_plus.and_then(|n| {
        if n == ground {
            None
        } else {
            net_to_idx.get(&n).copied()
        }
    });
    let idx_minus = net_minus.and_then(|n| {
        if n == ground {
            None
        } else {
            net_to_idx.get(&n).copied()
        }
    });

    if let Some(p) = idx_plus {
        i_vec[p] += current;
    }
    if let Some(m) = idx_minus {
        i_vec[m] -= current;
    }
}

/// Solve Ax = b using Gaussian elimination with partial pivoting.
/// Returns None if matrix is singular.
#[cfg(test)]
fn gaussian_solve(a: &mut Vec<Vec<f64>>, b: &mut Vec<f64>) -> Option<Vec<f64>> {
    gaussian_solve_cancellable(a, b, &AtomicBool::new(false))
}

fn gaussian_solve_cancellable(
    a: &mut Vec<Vec<f64>>,
    b: &mut Vec<f64>,
    cancel: &AtomicBool,
) -> Option<Vec<f64>> {
    let n = b.len();
    if n == 0 {
        return Some(Vec::new());
    }

    // Forward elimination with partial pivoting.
    for col in 0..n {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        // Find pivot.
        let mut max_row = col;
        let mut max_val = a[col][col].abs();
        for row in (col + 1)..n {
            let val = a[row][col].abs();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }

        if !max_val.is_finite() || max_val < 1e-15 {
            return None; // Singular matrix.
        }

        // Swap rows.
        if max_row != col {
            a.swap(col, max_row);
            b.swap(col, max_row);
        }

        // Eliminate below.
        let pivot = a[col][col];
        for row in (col + 1)..n {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let factor = a[row][col] / pivot;
            for j in col..n {
                a[row][j] -= factor * a[col][j];
            }
            b[row] -= factor * b[col];
        }
    }

    // Back substitution.
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut sum = b[i];
        for j in (i + 1)..n {
            sum -= a[i][j] * x[j];
        }
        if a[i][i].abs() < 1e-15 {
            return None;
        }
        x[i] = sum / a[i][i];
        if !x[i].is_finite() {
            return None;
        }
    }

    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schematic::Schematic;

    #[test]
    fn empty_circuit() {
        let sch = Schematic::new("Test");
        let result = simulate_dc(&sch);
        assert!(!result.converged || result.messages.iter().any(|m| m.contains("Empty")));
    }

    #[test]
    fn gaussian_solve_2x2() {
        // 2x + y = 5
        // x + 3y = 7
        // Solution: x = 1.6, y = 1.8
        let mut a = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let mut b = vec![5.0, 7.0];
        let x = gaussian_solve(&mut a, &mut b).unwrap();
        assert!((x[0] - 1.6).abs() < 0.001);
        assert!((x[1] - 1.8).abs() < 0.001);
    }

    #[test]
    fn gaussian_solve_singular() {
        let mut a = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let mut b = vec![3.0, 6.0];
        assert!(gaussian_solve(&mut a, &mut b).is_none());
    }
}
