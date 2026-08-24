use std::{cmp::Reverse, collections::BTreeMap, collections::BinaryHeap};

use thiserror::Error;

use crate::{
    AnyExpression, AnyNode, Citation, CitationId, ExpressionError, FindingCondition, NodeId,
    Operand, ValueType,
};

/// A labelled causal dependency generated from an expression or finding operand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edge {
    source: NodeId,
    target: NodeId,
    role: String,
}

impl Edge {
    fn from_operand(target: NodeId, operand: &Operand) -> Self {
        Self {
            source: operand.node_id().clone(),
            target,
            role: operand.role().to_owned(),
        }
    }

    pub fn source(&self) -> &NodeId {
        &self.source
    }

    pub fn target(&self) -> &NodeId {
        &self.target
    }

    pub fn role(&self) -> &str {
        &self.role
    }
}

/// An unvalidated calculation graph assembled by a client profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphDefinition {
    nodes: Vec<AnyNode>,
    citations: Vec<Citation>,
    outputs: Vec<NodeId>,
}

impl GraphDefinition {
    pub fn new(nodes: Vec<AnyNode>, citations: Vec<Citation>, outputs: Vec<NodeId>) -> Self {
        Self {
            nodes,
            citations,
            outputs,
        }
    }

    pub fn nodes(&self) -> &[AnyNode] {
        &self.nodes
    }

    pub fn citations(&self) -> &[Citation] {
        &self.citations
    }

    pub fn outputs(&self) -> &[NodeId] {
        &self.outputs
    }

    /// Validates this definition in the deterministic phase order documented by the core design.
    pub fn validate(self) -> Result<ValidatedGraph, GraphValidationError> {
        validate_local_validity(&self)?;

        let node_index = build_node_index(&self.nodes)?;
        let citation_index = build_citation_index(&self.citations)?;
        validate_outputs_and_citations(&self, &node_index, &citation_index)?;
        validate_operand_references(&self.nodes, &node_index)?;

        let edges = generate_edges(&self.nodes);
        let dependency_indices = build_dependency_indices(&self.nodes, &node_index);
        validate_acyclic(&self.nodes, &dependency_indices)?;

        let topological_indices = topological_sort(&self.nodes, &node_index, &edges);
        let resolved_types = resolve_types(&self.nodes, &topological_indices)?;
        validate_unique_setting_keys(&self.nodes)?;
        validate_reachability(&self.nodes, &self.outputs, &node_index, &dependency_indices)?;

        Ok(ValidatedGraph::new(
            self,
            node_index,
            citation_index,
            edges,
            topological_indices,
            resolved_types,
        ))
    }
}

/// A structurally valid graph with immutable lookup and traversal indexes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedGraph {
    nodes: Vec<AnyNode>,
    citations: Vec<Citation>,
    outputs: Vec<NodeId>,
    node_index: BTreeMap<NodeId, usize>,
    citation_index: BTreeMap<CitationId, usize>,
    edges: Vec<Edge>,
    dependencies: BTreeMap<NodeId, Vec<Edge>>,
    dependents: BTreeMap<NodeId, Vec<Edge>>,
    topological_order: Vec<NodeId>,
    resolved_types: BTreeMap<NodeId, ValueType>,
}

