//! Defines cost model.

use crate::error::DolomiteResult;
use crate::optimizer::Optimizer;

mod trivial;
pub use trivial::*;

use derive_more::{Add, AddAssign, Sub, SubAssign, Sum};

pub const INF: Cost = Cost(f64::INFINITY);

#[derive(
    Copy, Clone, Debug, PartialOrd, PartialEq, Add, Sub, Sum, AddAssign, SubAssign,
)]
pub struct Cost(pub f64);

impl From<f64> for Cost {
    fn from(c: f64) -> Self {
        Cost(c)
    }
}

#[derive(Default)]
pub struct CostModel {
    /// Actual strategy.
    inner: SimpleCostModel,
}

impl CostModel {
    /// Estimate cost of current operator without accumulating children's cost.
    pub fn estimate_cost<O: Optimizer>(&self, expr: &O::Expr) -> DolomiteResult<Cost> {
        self.inner.cost::<O>(expr)
    }
}
