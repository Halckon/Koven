//! 夹具的同步调用驱动；只执行具备 Pass/Bind 的已知 body，其余调用保持 opaque。
use super::*;
use crate::{
    name_resolution::NameResolution,
    parser::{Expression, Item, NameMarker, ParsedFile, Statement},
};

impl Replay {
    pub(super) fn call(
        &mut self,
        facts: &DropPlan,
        parsed: &ParsedFile,
        names: &NameResolution,
        expected_bodies: &[(ExpressionId, ExpressionId)],
        call: ExpressionId,
    ) {
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        assert!(arguments.is_empty(), "this fixture has no call operands");
        self.point(facts, DropPoint::AfterExpression(*callee));
        self.point(facts, DropPoint::CallEntry(call));
        if let Some(closure) = expected_bodies
            .iter()
            .find_map(|&(at, body)| (at == call).then_some(body))
        {
            // expected_bodies 只指定实际执行路径；环境必须由事实传入，不能按夹具补写 owner。
            self.point(facts, DropPoint::LambdaEntry(closure));
            let Expression::Lambda { body, .. } =
                parsed.ast().expressions().get(closure).unwrap().payload()
            else {
                unreachable!()
            };
            let Statement::LambdaBody { elements } =
                parsed.ast().statements().get(*body).unwrap().payload()
            else {
                unreachable!()
            };
            for &statement in elements {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(statement).unwrap().payload()
                else {
                    unreachable!()
                };
                let Item::Variable {
                    initializer,
                    name: NameMarker::Present(span),
                    ..
                } = parsed.ast().items().get(*declaration).unwrap().payload()
                else {
                    unreachable!()
                };
                match parsed
                    .ast()
                    .expressions()
                    .get(*initializer)
                    .unwrap()
                    .payload()
                {
                    Expression::Lambda { .. } => {
                        self.point(facts, DropPoint::AfterExpression(*initializer));
                        let symbol = names
                            .symbols()
                            .iter()
                            .find(|symbol| symbol.span() == *span)
                            .unwrap()
                            .id();
                        if let Some(owner) = self.bindings.get(&symbol) {
                            assert_eq!(Some(*owner), self.result);
                        } else {
                            self.bind(symbol, self.result.unwrap());
                        }
                    }
                    Expression::Call { .. } => {
                        self.call(facts, parsed, names, expected_bodies, *initializer)
                    }
                    // 本夹具的 Copyable 读取没有新 owner；仍执行读取后的公开清理点。
                    Expression::Name => self.point(facts, DropPoint::AfterExpression(*initializer)),
                    other => panic!("unsupported fixture body expression: {other:?}"),
                }
                self.point(facts, DropPoint::AfterStatement(statement));
            }
            self.point(facts, DropPoint::AfterStatement(*body));
            self.leave_environment();
        } else {
            assert!(
                self.pending_environment.is_none(),
                "a passed environment requires body execution"
            );
        }
        self.point(facts, DropPoint::CallReturn(call));
        self.point(facts, DropPoint::AfterExpression(call));
    }
}
