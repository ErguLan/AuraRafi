//! Bounded inverse-kinematics solvers for procedural limbs.
//!
//! The solvers are deliberately independent from skeletal playback. A chain is
//! authored from current joint positions, preserves its segment lengths, and
//! returns solved world positions. The animation layer can then convert those
//! positions into bone rotations when keyframe playback lands.
//!
//! Two solvers cover the useful cases:
//!
//! - [`IkChain::solve_two_bone`] for hands, feet and other three-joint limbs,
//!   with an explicit pole vector so knees and elbows bend on the intended side.
//! - [`IkChain::solve_fabrik`] for longer chains such as tentacles, tails or
//!   procedural spines.
//!
//! Both are bounded, validate their inputs and never introduce segment stretch.

use std::fmt;

use glam::Vec3;

/// Smallest accepted segment length. Shorter segments are treated as
/// degenerate instead of producing NaNs during normalization.
const MIN_SEGMENT: f32 = 1.0e-5;
/// Default convergence threshold for the iterative solver.
const DEFAULT_TOLERANCE: f32 = 1.0e-3;
/// Hard iteration ceiling even if a caller asks for more.
const MAX_ITERATIONS: u32 = 128;

/// Why a chain could not be built or solved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IkError {
    /// A chain needs a root and at least one segment.
    TooFewJoints,
    /// A joint position or segment length is not finite.
    NonFiniteInput,
    /// Two consecutive joints share a position.
    DegenerateSegment,
    /// The target (or pole) is not a finite world position.
    InvalidTarget,
    /// The two-bone solver received a chain that does not have three joints.
    NotTwoBone,
}

impl fmt::Display for IkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewJoints => write!(formatter, "an IK chain needs at least two joints"),
            Self::NonFiniteInput => write!(formatter, "an IK joint position is not finite"),
            Self::DegenerateSegment => write!(formatter, "an IK segment has zero length"),
            Self::InvalidTarget => write!(formatter, "the IK target is not a finite position"),
            Self::NotTwoBone => {
                write!(formatter, "the two-bone solver needs exactly three joints")
            }
        }
    }
}

impl std::error::Error for IkError {}

/// Result of one solve pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IkSolution {
    /// Iterations actually spent. One for the analytic two-bone solver.
    pub iterations: u32,
    /// Distance between the solved tip and the requested target.
    pub error: f32,
    /// True when `error` is within the requested tolerance.
    pub reached: bool,
}

/// A length-preserving chain of world-space joint positions, ordered from root
/// to tip.
#[derive(Debug, Clone, PartialEq)]
pub struct IkChain {
    joints: Vec<Vec3>,
    lengths: Vec<f32>,
}

impl IkChain {
    /// Builds a chain from root-to-tip joint positions.
    pub fn new(joints: Vec<Vec3>) -> Result<Self, IkError> {
        if joints.len() < 2 {
            return Err(IkError::TooFewJoints);
        }
        if joints.iter().any(|joint| !joint.is_finite()) {
            return Err(IkError::NonFiniteInput);
        }
        let lengths = joints
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .collect::<Vec<_>>();
        if lengths
            .iter()
            .any(|length| !length.is_finite() || *length < MIN_SEGMENT)
        {
            return Err(IkError::DegenerateSegment);
        }
        Ok(Self { joints, lengths })
    }

    /// Current joint positions, root first.
    pub fn joints(&self) -> &[Vec3] {
        &self.joints
    }

    /// Fixed segment lengths, matching `joints().len() - 1`.
    pub fn lengths(&self) -> &[f32] {
        &self.lengths
    }

    pub fn root(&self) -> Vec3 {
        self.joints[0]
    }

    pub fn tip(&self) -> Vec3 {
        self.joints[self.joints.len() - 1]
    }

    /// Maximum tip distance from the root when every segment is straight.
    pub fn reach(&self) -> f32 {
        self.lengths.iter().sum()
    }

