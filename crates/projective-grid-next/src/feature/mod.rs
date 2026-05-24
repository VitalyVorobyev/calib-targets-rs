//! Input types: [`Observation<F>`] (a 2D feature with axes + tag + score)
//! and [`AxisEstimate<F>`] (an undirected local grid direction).

pub mod axis;
pub mod observation;

pub use axis::AxisEstimate;
pub use observation::Observation;