impl ValidatedGraph {
    fn new(
        definition: GraphDefinition,
        node_index: BTreeMap<NodeId, usize>,
        citation_index: BTreeMap<CitationId, usize>,
        edges: Vec<Edge>,
        topological_indices: Vec<usize>,
        resolved_types: BTreeMap<NodeId, ValueType>,
    ) -> Self {
        let mut dependencies = definition
            .nodes
            .iter()
            .map(|node| (node.id().clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let mut dependents = dependencies.clone();
        for edge in &edges {
            dependencies
                .get_mut(edge.target())
                .expect("validated edge target must be indexed")
                .push(edge.clone());
            dependents
                .get_mut(edge.source())
                .expect("validated edge source must be indexed")
                .push(edge.clone());
        }

        let topological_order = topological_indices
            .into_iter()
            .map(|index| definition.nodes[index].id().clone())
            .collect();

        Self {
            nodes: definition.nodes,
            citations: definition.citations,
            outputs: definition.outputs,
            node_index,
            citation_index,
            edges,
            dependencies,
            dependents,
            topological_order,
            resolved_types,
        }
    }

    pub fn nodes(&self) -> &[AnyNode] {
        &self.nodes
    }

    pub fn citations(&self) -> &[Citation] {
        &self.citations
    }

    pub fn outputs(&self) -> &[NodeId] {
        &self.outputs
    }

    pub fn node(&self, id: &NodeId) -> Option<&AnyNode> {
        self.node_index.get(id).map(|index| &self.nodes[*index])
    }

    pub fn citation(&self, id: &CitationId) -> Option<&Citation> {
        self.citation_index
            .get(id)
            .map(|index| &self.citations[*index])
    }

    /// Returns all generated edges in target declaration and operand insertion order.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Returns the incoming causal edges for a known node.
    pub fn dependencies(&self, id: &NodeId) -> Option<&[Edge]> {
        self.dependencies.get(id).map(Vec::as_slice)
    }

    /// Returns the outgoing causal edges for a known node.
    pub fn dependents(&self, id: &NodeId) -> Option<&[Edge]> {
        self.dependents.get(id).map(Vec::as_slice)
    }

    pub fn topological_order(&self) -> &[NodeId] {
        &self.topological_order
    }

    /// Returns a value node's statically resolved type. Findings do not produce values.
    pub fn resolved_type(&self, id: &NodeId) -> Option<ValueType> {
        self.resolved_types.get(id).copied()
    }
}

/// The first deterministic structural defect found in a graph definition.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum GraphValidationError {
    #[error("graph must declare at least one output")]
    NoOutputs,
    #[error("setting node `{node_id}` must define a non-empty configuration key")]
    EmptySettingKey { node_id: NodeId },
    #[error("node `{node_id}` operand {operand} must define a non-empty role")]
    EmptyOperandRole { node_id: NodeId, operand: usize },
    #[error(
        "node ID `{node_id}` is duplicated at declaration {duplicate_index}; first declared at {first_index}"
    )]
    DuplicateNodeId {
        node_id: NodeId,
        first_index: usize,
        duplicate_index: usize,
    },
    #[error(
        "citation ID `{citation_id}` is duplicated at declaration {duplicate_index}; first declared at {first_index}"
    )]
    DuplicateCitationId {
        citation_id: CitationId,
        first_index: usize,
        duplicate_index: usize,
    },
    #[error("output `{output_id}` is declared more than once")]
    DuplicateOutput { output_id: NodeId },
    #[error("output references unknown node `{output_id}`")]
    UnknownOutput { output_id: NodeId },
    #[error("node `{output_id}` of type {node_type} cannot be a public output")]
    InvalidOutputNodeType {
        output_id: NodeId,
        node_type: &'static str,
    },
    #[error("node `{node_id}` citation claim {claim} references unknown citation `{citation_id}`")]
    UnknownCitation {
        node_id: NodeId,
        claim: usize,
        citation_id: CitationId,
    },
    #[error("node `{node_id}` operand {operand} references unknown node `{referenced_node_id}`")]
    UnknownOperand {
        node_id: NodeId,
        operand: usize,
        referenced_node_id: NodeId,
    },
    #[error(
        "node `{node_id}` operand {operand} references finding `{finding_id}`, which does not produce a value"
    )]
    FindingOperand {
        node_id: NodeId,
        operand: usize,
        finding_id: NodeId,
    },
    #[error("graph contains dependency cycle {path:?}")]
    Cycle { path: Vec<NodeId> },
    #[error("node `{node_id}` has an invalid expression type contract: {source}")]
    ExpressionType {
        node_id: NodeId,
        #[source]
        source: ExpressionError,
    },
    #[error("finding `{node_id}` compares incompatible value types {left:?} and {right:?}")]
    FindingComparisonTypeMismatch {
        node_id: NodeId,
        left: ValueType,
        right: ValueType,
    },
    #[error("setting key `{key}` on node `{duplicate_node_id}` duplicates node `{first_node_id}`")]
    DuplicateSettingKey {
        key: String,
        first_node_id: NodeId,
        duplicate_node_id: NodeId,
    },
    #[error("node `{node_id}` does not contribute to any declared output")]
    UnreachableNode { node_id: NodeId },
}

