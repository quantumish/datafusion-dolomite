use crate::conversion::expr_to_df_join_condition;
use anyhow::bail;
use datafusion::datasource::empty::EmptyTable;

use datafusion::logical_expr::LogicalPlan;
use datafusion::scalar::ScalarValue;
use datafusion_expr::Expr as DFExpr;
use datafusion_expr::logical_plan::JoinConstraint;
use datafusion_expr::logical_plan::{
    Join as DFJoin, Limit as DFLimit, Projection as DFProjection,
    TableScan as DFTableScan,
};

use datafusion::datasource::DefaultTableSource;

use dolomite::error::DolomiteResult;
use dolomite::operator::LogicalOperator::{
    LogicalJoin, LogicalLimit, LogicalProjection, LogicalScan,
};
use dolomite::operator::Operator::Logical;

use dolomite::operator::{Filter, Limit, LogicalOperator, Projection, TableScan};

use dolomite::plan::{Plan, PlanNode, PlanNodeIdGen};

use datafusion_sql::TableReference;
use std::sync::Arc;

/// Convert dolomite logical plan to datafusion logical plan.
pub fn to_df_logical(plan: &Plan) -> DolomiteResult<LogicalPlan> {
    plan_node_to_df_logical_plan(&plan.root())
}

/// Convert datafusion logical plan to dolomite logical plan.
pub fn from_df_logical(df_plan: &LogicalPlan) -> DolomiteResult<Plan> {
    let mut plan_node_id_gen = PlanNodeIdGen::new();
    let root = df_logical_plan_to_plan_node(df_plan, &mut plan_node_id_gen)?;
    Ok(Plan::new(Arc::new(root)))
}

fn plan_node_to_df_logical_plan(plan_node: &PlanNode) -> DolomiteResult<LogicalPlan> {
    let mut inputs = plan_node
        .inputs()
        .iter()
        .map(|p| plan_node_to_df_logical_plan(p))
        .collect::<DolomiteResult<Vec<LogicalPlan>>>()?;

    match plan_node.operator() {
        Logical(LogicalProjection(projection)) => {
            let df_projection = DFProjection::try_new_with_schema(
                Vec::from(projection.expr()),
                Arc::new(inputs.remove(0)),
                Arc::new(plan_node.logical_prop().unwrap().schema().clone()),
            )
            .unwrap();

            Ok(LogicalPlan::Projection(df_projection))
        }
        Logical(LogicalLimit(limit)) => {
            let df_limit = DFLimit {
                skip: None,
                fetch: Some(Box::new(DFExpr::Literal(
					ScalarValue::Int64(Some(limit.limit() as i64))
				))),
                input: Arc::new(inputs.remove(0)),
            };

            Ok(LogicalPlan::Limit(df_limit))
        }
        Logical(LogicalJoin(join)) => {
            let df_join = DFJoin {
                left: Arc::new(inputs.remove(0)),
                right: Arc::new(inputs.remove(0)),
                on: expr_to_df_join_condition(join.expr())?,
                filter: None,
                join_type: join.join_type(),
                join_constraint: JoinConstraint::On,
                schema: Arc::new(plan_node.logical_prop().unwrap().schema().clone()),
                null_equals_null: true,
            };

            Ok(LogicalPlan::Join(df_join))
        }
        Logical(LogicalScan(scan)) => {
            let schema = Arc::new(plan_node.logical_prop().unwrap().schema().clone());
            let source = Arc::new(DefaultTableSource::new(Arc::new(EmptyTable::new(
                Arc::new((*schema).clone().into()),
            ))));
            let df_scan = DFTableScan {
                table_name: TableReference::from(scan.table_name()),
                source,
                projection: None,
                projected_schema: schema,
                filters: vec![],
                fetch: scan.limit(),
            };

            Ok(LogicalPlan::TableScan(df_scan))
        }
        op => bail!("Can't convert plan to data fusion logical plan: {:?}", op),
    }
}

fn df_logical_plan_to_plan_node(
    df_plan: &LogicalPlan,
    id_gen: &mut PlanNodeIdGen,
) -> DolomiteResult<PlanNode> {
    let id = id_gen.gen_next();
    let (operator, inputs) = match df_plan {
        LogicalPlan::Projection(projection) => {
            let operator = LogicalOperator::LogicalProjection(Projection::new(
                projection.expr.clone(),
            ));
            let inputs = vec![df_logical_plan_to_plan_node(&projection.input, id_gen)?];
            (operator, inputs)
        }
        LogicalPlan::Limit(limit) => {
			let DFExpr::Literal(ScalarValue::Int64(Some(l))) = *limit.fetch.as_ref().unwrap().as_ref() else {
				panic!("got complicated limit clause");
			};
            let operator =
                LogicalOperator::LogicalLimit(Limit::new(l as usize));
            let inputs = vec![df_logical_plan_to_plan_node(&limit.input, id_gen)?];
            (operator, inputs)
        }
        // LogicalPlan::Join(join) => {
        //     let join_cond = join
        //         .on
        //         .iter()
        //         .map(|(left, right)| {
        //             ExprColumn(left.clone()).eq(ExprColumn(right.clone()))
        //         })
        //         .reduce(and)
        //         .unwrap_or(Expr::Literal(ScalarValue::Boolean(Some(true))));
        //     let operator =
        //         LogicalOperator::LogicalJoin(Join::new(join.join_type, join_cond));
        //     let inputs = vec![
        //         df_logical_plan_to_plan_node(&join.left, id_gen)?,
        //         df_logical_plan_to_plan_node(&join.right, id_gen)?,
        //     ];
        //     (operator, inputs)
        // }
        LogicalPlan::TableScan(scan) => {
            let operator = LogicalOperator::LogicalScan(TableScan::new(
                scan.table_name.table().to_string(),
            ));
            let inputs = vec![];
            (operator, inputs)
        }
		LogicalPlan::Filter(filter) => {
            let operator = LogicalOperator::LogicalFilter(Filter::new(
                filter.predicate.clone(),
				vec![] // FIXME(quantumish) this may be a questionable default
            ));
            let inputs = vec![df_logical_plan_to_plan_node(&filter.input, id_gen)?];
            (operator, inputs)
        }
        plan => {
            bail!("Unsupported datafusion logical plan: {:?}", plan);
        }
    };

    Ok(PlanNode::new(
        id,
        Logical(operator),
        inputs.into_iter().map(Arc::new).collect(),
    ))
}