    /// Analytic three-joint solve. `pole` selects the bend side; moving it to
    /// the opposite side of the root-to-target line mirrors the elbow/knee.
    pub fn solve_two_bone(&mut self, target: Vec3, pole: Vec3) -> Result<IkSolution, IkError> {
        if self.joints.len() != 3 {
            return Err(IkError::NotTwoBone);
        }
        if !target.is_finite() || !pole.is_finite() {
            return Err(IkError::InvalidTarget);
        }

        let root = self.joints[0];
        let upper = self.lengths[0];
        let lower = self.lengths[1];
        let to_target = target - root;
        let distance = to_target.length();
        if distance < MIN_SEGMENT {
            return Err(IkError::InvalidTarget);
        }

        let direction = to_target / distance;
        let minimum = (upper - lower).abs() + MIN_SEGMENT;
        let maximum = (upper + lower - MIN_SEGMENT).max(minimum);
        let solved_distance = distance.clamp(minimum, maximum);
        let pole_vector = pole - root;
        let pole_plane = pole_vector - direction * pole_vector.dot(direction);
        let bend = if pole_plane.length_squared() > MIN_SEGMENT * MIN_SEGMENT {
            pole_plane.normalize()
        } else {
            perpendicular(direction)
        };

        // Law of cosines at the root. Clamping protects the acos-free form from
        // tiny floating-point excursions outside the valid triangle range.
        let cosine = ((upper * upper + solved_distance * solved_distance - lower * lower)
            / (2.0 * upper * solved_distance))
            .clamp(-1.0, 1.0);
        let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
        self.joints[1] = root + (direction * cosine + bend * sine) * upper;
        self.joints[2] = root + direction * solved_distance;

        let error = self.joints[2].distance(target);
        Ok(IkSolution {
            iterations: 1,
            error,
            reached: error <= DEFAULT_TOLERANCE,
        })
    }

    /// Iterative FABRIK solve for chains with three or more segments.
    ///
    /// `iterations` and `tolerance` are bounded; an unreachable target is
    /// handled by stretching the chain toward it without changing lengths.
    pub fn solve_fabrik(
        &mut self,
        target: Vec3,
        iterations: u32,
        tolerance: f32,
    ) -> Result<IkSolution, IkError> {
        if !target.is_finite() {
            return Err(IkError::InvalidTarget);
        }
        let root = self.joints[0];
        let distance = root.distance(target);
        if distance < MIN_SEGMENT {
            return Err(IkError::InvalidTarget);
        }
        let tolerance = if tolerance.is_finite() {
            tolerance.clamp(MIN_SEGMENT, 1.0)
        } else {
            DEFAULT_TOLERANCE
        };
        let iterations = iterations.clamp(1, MAX_ITERATIONS);
        let last = self.joints.len() - 1;

        if distance >= self.reach() - MIN_SEGMENT {
            let direction = (target - root) / distance;
            for index in 1..=last {
                self.joints[index] = self.joints[index - 1] + direction * self.lengths[index - 1];
            }
            let error = self.joints[last].distance(target);
            return Ok(IkSolution {
                iterations: 1,
                error,
                reached: error <= tolerance,
            });
        }

        let mut error = f32::INFINITY;
        let mut spent = 0;
        for iteration in 1..=iterations {
            spent = iteration;
            self.joints[last] = target;
            for index in (0..last).rev() {
                let direction = (self.joints[index] - self.joints[index + 1]).normalize_or_zero();
                if direction == Vec3::ZERO {
                    return Err(IkError::DegenerateSegment);
                }
                self.joints[index] = self.joints[index + 1] + direction * self.lengths[index];
            }
            self.joints[0] = root;
            for index in 1..=last {
                let direction = (self.joints[index] - self.joints[index - 1]).normalize_or_zero();
                if direction == Vec3::ZERO {
                    return Err(IkError::DegenerateSegment);
                }
                self.joints[index] = self.joints[index - 1] + direction * self.lengths[index - 1];
            }
            error = self.joints[last].distance(target);
            if error <= tolerance {
                break;
            }
        }

        Ok(IkSolution {
            iterations: spent,
            error,
            reached: error <= tolerance,
        })
    }
}

