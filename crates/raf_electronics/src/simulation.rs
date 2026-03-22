//! DC circuit simulation engine using Modified Nodal Analysis (MNA).
//!
//! Pure Rust, no external dependencies. Solves `Gv = i` using
//! Gaussian elimination for resistive DC circuits with LEDs.
//!
//! Technology based on Yoll AU - yoll.site

use crate::component::SimModel;
use crate::netlist::Netlist;
use crate::schematic::Schematic;
use std::collections::HashMap;
use uuid::Uuid;

/// Results of a DC simulation.
#[derive(Debug, Clone)]
pub struct SimulationResults {
    /// Voltage at each net node (net_id -> volts).
    pub node_voltages: HashMap<usize, f64>,
    /// Current through each component (component_index -> amps).
    /// Positive = conventional current direction (pin 0 -> pin 1).
    pub component_currents: HashMap<usize, f64>,
    /// Power dissipated by each component (component_index -> watts).
    pub component_power: HashMap<usize, f64>,
    /// Whether the simulation converged successfully.
    pub converged: bool,
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
            messages: Vec::new(),
        }
    }
}

/// Run a DC simulation on a schematic.
///
/// Uses Modified Nodal Analysis (MNA):
/// 1. Build netlist to identify nodes.
/// 2. Set up conductance matrix G and current vector I.
/// 3. Stamp each component into G and I.
/// 4. Solve Gv = I with Gaussian elimination.
/// 5. Compute branch currents and power.
pub fn simulate_dc(schematic: &Schematic) -> SimulationResults {
    let netlist = Netlist::from_schematic(schematic);
    let mut results = SimulationResults::default();

    if netlist.nets.is_empty() || schematic.components.is_empty() {
        results.messages.push("Empty circuit - nothing to simulate.".to_string());
        return results;
    }

    let net_count = netlist.nets.len();

    // We need a ground reference. Use the node with the most connections
    // to pins marked as Ground, or node 0 as fallback.
    let ground_node = find_ground_node(schematic, &netlist).unwrap_or(0);

    // MNA matrix dimension: (net_count - 1) since ground is removed.
    // But if there's only one net, nothing to solve.
    if net_count <= 1 {
        results.converged = true;
        results.messages.push("Single node circuit.".to_string());
        return results;
    }

    // Map net IDs to matrix indices (skipping ground).
    let mut net_to_idx: HashMap<usize, usize> = HashMap::new();
    let mut idx = 0;
    for net in &netlist.nets {
        if net.id == ground_node {
            continue;
        }
        net_to_idx.insert(net.id, idx);
        idx += 1;
    }

    let dim = net_to_idx.len();
    if dim == 0 {
        results.converged = true;
        return results;
    }

    // Conductance matrix G and current vector I.
    let mut g_matrix: Vec<Vec<f64>> = vec![vec![0.0; dim]; dim];
    let mut i_vector: Vec<f64> = vec![0.0; dim];

    // Stamp each component into the MNA system.
    for (ci, comp) in schematic.components.iter().enumerate() {
        // Find net IDs for each pin.
        let pin_nets: Vec<Option<usize>> = comp
            .pins
            .iter()
            .enumerate()
            .map(|(pi, _)| {
                netlist
                    .net_for_pin(ci, pi)
                    .map(|n| n.id)
            })
            .collect();

        match &comp.sim_model {
            SimModel::Resistor { ohms } => {
                if *ohms <= 0.0 {
                    continue;
                }
                let conductance = 1.0 / ohms;
                stamp_resistor(
                    &mut g_matrix,
                    pin_nets.get(0).copied().flatten(),
                    pin_nets.get(1).copied().flatten(),
                    conductance,
                    ground_node,
                    &net_to_idx,
                );
            }
            SimModel::Capacitor { .. } => {
                // DC steady-state: capacitor = open circuit. No stamp.
            }
            SimModel::Led { forward_voltage } => {
                // Model LED as a voltage source in series with small resistance.
                // Simplified: treat as resistor with voltage drop.
                // In DC, LED conducts if voltage across > Vf.
                // Approximate with small resistance (100 ohms) when forward biased.
                let led_resistance = 100.0;
                let conductance = 1.0 / led_resistance;
                stamp_resistor(
                    &mut g_matrix,
                    pin_nets.get(0).copied().flatten(),
                    pin_nets.get(1).copied().flatten(),
                    conductance,
                    ground_node,
                    &net_to_idx,
                );
                // Stamp the forward voltage as a current source.
                let i_led = *forward_voltage / led_resistance;
                stamp_current_source(
                    &mut i_vector,
                    pin_nets.get(0).copied().flatten(),
                    pin_nets.get(1).copied().flatten(),
                    i_led,
                    ground_node,
                    &net_to_idx,
                );
            }
            SimModel::Wire => {
                // Wire = very low resistance (0.001 ohm).
                let conductance = 1000.0;
                stamp_resistor(
                    &mut g_matrix,
                    pin_nets.get(0).copied().flatten(),
                    pin_nets.get(1).copied().flatten(),
                    conductance,
                    ground_node,
                    &net_to_idx,
                );
            }
            SimModel::Magnet { .. } => {
                // Magnets are passive - no electrical stamp in DC.
            }
        }
    }

    // Solve Gv = I using Gaussian elimination with partial pivoting.
    let voltages = gaussian_solve(&mut g_matrix, &mut i_vector);

    match voltages {
        Some(v) => {
            results.converged = true;

            // Map voltages back to net IDs.
            results.node_voltages.insert(ground_node, 0.0);
            for (net_id, idx) in &net_to_idx {
                results.node_voltages.insert(*net_id, v[*idx]);
            }

            // Compute branch currents and power.
            for (ci, comp) in schematic.components.iter().enumerate() {
                let pin_nets: Vec<Option<usize>> = comp
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(pi, _)| netlist.net_for_pin(ci, pi).map(|n| n.id))
                    .collect();

                let v0 = pin_nets
                    .get(0)
                    .copied()
                    .flatten()
                    .and_then(|n| results.node_voltages.get(&n))
                    .copied()
                    .unwrap_or(0.0);
                let v1 = pin_nets
                    .get(1)
                    .copied()
                    .flatten()
                    .and_then(|n| results.node_voltages.get(&n))
                    .copied()
                    .unwrap_or(0.0);

                let (current, power) = match &comp.sim_model {
                    SimModel::Resistor { ohms } => {
                        if *ohms > 0.0 {
                            let i = (v0 - v1) / ohms;
                            let p = i * i * ohms;
                            (i, p)
                        } else {
                            (0.0, 0.0)
                        }
                    }
                    SimModel::Led { forward_voltage } => {
                        let led_r = 100.0;
                        let vdrop = v0 - v1;
                        let i = if vdrop > *forward_voltage {
                            (vdrop - forward_voltage) / led_r
                        } else {
                            0.0
                        };
                        let p = i * vdrop;
                        (i, p)
                    }
                    SimModel::Capacitor { .. } => (0.0, 0.0),
                    SimModel::Wire => {
                        let i = (v0 - v1) * 1000.0;
                        (i, 0.0)
                    }
                    SimModel::Magnet { .. } => (0.0, 0.0),
                };

                results.component_currents.insert(ci, current);
                results.component_power.insert(ci, power);
            }

            results.messages.push("DC simulation completed successfully.".to_string());
        }
        None => {
            results.converged = false;
            results.messages.push(
                "Simulation failed: singular matrix (check circuit connectivity).".to_string(),
            );
        }
    }

    results
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
    let idx_a = net_a.and_then(|n| if n == ground { None } else { net_to_idx.get(&n).copied() });
    let idx_b = net_b.and_then(|n| if n == ground { None } else { net_to_idx.get(&n).copied() });

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
    let idx_plus =
        net_plus.and_then(|n| if n == ground { None } else { net_to_idx.get(&n).copied() });
    let idx_minus =
        net_minus.and_then(|n| if n == ground { None } else { net_to_idx.get(&n).copied() });

    if let Some(p) = idx_plus {
        i_vec[p] += current;
    }
    if let Some(m) = idx_minus {
        i_vec[m] -= current;
    }
}

/// Solve Ax = b using Gaussian elimination with partial pivoting.
/// Returns None if matrix is singular.
fn gaussian_solve(a: &mut Vec<Vec<f64>>, b: &mut Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    if n == 0 {
        return Some(Vec::new());
    }

    // Forward elimination with partial pivoting.
    for col in 0..n {
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

        if max_val < 1e-15 {
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
        let mut sum = b[i];
        for j in (i + 1)..n {
            sum -= a[i][j] * x[j];
        }
        if a[i][i].abs() < 1e-15 {
            return None;
        }
        x[i] = sum / a[i][i];
    }

    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;
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