fn validate_local_validity(definition: &GraphDefinition) -> Result<(), GraphValidationError> {
    if definition.outputs.is_empty() {
        return Err(GraphValidationError::NoOutputs);
    }

    for node in &definition.nodes {
        if let AnyNode::Setting(setting) = node
            && setting.node_type().key().trim().is_empty()
        {
            return Err(GraphValidationError::EmptySettingKey {
                node_id: node.id().clone(),
            });
        }

        for (operand, value) in operands(node).enumerate() {
            if value.role().trim().is_empty() {
                return Err(GraphValidationError::EmptyOperandRole {
                    node_id: node.id().clone(),
                    operand,
                });
            }
        }
    }

    Ok(())
}

fn build_node_index(nodes: &[AnyNode]) -> Result<BTreeMap<NodeId, usize>, GraphValidationError> {
    let mut index = BTreeMap::new();
    for (duplicate_index, node) in nodes.iter().enumerate() {
        if let Some(first_index) = index.insert(node.id().clone(), duplicate_index) {
            return Err(GraphValidationError::DuplicateNodeId {
                node_id: node.id().clone(),
                first_index,
                duplicate_index,
            });
        }
    }
    Ok(index)
}

fn build_citation_index(
    citations: &[Citation],
) -> Result<BTreeMap<CitationId, usize>, GraphValidationError> {
    let mut index = BTreeMap::new();
    for (duplicate_index, citation) in citations.iter().enumerate() {
        if let Some(first_index) = index.insert(citation.id().clone(), duplicate_index) {
            return Err(GraphValidationError::DuplicateCitationId {
                citation_id: citation.id().clone(),
                first_index,
                duplicate_index,
            });
        }
    }
    Ok(index)
}

fn validate_outputs_and_citations(
    definition: &GraphDefinition,
    node_index: &BTreeMap<NodeId, usize>,
    citation_index: &BTreeMap<CitationId, usize>,
) -> Result<(), GraphValidationError> {
    let mut output_index = BTreeMap::new();
    for output_id in &definition.outputs {
        if output_index.insert(output_id, ()).is_some() {
            return Err(GraphValidationError::DuplicateOutput {
                output_id: output_id.clone(),
            });
        }
        let Some(index) = node_index.get(output_id) else {
            return Err(GraphValidationError::UnknownOutput {
                output_id: output_id.clone(),
            });
        };
        let node_type = match &definition.nodes[*index] {
            AnyNode::Input(_) => Some("input"),
            AnyNode::Constant(_) => Some("constant"),
            AnyNode::Derived(_) | AnyNode::Setting(_) | AnyNode::Finding(_) => None,
        };
        if let Some(node_type) = node_type {
            return Err(GraphValidationError::InvalidOutputNodeType {
                output_id: output_id.clone(),
                node_type,
            });
        }
    }

    for node in &definition.nodes {
        for (claim, citation_claim) in node.metadata().citation_claims().iter().enumerate() {
            if !citation_index.contains_key(citation_claim.citation_id()) {
                return Err(GraphValidationError::UnknownCitation {
                    node_id: node.id().clone(),
                    claim,
                    citation_id: citation_claim.citation_id().clone(),
                });
            }
        }
    }

    Ok(())
}

fn validate_operand_references(
    nodes: &[AnyNode],
    node_index: &BTreeMap<NodeId, usize>,
) -> Result<(), GraphValidationError> {
    for node in nodes {
        for (operand, value) in operands(node).enumerate() {
            let Some(source_index) = node_index.get(value.node_id()) else {
                return Err(GraphValidationError::UnknownOperand {
                    node_id: node.id().clone(),
                    operand,
                    referenced_node_id: value.node_id().clone(),
                });
            };
            if matches!(nodes[*source_index], AnyNode::Finding(_)) {
                return Err(GraphValidationError::FindingOperand {
                    node_id: node.id().clone(),
                    operand,
                    finding_id: value.node_id().clone(),
                });
            }
        }
    }
    Ok(())
}

