mod router;

#[cfg(test)]
mod tests;

pub use router::{RoutedVoiceFrame, SpatialVoiceRouter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DistanceModel {
    Linear {
        min_distance: f64,
        max_distance: f64,
    },

    Inverse {
        min_distance: f64,
        max_distance: f64,
        rolloff: f64,
    },
}

impl Default for DistanceModel {
    fn default() -> Self {
        Self::Inverse {
            min_distance: 1.5,
            max_distance: 25.0,
            rolloff: 1.0,
        }
    }
}

impl DistanceModel {
    pub fn max_distance(&self) -> f64 {
        match self {
            Self::Linear { max_distance, .. } => *max_distance,
            Self::Inverse { max_distance, .. } => *max_distance,
        }
    }

    pub fn compute_volume(&self, distance: f64) -> f32 {
        match *self {
            Self::Linear {
                min_distance,
                max_distance,
            } => {
                if distance <= min_distance {
                    1.0
                } else if distance >= max_distance {
                    0.0
                } else {
                    let factor = (max_distance - distance) / (max_distance - min_distance);
                    factor.clamp(0.0, 1.0) as f32
                }
            }
            Self::Inverse {
                min_distance,
                max_distance,
                rolloff,
            } => {
                if distance <= min_distance {
                    1.0
                } else if distance >= max_distance {
                    0.0
                } else {
                    let denom = min_distance + rolloff * (distance - min_distance);
                    if denom <= 0.0 {
                        1.0
                    } else {
                        ((min_distance / denom) as f32).clamp(0.0, 1.0)
                    }
                }
            }
        }
    }
}
