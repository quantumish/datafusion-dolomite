use crate::cascades::memo::Memo;
use crate::cascades::task::{schedule, OptimizeGroup};
use crate::cascades::{Group, GroupExpr, GroupExprId, GroupId};

use crate::cost::{CostModel, INF};
use crate::error::DolomiteResult;

use crate::optimizer::{Optimizer, OptimizerContext};
use crate::plan::Plan;
use crate::properties::PhysicalPropertySet;
use crate::rules::RuleImpl;

pub struct CascadesOptimizer {
    pub required_prop: PhysicalPropertySet,
    pub rules: Vec<RuleImpl>,
    pub memo: Memo,
    pub(super) context: OptimizerContext,
    pub(super) cost_model: CostModel,
}

impl Optimizer for CascadesOptimizer {
    type GroupHandle = GroupId;
    type ExprHandle = GroupExprId;
    type Group = Group;
    type Expr = GroupExpr;

    fn context(&self) -> &OptimizerContext {
        &self.context
    }

    fn group_at(&self, group_handle: GroupId) -> &Group {
        &self.memo[group_handle]
    }

    fn expr_at(&self, expr_handle: GroupExprId) -> &GroupExpr {
        &self.memo[expr_handle]
    }

    fn find_best_plan(&mut self) -> DolomiteResult<Plan> {
        let root_task = OptimizeGroup::new(
            self.memo.root_group_id(),
            self.required_prop.clone(),
            INF,
        )
        .into();

        schedule(self, root_task)?;

        self.memo.best_plan(&self.required_prop)
    }
}

impl CascadesOptimizer {
    pub fn new(
        required_prop: PhysicalPropertySet,
        rules: Vec<RuleImpl>,
        plan: Plan,
        context: OptimizerContext,
        cost_model: CostModel,
    ) -> Self {
        Self {
            required_prop,
            rules,
            memo: Memo::from(plan),
            context,
            cost_model,
        }
    }

    pub fn default(plan: Plan) -> Self {
        Self {
            required_prop: PhysicalPropertySet::default(),
            rules: vec![],
            memo: Memo::from(plan),
            context: OptimizerContext::default(),
            cost_model: CostModel::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cascades::CascadesOptimizer;

    use crate::cost::CostModel;
    use crate::optimizer::{Optimizer, OptimizerContext};
    use crate::plan::{LogicalPlanBuilder, PhysicalPlanBuilder};
    use crate::properties::PhysicalPropertySet;
    use crate::rules::{CommutateJoinRule, Join2HashJoinRule, Scan2TableScanRule};
    use datafusion::logical_expr::{binary_expr, col};
    use datafusion_expr::logical_plan::JoinType;
    use datafusion_expr::Operator::Eq;

    #[test]
    fn test_optimize_join() {
        let plan = {
            let mut builder = LogicalPlanBuilder::new();
            let right = builder.scan(None, "t2").build().root();
            builder
                .scan(None, "t1")
                .join(
                    JoinType::Inner,
                    binary_expr(col("t1.c1"), Eq, col("t2.c2")),
                    right,
                )
                .build()
        };

        let optimizer = CascadesOptimizer::new(
            PhysicalPropertySet::default(),
            vec![
                CommutateJoinRule::new().into(),
                Join2HashJoinRule::new().into(),
                Scan2TableScanRule::new().into(),
            ],
            plan,
            OptimizerContext::default(),
            CostModel::default(),
        );

        let expected_plan = {
            let right = PhysicalPlanBuilder::scan(None, "t2").build().root();

            PhysicalPlanBuilder::scan(None, "t1")
                .hash_join(
                    JoinType::Inner,
                    binary_expr(col("t1.c1"), Eq, col("t2.c2")),
                    right,
                )
                .build()
        };

        assert_eq!(expected_plan, optimizer.find_best_plan().unwrap());
    }
}