fn generate_edges(nodes: &[AnyNode]) -> Vec<Edge> {
    nodes
        .iter()
        .flat_map(|node| {
            operands(node).map(|operand| Edge::from_operand(node.id().clone(), operand))
        })
        .collect()
}

fn build_dependency_indices(
    nodes: &[AnyNode],
    node_index: &BTreeMap<NodeId, usize>,
) -> Vec<Vec<usize>> {
    nodes
        .iter()
        .map(|node| {
            operands(node)
                .map(|operand| {
                    *node_index
                        .get(operand.node_id())
                        .expect("operand references were validated")
                })
                .collect()
        })
        .collect()
}

fn validate_acyclic(
    nodes: &[AnyNode],
    dependencies: &[Vec<usize>],
) -> Result<(), GraphValidationError> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Unvisited,
        Visiting,
        Visited,
    }

    fn visit(
        index: usize,
        nodes: &[AnyNode],
        dependencies: &[Vec<usize>],
        states: &mut [State],
        stack: &mut Vec<usize>,
    ) -> Result<(), GraphValidationError> {
        states[index] = State::Visiting;
        stack.push(index);

        for dependency in dependencies[index].iter().copied() {
            match states[dependency] {
                State::Unvisited => visit(dependency, nodes, dependencies, states, stack)?,
                State::Visiting => {
                    let start = stack
                        .iter()
                        .position(|candidate| *candidate == dependency)
                        .expect("visiting dependency must be on DFS stack");
                    let mut path = stack[start..]
                        .iter()
                        .map(|node_index| nodes[*node_index].id().clone())
                        .collect::<Vec<_>>();
                    path.push(nodes[dependency].id().clone());
                    return Err(GraphValidationError::Cycle { path });
                }
                State::Visited => {}
            }
        }

        stack.pop();
        states[index] = State::Visited;
        Ok(())
    }

    let mut states = vec![State::Unvisited; nodes.len()];
    let mut stack = Vec::new();
    for index in 0..nodes.len() {
        if states[index] == State::Unvisited {
            visit(index, nodes, dependencies, &mut states, &mut stack)?;
        }
    }
    Ok(())
}