/// A stable perpendicular for the degenerate case where the pole lies on the
/// root-to-target line.
fn perpendicular(direction: Vec3) -> Vec3 {
    let axis = if direction.x.abs() < 0.9 {
        Vec3::X
    } else {
        Vec3::Y
    };
    direction.cross(axis).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(points: &[[f32; 3]]) -> IkChain {
        IkChain::new(points.iter().copied().map(Vec3::from).collect()).unwrap()
    }

    fn assert_lengths(chain: &IkChain, expected: &[f32]) {
        for (actual, expected) in chain.lengths().iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-4);
        }
    }

    #[test]
    fn two_bone_reaches_a_target_inside_its_range() {
        let mut limb = chain(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]]);
        let target = Vec3::new(1.2, 0.8, 0.0);

        let solution = limb.solve_two_bone(target, Vec3::new(0.0, 0.0, 1.0)).unwrap();

        assert!(solution.reached, "error {}", solution.error);
        assert!(limb.tip().distance(target) < 1.0e-3);
        assert_lengths(&limb, &[1.0, 1.0]);
    }

    #[test]
    fn two_bone_stretches_toward_an_unreachable_target() {
        let mut limb = chain(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]]);

        let solution = limb
            .solve_two_bone(Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0))
            .unwrap();

        assert!(!solution.reached);
        assert!((limb.tip().x - 2.0).abs() < 1.0e-3);
        assert!(solution.error > 7.9);
        assert_lengths(&limb, &[1.0, 1.0]);
    }

    #[test]
    fn the_pole_vector_selects_the_bend_side() {
        let mut positive = chain(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]]);
        let mut negative = positive.clone();
        let target = Vec3::new(1.2, 0.0, 0.0);

        positive
            .solve_two_bone(target, Vec3::new(0.0, 0.0, 1.0))
            .unwrap();
        negative
            .solve_two_bone(target, Vec3::new(0.0, 0.0, -1.0))
            .unwrap();

        assert!(positive.joints()[1].z > 0.0);
        assert!(negative.joints()[1].z < 0.0);
    }

    #[test]
    fn fabrik_reaches_a_target_with_a_longer_chain() {
        let mut chain = chain(&[
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 3.0, 0.0],
        ]);
        let target = Vec3::new(1.4, 1.3, 0.6);

        let solution = chain.solve_fabrik(target, 48, 1.0e-3).unwrap();

        assert!(solution.reached, "error {}", solution.error);
        assert!(chain.tip().distance(target) < 1.0e-2);
        assert_lengths(&chain, &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn fabrik_stretches_without_stretching_segments() {
        let mut chain = chain(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);

        let solution = chain
            .solve_fabrik(Vec3::new(9.0, 0.0, 0.0), 8, 1.0e-3)
            .unwrap();

        assert!(!solution.reached);
        assert!((chain.tip().x - 2.0).abs() < 1.0e-4);
        assert_lengths(&chain, &[1.0, 1.0]);
    }

    #[test]
    fn degenerate_chains_and_targets_are_rejected() {
        assert_eq!(
            IkChain::new(vec![Vec3::ZERO, Vec3::ZERO]),
            Err(IkError::DegenerateSegment)
        );
        assert_eq!(IkChain::new(vec![Vec3::ZERO]), Err(IkError::TooFewJoints));
        assert_eq!(
            IkChain::new(vec![Vec3::ZERO, Vec3::new(f32::NAN, 0.0, 0.0)]),
            Err(IkError::NonFiniteInput)
        );

        let mut limb = chain(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]]);
        assert_eq!(
            limb.solve_two_bone(Vec3::ZERO, Vec3::Z),
            Err(IkError::InvalidTarget)
        );
        assert_eq!(
            limb.solve_fabrik(Vec3::new(f32::NAN, 0.0, 0.0), 8, 1.0e-3),
            Err(IkError::InvalidTarget)
        );
    }

    #[test]
    fn the_two_bone_solver_rejects_longer_chains() {
        let mut chain = chain(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0], [0.0, 3.0, 0.0]]);
        assert_eq!(
            chain.solve_two_bone(Vec3::new(1.0, 0.0, 0.0), Vec3::Z),
            Err(IkError::NotTwoBone)
        );
    }
}