fn topological_sort(
    nodes: &[AnyNode],
    node_index: &BTreeMap<NodeId, usize>,
    edges: &[Edge],
) -> Vec<usize> {
    let mut in_degree = vec![0_usize; nodes.len()];
    let mut dependents = vec![Vec::new(); nodes.len()];
    for edge in edges {
        let source = node_index[edge.source()];
        let target = node_index[edge.target()];
        in_degree[target] += 1;
        dependents[source].push(target);
    }

    let mut ready = in_degree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(Reverse(index)))
        .collect::<BinaryHeap<_>>();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(Reverse(index)) = ready.pop() {
        order.push(index);
        for dependent in dependents[index].iter().copied() {
            in_degree[dependent] -= 1;
            if in_degree[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }

    debug_assert_eq!(order.len(), nodes.len(), "cycles were validated first");
    order
}

fn resolve_types(
    nodes: &[AnyNode],
    topological_order: &[usize],
) -> Result<BTreeMap<NodeId, ValueType>, GraphValidationError> {
    let mut resolved = BTreeMap::new();
    for index in topological_order.iter().copied() {
        let node = &nodes[index];
        let result_type = match node {
            AnyNode::Input(input) => Some(input.node_type().value_type()),
            AnyNode::Constant(constant) => Some(constant.node_type().value().value_type()),
            AnyNode::Derived(derived) => Some(resolve_expression_type(
                node.id(),
                derived.node_type().expression(),
                &resolved,
            )?),
            AnyNode::Setting(setting) => Some(resolve_expression_type(
                node.id(),
                setting.node_type().expression(),
                &resolved,
            )?),
            AnyNode::Finding(finding) => {
                if let FindingCondition::Comparison(comparison) = finding.node_type().condition() {
                    let left = resolved[comparison.left().node_id()];
                    let right = resolved[comparison.right().node_id()];
                    if left != right {
                        return Err(GraphValidationError::FindingComparisonTypeMismatch {
                            node_id: node.id().clone(),
                            left,
                            right,
                        });
                    }
                }
                None
            }
        };
        if let Some(result_type) = result_type {
            resolved.insert(node.id().clone(), result_type);
        }
    }
    Ok(resolved)
}

fn resolve_expression_type(
    node_id: &NodeId,
    expression: &AnyExpression,
    resolved: &BTreeMap<NodeId, ValueType>,
) -> Result<ValueType, GraphValidationError> {
    let operand_types = expression
        .operands()
        .iter()
        .map(|operand| resolved[operand.node_id()])
        .collect::<Vec<_>>();
    expression
        .result_type(&operand_types)
        .map_err(|source| GraphValidationError::ExpressionType {
            node_id: node_id.clone(),
            source,
        })
}

fn validate_unique_setting_keys(nodes: &[AnyNode]) -> Result<(), GraphValidationError> {
    let mut settings = BTreeMap::<&str, &NodeId>::new();
    for node in nodes {
        if let AnyNode::Setting(setting) = node {
            let key = setting.node_type().key();
            if let Some(first_node_id) = settings.insert(key, node.id()) {
                return Err(GraphValidationError::DuplicateSettingKey {
                    key: key.to_owned(),
                    first_node_id: first_node_id.clone(),
                    duplicate_node_id: node.id().clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_reachability(
    nodes: &[AnyNode],
    outputs: &[NodeId],
    node_index: &BTreeMap<NodeId, usize>,
    dependencies: &[Vec<usize>],
) -> Result<(), GraphValidationError> {
    let mut reachable = vec![false; nodes.len()];
    let mut pending = outputs
        .iter()
        .map(|output| node_index[output])
        .collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        if std::mem::replace(&mut reachable[index], true) {
            continue;
        }
        pending.extend(dependencies[index].iter().copied());
    }

    if let Some(node) = nodes
        .iter()
        .zip(reachable)
        .find_map(|(node, reachable)| (!reachable).then_some(node))
    {
        return Err(GraphValidationError::UnreachableNode {
            node_id: node.id().clone(),
        });
    }
    Ok(())
}

fn operands(node: &AnyNode) -> impl Iterator<Item = &Operand> {
    let operands = match node {
        AnyNode::Input(_) | AnyNode::Constant(_) => &[][..],
        AnyNode::Derived(derived) => derived.node_type().expression().operands(),
        AnyNode::Setting(setting) => setting.node_type().expression().operands(),
        AnyNode::Finding(finding) => match finding.node_type().condition() {
            FindingCondition::Always => &[],
            FindingCondition::Comparison(comparison) => {
                // Comparisons have fixed arity, but their operands are stored separately.
                return EitherOperands::Pair([comparison.left(), comparison.right()].into_iter());
            }
        },
    };
    EitherOperands::Slice(operands.iter())
}

enum EitherOperands<'a> {
    Slice(std::slice::Iter<'a, Operand>),
    Pair(std::array::IntoIter<&'a Operand, 2>),
}

impl<'a> Iterator for EitherOperands<'a> {
    type Item = &'a Operand;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Slice(iter) => iter.next(),
            Self::Pair(iter) => iter.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{
        Add, CitationClaim, Comparison, ComparisonOperator, Constant, ConstantOrigin, Derived,
        Expression, Finding, FindingSeverity, Input, Node, NodeIdSuffix, NodeMetadata, Reference,
        Setting, SettingScope, SettingUnit, Value,
    };

    fn id(value: &str) -> NodeId {
        NodeId::new(value).expect("test node ID should be valid")
    }

    fn suffix(value: &str) -> NodeIdSuffix {
        NodeIdSuffix::new(value).expect("test node suffix should be valid")
    }

    fn metadata() -> NodeMetadata {
        NodeMetadata::new(String::from("Label"), String::from("Description"), vec![])
    }

    fn operand(node_id: &str, role: &str) -> Operand {
        Operand::new(id(node_id), role.to_owned())
    }

    fn input(name: &str, value_type: ValueType) -> AnyNode {
        Node::new(
            suffix(name),
            metadata(),
            Input::new(value_type, None, vec![]).expect("test input should be valid"),
        )
        .into()
    }

    fn constant(name: &str, value: Value) -> AnyNode {
        Node::new(
            suffix(name),
            metadata(),
            Constant::new(
                value,
                ConstantOrigin::CalculatorPolicy,
                String::from("Test policy"),
            ),
        )
        .into()
    }

    fn derived(name: &str, expression: impl Into<AnyExpression>) -> AnyNode {
        Node::new(suffix(name), metadata(), Derived::new(expression.into())).into()
    }

    fn setting(name: &str, key: &str, expression: impl Into<AnyExpression>) -> AnyNode {
        Node::new(
            suffix(name),
            metadata(),
            Setting::new(
                key.to_owned(),
                SettingScope::Producer,
                SettingUnit::Messages,
                expression.into(),
            ),
        )
        .into()
    }

    fn reference(source: &str) -> Expression<Reference> {
        Expression::<Reference>::new(operand(source, "source value"))
    }

    fn valid_definition() -> GraphDefinition {
        GraphDefinition::new(
            vec![
                input("source", ValueType::MessageCount),
                derived("copied", reference("input.source")),
                setting(
                    "result",
                    "queue.buffering.max.messages",
                    reference("derived.copied"),
                ),
            ],
            vec![],
            vec![id("setting.result")],
        )
    }

    #[googletest::test]
    fn validates_indexes_types_edges_and_stable_topological_order() {
        let graph = valid_definition()
            .validate()
            .expect("valid graph should pass validation");

        assert_that!(graph.nodes().len(), eq(3));
        assert_that!(graph.node(&id("derived.copied")).is_some(), eq(true));
        assert_that!(graph.node(&id("derived.missing")).is_none(), eq(true));
        assert_that!(
            graph.resolved_type(&id("setting.result")),
            eq(Some(ValueType::MessageCount))
        );
        assert_that!(
            graph.topological_order(),
            eq([
                id("input.source"),
                id("derived.copied"),
                id("setting.result"),
            ])
        );
        assert_that!(graph.edges().len(), eq(2));
        assert_that!(
            graph
                .dependencies(&id("setting.result"))
                .map(|edges| edges.len()),
            eq(Some(1))
        );
        assert_that!(
            graph
                .dependents(&id("input.source"))
                .map(|edges| edges.len()),
            eq(Some(1))
        );
    }

    #[googletest::test]
    fn retains_duplicate_labelled_expression_edges() {
        let repeated = operand("input.source", "repeated factor");
        let graph = GraphDefinition::new(
            vec![
                input("source", ValueType::Scalar),
                derived("sum", Expression::<Add>::new(repeated.clone(), repeated)),
            ],
            vec![],
            vec![id("derived.sum")],
        )
        .validate()
        .expect("repeated operands should be valid");

        assert_that!(graph.edges().len(), eq(2));
        for edge in graph.edges() {
            assert_that!(edge.source(), eq(&id("input.source")));
            assert_that!(edge.target(), eq(&id("derived.sum")));
            assert_that!(edge.role(), eq("repeated factor"));
        }
        assert_that!(
            graph
                .dependencies(&id("derived.sum"))
                .map(|edges| edges.len()),
            eq(Some(2))
        );
        assert_that!(
            graph
                .dependents(&id("input.source"))
                .map(|edges| edges.len()),
            eq(Some(2))
        );
    }

    #[googletest::test]
    fn preserves_declaration_order_between_independent_nodes() {
        let graph = GraphDefinition::new(
            vec![
                input("second", ValueType::Scalar),
                input("first", ValueType::Scalar),
                derived(
                    "sum",
                    Expression::<Add>::new(
                        operand("input.first", "first term"),
                        operand("input.second", "second term"),
                    ),
                ),
            ],
            vec![],
            vec![id("derived.sum")],
        )
        .validate()
        .expect("valid graph should pass validation");

        assert_that!(
            graph.topological_order(),
            eq([id("input.second"), id("input.first"), id("derived.sum"),])
        );
    }

    #[googletest::test]
    fn local_validity_is_checked_before_global_references() {
        let graph = GraphDefinition::new(
            vec![setting(
                "result",
                "",
                Expression::<Reference>::new(operand("input.missing", "")),
            )],
            vec![],
            vec![],
        );

        assert_that!(graph.validate(), err(eq(&GraphValidationError::NoOutputs)));

        let graph = GraphDefinition::new(
            vec![setting(
                "result",
                "",
                Expression::<Reference>::new(operand("input.missing", "")),
            )],
            vec![],
            vec![id("setting.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::EmptySettingKey {
                node_id: id("setting.result"),
            }))
        );

        let graph = GraphDefinition::new(
            vec![derived(
                "result",
                Expression::<Reference>::new(operand("input.missing", "  ")),
            )],
            vec![],
            vec![id("derived.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::EmptyOperandRole {
                node_id: id("derived.result"),
                operand: 0,
            }))
        );
    }

    #[googletest::test]
    fn rejects_duplicate_node_and_citation_ids() {
        let duplicate = input("source", ValueType::Scalar);
        let graph = GraphDefinition::new(
            vec![duplicate.clone(), duplicate],
            vec![],
            vec![id("derived.output")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::DuplicateNodeId {
                node_id: id("input.source"),
                first_index: 0,
                duplicate_index: 1,
            }))
        );

        let citation_id = CitationId::new("source.docs").expect("citation ID should be valid");
        let citation = Citation::new(
            citation_id.clone(),
            String::from("Docs"),
            String::from("https://example.com"),
            String::from("Summary"),
        );
        let graph = GraphDefinition::new(
            valid_definition().nodes,
            vec![citation.clone(), citation],
            vec![id("setting.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::DuplicateCitationId {
                citation_id,
                first_index: 0,
                duplicate_index: 1,
            }))
        );
    }

    #[googletest::test]
    fn validates_outputs_and_citation_references_before_operands() {
        let graph = GraphDefinition::new(
            vec![derived("result", reference("input.missing"))],
            vec![],
            vec![id("derived.unknown")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::UnknownOutput {
                output_id: id("derived.unknown"),
            }))
        );

        let claim_id = CitationId::new("missing.docs").expect("citation ID should be valid");
        let claimed_input = Node::new(
            suffix("source"),
            NodeMetadata::new(
                String::from("Label"),
                String::from("Description"),
                vec![CitationClaim::new(claim_id.clone(), String::from("Claim"))],
            ),
            Input::new(ValueType::Scalar, None, vec![]).expect("input should be valid"),
        );
        let graph = GraphDefinition::new(
            vec![
                claimed_input.into(),
                derived("result", reference("input.missing")),
            ],
            vec![],
            vec![id("derived.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::UnknownCitation {
                node_id: id("input.source"),
                claim: 0,
                citation_id: claim_id,
            }))
        );
    }

    #[googletest::test]
    fn rejects_duplicate_or_non_result_outputs() {
        let definition = valid_definition();
        let graph = GraphDefinition::new(
            definition.nodes.clone(),
            vec![],
            vec![id("setting.result"), id("setting.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::DuplicateOutput {
                output_id: id("setting.result"),
            }))
        );

        let graph = GraphDefinition::new(
            vec![input("source", ValueType::Scalar)],
            vec![],
            vec![id("input.source")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::InvalidOutputNodeType {
                output_id: id("input.source"),
                node_type: "input",
            }))
        );
    }

    #[googletest::test]
    fn rejects_unknown_and_finding_value_operands() {
        let graph = GraphDefinition::new(
            vec![derived("result", reference("input.missing"))],
            vec![],
            vec![id("derived.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::UnknownOperand {
                node_id: id("derived.result"),
                operand: 0,
                referenced_node_id: id("input.missing"),
            }))
        );

        let finding: AnyNode = Node::new(
            suffix("notice"),
            metadata(),
            Finding::new(FindingSeverity::Warning, FindingCondition::Always),
        )
        .into();
        let graph = GraphDefinition::new(
            vec![finding, derived("result", reference("finding.notice"))],
            vec![],
            vec![id("derived.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::FindingOperand {
                node_id: id("derived.result"),
                operand: 0,
                finding_id: id("finding.notice"),
            }))
        );
    }

    #[googletest::test]
    fn reports_a_deterministic_closed_cycle_path() {
        let graph = GraphDefinition::new(
            vec![
                derived("first", reference("derived.second")),
                derived("second", reference("derived.first")),
            ],
            vec![],
            vec![id("derived.first")],
        );

        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::Cycle {
                path: vec![
                    id("derived.first"),
                    id("derived.second"),
                    id("derived.first"),
                ],
            }))
        );
    }

    #[googletest::test]
    fn checks_expression_and_finding_types_in_topological_order() {
        let graph = GraphDefinition::new(
            vec![
                input("count", ValueType::MessageCount),
                input("size", ValueType::DataSize),
                derived(
                    "result",
                    Expression::<Add>::new(
                        operand("input.count", "count"),
                        operand("input.size", "size"),
                    ),
                ),
            ],
            vec![],
            vec![id("derived.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::ExpressionType {
                node_id: id("derived.result"),
                source: ExpressionError::IncompatibleOperandType {
                    operation: "add",
                    operand: 1,
                    expected: ValueType::MessageCount,
                    actual: ValueType::DataSize,
                },
            }))
        );

        let finding: AnyNode = Node::new(
            suffix("mismatch"),
            metadata(),
            Finding::new(
                FindingSeverity::Warning,
                FindingCondition::Comparison(Comparison::new(
                    operand("input.count", "count"),
                    ComparisonOperator::GreaterThan,
                    operand("input.size", "size"),
                )),
            ),
        )
        .into();
        let graph = GraphDefinition::new(
            vec![
                input("count", ValueType::MessageCount),
                input("size", ValueType::DataSize),
                finding,
            ],
            vec![],
            vec![id("finding.mismatch")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::FindingComparisonTypeMismatch {
                node_id: id("finding.mismatch"),
                left: ValueType::MessageCount,
                right: ValueType::DataSize,
            }))
        );
    }

    #[googletest::test]
    fn checks_setting_keys_before_output_reachability() {
        let graph = GraphDefinition::new(
            vec![
                input("source", ValueType::MessageCount),
                constant("unused", Value::MessageCount(1)),
                setting("first", "duplicate.key", reference("input.source")),
                setting("second", "duplicate.key", reference("input.source")),
            ],
            vec![],
            vec![id("setting.first"), id("setting.second")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::DuplicateSettingKey {
                key: String::from("duplicate.key"),
                first_node_id: id("setting.first"),
                duplicate_node_id: id("setting.second"),
            }))
        );

        let graph = GraphDefinition::new(
            vec![
                input("source", ValueType::MessageCount),
                constant("unused", Value::MessageCount(1)),
                setting("result", "unique.key", reference("input.source")),
            ],
            vec![],
            vec![id("setting.result")],
        );
        assert_that!(
            graph.validate(),
            err(eq(&GraphValidationError::UnreachableNode {
                node_id: id("constant.unused"),
            }))
        );
    }

    #[googletest::test]
    fn indexes_referenced_citations() {
        let citation_id = CitationId::new("client.docs").expect("citation ID should be valid");
        let citation = Citation::new(
            citation_id.clone(),
            String::from("Client docs"),
            String::from("https://example.com/client"),
            String::from("Client behavior"),
        );
        let source: AnyNode = Node::new(
            suffix("source"),
            NodeMetadata::new(
                String::from("Label"),
                String::from("Description"),
                vec![CitationClaim::new(
                    citation_id.clone(),
                    String::from("Supported claim"),
                )],
            ),
            Input::new(ValueType::MessageCount, None, vec![]).expect("input should be valid"),
        )
        .into();
        let graph = GraphDefinition::new(
            vec![source, derived("result", reference("input.source"))],
            vec![citation.clone()],
            vec![id("derived.result")],
        )
        .validate()
        .expect("referenced citation should be valid");

        assert_that!(graph.citation(&citation_id), eq(Some(&citation)));
        assert_that!(graph.citations(), eq([citation]));
    }
}
