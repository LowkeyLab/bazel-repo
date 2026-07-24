# Kafka Buffer Calculator Core Design

## Status

This document is the agreed high-level design for the pure Rust expression evaluator and explainable calculation-graph engine. The initial calculation profile targets librdkafka producer and regular high-level consumer buffer settings. Additional Kafka client profiles may be added later without moving calculation logic into a frontend.

Implementation is incremental. The document describes the intended completed architecture; the presence of a type or module here does not imply that it has already been implemented.

## Goals

The core must:

- Calculate Kafka client buffer recommendations with exact, deterministic arithmetic.
- Represent every meaningful input, constant, intermediate value, recommendation, and finding as an explainable graph node.
- Make causal links explicit: every result must identify the values that contributed to it and the role each value played.
- Distinguish mathematical derivation from external justification by attaching citations to client-specific claims.
- Generate structured evaluation traces that a caller can render without reimplementing calculations.
- Keep client-specific settings, defaults, bounds, and policies in reusable profiles rather than in the generic evaluator.
- Reject malformed graphs before evaluation.
- Remain a pure Rust library that can later be used by a thin WebAssembly adapter, CLI, or server.

## Non-goals

The initial core will not:

- Render a graph or user interface.
- Contain Angular or browser-specific behavior.
- Reimplement calculations in TypeScript.
- Query a Kafka cluster or inspect a running client.
- Guarantee process RSS from payload limits; queue-size results are payload budgets, while real memory use can include metadata, compression buffers, requests, and language-binding copies.
- Support arbitrary nested expressions, arbitrary decimal division, negative values, or client-specific evaluator branches.
- Support librdkafka share-consumer queue sizing, because the regular-consumer queue properties do not apply to share consumers.

## Architectural overview

The core uses distinct phases and types:

```text
Client profile
    |
    v
GraphDefinition
    | validate
    v
ValidatedGraph
    | bind typed runtime inputs
    v
BoundGraph
    | evaluate in topological order
    v
Evaluation
```

`GraphDefinition` describes formulas and explanations. `ValidatedGraph` proves the structure is safe to evaluate. `BoundGraph` combines a validated graph with one request's inputs. `Evaluation` contains values, findings, and traces for that request.

The librdkafka profile owns a reusable `ValidatedGraph`. It maps a typed `LibrdkafkaInputs` value to generic graph inputs and delegates all arithmetic to the engine.

## Exact values and units

### Decimal parsing

User-entered decimals are parsed with `rust_decimal::Decimal::from_str_exact` through a domain wrapper such as `ExactDecimal`. The wrapper enforces non-negative values and checked arithmetic. Binary floating-point numbers are not used.

Ordinary unsigned decimal syntax is sufficient initially. Scientific notation, `NaN`, infinity, and negative values are rejected.

### Whole values

Message counts, whole-byte values after explicit rounding, and configuration values use `u128`. Conversions from decimal to whole values are explicit graph operations. Arithmetic must never silently overflow or round.

### Units

The value model distinguishes at least:

- Scalars
- Ratios
- Message counts
- Data sizes with `B`, `KB`, `KiB`, `MB`, and `MiB`
- Final setting units such as bytes, KBytes, and messages

SI and IEC units retain their distinct factors. `ConvertDataSize` resolves the source unit from the referenced node's evaluated quantity and converts it to an explicit target unit. The operation is generic and has no client-profile awareness.

Client-specific interpretations are represented by profile constants instead. For example, the librdkafka producer queue KByte divisor is 1,024 bytes, while the consumer queue KByte divisor is 1,000 bytes. Those values are separate cited constant nodes and are not hidden inside unit conversion.

## Node identity and metadata

A node is the smallest fact that a caller can ask to explain. Stable textual identifiers use validated `String` newtypes:

```rust
pub struct NodeId(String);
pub struct NodeIdSuffix(String);
pub struct CitationId(String);
```

A `NodeId` consists of a node-type prefix and a caller-provided `NodeIdSuffix`. Both use lowercase dotted segments with no empty segments. The suffix should exclude the node-type prefix; for example, an input node constructed with `NodeIdSuffix::new("message.maximum_size")` receives the full ID `input.message.maximum_size`.

Full node IDs remain the stable identifiers used by operands, graph outputs, input binding, traces, and external lookup:

```text
input.message.maximum_size
derived.message.safe_bytes
derived.producer.queue_bytes
setting.producer.queue_buffering_max_kbytes
finding.consumer.queue_limit_exceeded
```

A node owns its stable ID separately from its common descriptive metadata. `NodeMetadata` contains:

- Short English label
- English description of what the node means
- Zero or more citation claims

The Rust profile owns the default English explanatory text so that meaning remains beside the formulas. Text fields use `String`; `Box<str>` is not used as a premature storage optimization.

## Citations

A citation supports a non-mathematical claim, such as the meaning, default, bound, or implementation behavior of a librdkafka setting. A citation answers "what authoritative source justifies this rule?" while a graph dependency answers "what caused this number?"

A graph owns a citation catalog. Nodes reference citations through specific claims rather than duplicating source metadata.

```rust
pub struct Citation {
    id: CitationId,
    title: String,
    url: String,
    summary: String,
}

pub struct CitationClaim {
    citation_id: CitationId,
    claim: String,
}
```

User inputs and ordinary arithmetic generally need no citation. Client defaults, setting semantics, supported bounds, and surprising client behavior should be cited. Calculator policies must be identified as policies rather than represented as client requirements.

## Typed nodes

Nodes are parameterized by their node type:

```rust
pub struct Node<T: NodeTypeMetadata> {
    id: NodeId,
    metadata: NodeMetadata,
    node_type: T,
}
```

The concrete node types are `Input`, `Constant`, `Derived`, `Setting`, and `Finding`. Each stores its kind-specific unevaluated data directly.

Each concrete node type implements the sealed `NodeTypeMetadata` trait. Its associated prefix is static metadata selected by `T`, not data supplied to an individual node:

```rust
mod private {
    pub trait Sealed {}
}

pub trait NodeTypeMetadata: private::Sealed {
    const ID_PREFIX: &'static str;
}

impl private::Sealed for Input {}
impl NodeTypeMetadata for Input {
    const ID_PREFIX: &'static str = "input";
}

impl private::Sealed for Constant {}
impl NodeTypeMetadata for Constant {
    const ID_PREFIX: &'static str = "constant";
}

impl private::Sealed for Derived {}
impl NodeTypeMetadata for Derived {
    const ID_PREFIX: &'static str = "derived";
}
```

`Setting` and `Finding` implement the same trait with `setting` and `finding` prefixes. Sealing keeps the engine-level node-type set closed and preserves exhaustive graph behavior while still allowing profiles to construct arbitrary nodes of those types.

The generic constructor derives the full ID from `T::ID_PREFIX` and the validated suffix:

```rust
impl<T: NodeTypeMetadata> Node<T> {
    pub fn new(suffix: NodeIdSuffix, metadata: NodeMetadata, node_type: T) -> Self {
        Self {
            id: NodeId::from_parts(T::ID_PREFIX, suffix),
            metadata,
            node_type,
        }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
    }

    pub fn metadata(&self) -> &NodeMetadata {
        &self.metadata
    }

    pub fn node_type(&self) -> &T {
        &self.node_type
    }
}
```

A typed node stores `T` directly, so it needs neither an additional runtime kind field nor `PhantomData<T>`. Private fields and construction through `Node<T>::new` make a mismatch between a node type and its ID prefix unrepresentable.

Graphs erase typed nodes only at the heterogeneous storage boundary:

```rust
pub enum AnyNode {
    Input(Node<Input>),
    Constant(Node<Constant>),
    Derived(Node<Derived>),
    Setting(Node<Setting>),
    Finding(Node<Finding>),
}
```

Conversions from each `Node<T>` into its matching `AnyNode` variant keep graph assembly ergonomic. `AnyNode` exposes common ID and metadata accessors through exhaustive matching. The enum is preferred over trait objects for the same reasons as `AnyExpression`: the closed set benefits from exhaustive matching and straightforward cloning, equality, validation, and future serialization.

### Input nodes

`Input` declares the expected value type, optional default, and hard input constraints. Runtime values are supplied separately during binding. A bound input records whether it was supplied or defaulted.

Hard constraints determine whether a request is meaningful and evaluable. Client configuration limits should normally become findings instead, allowing the graph to calculate and explain an out-of-range recommendation.

### Constant nodes

`Constant` holds a fixed value and identifies its origin:

- Unit definition
- Client default
- Client constraint
- Calculator policy

A rationale explains why the graph contains the constant. Citations support client-specific claims.

### Derived nodes

`Derived` contains an `AnyExpression` and no precomputed value. The evaluator obtains its type and value from referenced nodes.

### Setting nodes

`Setting` contains a client configuration key, producer/consumer/common scope, setting unit, and `AnyExpression`. Settings remain graph nodes so they can have incoming causes, citations, traces, constraints, and copied configuration representations.

### Finding nodes

`Finding` represents an informational, warning, or error conclusion. Findings are evaluated results, not engine failures. An error-severity finding means the calculation succeeded and discovered an unsupported recommendation.

## Expression model

Expressions reference existing nodes only. They cannot contain nested anonymous expressions. Every meaningful intermediate result therefore has its own stable identity, metadata, value, trace, and selectable place in the graph.

An operand contains a source node and a human-readable role:

```rust
pub struct Operand {
    node_id: NodeId,
    role: String,
}
```

The completed initial expression set is:

- `Reference`
- `Add`
- `Multiply`
- `Ceiling`
- `CeilingDivide`
- `Minimum`
- `Maximum`
- `ConvertDataSize`

The first expression-model increment implements every operation above except `ConvertDataSize`. Unit conversion remains part of the completed architecture but is deliberately deferred to a later increment.

### Typed expression construction

An operation is represented by a marker type, and `Expression<K>` is the typed expression for that operation:

```rust
use std::marker::PhantomData;

pub struct Reference;
pub struct Add;
pub struct Multiply;
pub struct Ceiling;
pub struct CeilingDivide;
pub struct Minimum;
pub struct Maximum;

pub struct Expression<K> {
    operands: Vec<Operand>,
    kind: PhantomData<K>,
}
```

The marker determines which constructors and accessors are available. There is no generic constructor that accepts an arbitrary operand vector. Fixed and minimum arities are instead encoded by operation-specific signatures:

```rust
impl Expression<Reference> {
    pub fn new(source: Operand) -> Self;
    pub fn source(&self) -> &Operand;
}

impl Expression<Add> {
    pub fn new(left: Operand, right: Operand) -> Self;
    pub fn and(self, term: Operand) -> Self;
    pub fn terms(&self) -> &[Operand];
}

impl Expression<CeilingDivide> {
    pub fn new(dividend: Operand, divisor: Operand) -> Self;
    pub fn dividend(&self) -> &Operand;
    pub fn divisor(&self) -> &Operand;
}
```

`Multiply`, `Minimum`, and `Maximum` follow the same at-least-two pattern as `Add`, with operation-specific factor or candidate terminology. `Ceiling` follows the unary `Reference` shape but exposes the operand as the value being rounded. Private storage and typed constructors make invalid arity unrepresentable. Repeated node references remain valid, and variable-arity expressions preserve insertion order for deterministic edge generation, validation, and traces.

The expression-only increment does not need a heterogeneous wrapper. When derived and setting nodes are implemented, the graph introduces an erased enum named `AnyExpression`:

```rust
pub enum AnyExpression {
    Reference(Expression<Reference>),
    Add(Expression<Add>),
    Multiply(Expression<Multiply>),
    Ceiling(Expression<Ceiling>),
    CeilingDivide(Expression<CeilingDivide>),
    Minimum(Expression<Minimum>),
    Maximum(Expression<Maximum>),
    ConvertDataSize(Expression<ConvertDataSize>),
}
```

Conversions from each valid `Expression<K>` into `AnyExpression` keep graph assembly ergonomic without exposing an API that can bypass the typed constructors. The enum is preferred over trait objects because the closed operation set benefits from exhaustive matching and straightforward `Clone`, `Debug`, equality, validation, and later serialization support.

Each typed operation also owns an operation-local static type contract. It accepts operand `ValueType`s that a future graph validator has already resolved; it does not perform graph lookup. The initial contracts are:

- `Reference` preserves its source type.
- `Add`, `Minimum`, and `Maximum` require homogeneous operand types and preserve that type.
- `Multiply` supports homogeneous scalar or ratio multiplication and one data-size operand combined with scalar, ratio, or message-count factors. Dimensionally unsupported combinations such as data size multiplied by data size are rejected.
- `Ceiling` preserves the accepted decimal quantity category.
- `CeilingDivide` requires compatible whole-value categories and produces a scalar quotient.

`ValueType` currently describes broad quantity categories rather than decimal whole-ness or a data size's concrete unit. Static contracts therefore reject category-level incompatibilities; the later evaluator remains responsible for checking whole values, matching units, division by zero, overflow, and decimal precision.

Expressions contain no profile-specific numeric literals. Profile defaults, limits, policies, and divisors are constant nodes.

Operations use checked arithmetic and enforce compatible value types. `Ceiling` is the explicit decimal-to-whole transition. `CeilingDivide` operates on whole values and records the quotient, remainder, and whether rounding occurred. Arbitrary decimal division is deliberately excluded.

Complex formulas use named intermediate nodes. For example:

```text
maximum message size -- ConvertDataSize --> maximum message bytes
key/header size ------ ConvertDataSize --> key/header bytes

maximum message bytes --+
                         +--> raw record bytes
key/header bytes --------+

100% --------+
              +--> headroom multiplier
headroom -----+

raw record bytes --------+
                          +--> unrounded safe record size -- Ceiling --> safe record bytes
headroom multiplier ------+
```

## Edges and causality

Profiles do not author a separate edge list. Each operand in an expression or finding comparison produces an edge:

```text
operand.node_id -> current node ID
```

The operand role becomes the edge label. Deriving edges from expressions prevents formulas and dependency metadata from diverging.

The graph declares public output node IDs explicitly. Outputs may include settings, findings, and useful derived values such as estimated queue memory.

## Graph validation

`GraphDefinition::validate` consumes an unvalidated definition and returns either a `ValidatedGraph` or the first deterministic `GraphValidationError`:

```rust
pub fn validate(self) -> Result<ValidatedGraph, GraphValidationError>;
```

Evaluation accepts only a `ValidatedGraph`.

Validation checks, in deterministic order:

1. Local node validity
2. Duplicate node IDs
3. Output and citation references
4. Operand references
5. Cycles
6. Static expression types in topological order
7. Duplicate setting keys
8. Reachability from declared outputs

Cycle errors include the discovered cycle path. Every node must contribute to at least one output. Independent nodes retain stable declaration order during topological sorting.

The `Node<T>` constructor already guarantees that each node ID has the prefix associated with its node type, so graph validation does not need a separate prefix-kind consistency check.

The validated graph stores read-only indexes, generated edges, dependencies, dependents, and deterministic topological order. Callers cannot mutate these invariants.

## Input binding

Graph validation proves the formula graph is structurally valid; it does not validate one request's values. Binding is a separate phase:

```rust
pub fn bind(
    &self,
    inputs: EvaluationInputs,
) -> Result<BoundGraph<'_>, InputBindingError>;
```

Binding:

- Rejects unknown IDs
- Rejects values supplied to non-input nodes
- Resolves supplied values and defaults
- Rejects missing required values
- Checks value types
- Applies hard input constraints

`BoundGraph` owns request-specific values and borrows the reusable immutable `ValidatedGraph`, avoiding graph copies on every recalculation. A profile may expose a convenience method that performs binding and evaluation in one call.

## Evaluation

The evaluator visits nodes in validated topological order:

- Input nodes receive their bound values and supplied/defaulted origin.
- Constant nodes receive their defined values and constant origin.
- Derived nodes evaluate their expressions.
- Setting nodes evaluate their expressions and attach configuration semantics.
- Finding nodes evaluate their conditions.

Evaluation fails on the first arithmetic error and returns no partial result. Checked failures include overflow, division by zero, precision exhaustion, non-whole values where whole values are required, and defensive invariant violations. User-controlled values must not cause panics.

An `Evaluation` preserves topological node order and maintains a lookup index by `NodeId`.

## Evaluation traces

An expression defines what should be calculated. An `EvaluationTrace` records what happened for one request. Traces are generated by the evaluator, never authored by profiles.

Trace categories cover inputs, constants, expressions, and findings. Expression traces include:

- Operation kind
- Referenced node IDs
- Operand roles
- Evaluated operand values
- Operation-specific details

Examples of operation-specific details include:

- Source unit, target unit, and factor for `ConvertDataSize`
- Whether `Ceiling` rounded upward
- Quotient, remainder, and rounding decision for `CeilingDivide`
- Selected candidate nodes for `Minimum` and `Maximum`, including all tied candidates

Traces let a caller render substituted formulas without implementing arithmetic. For example:

```text
queue.buffering.max.kbytes
= ceil(producer queue bytes / producer config-kbyte bytes)
= ceil(120,000,000 / 1,024)
= 117,188
```

## Predicates and findings

The initial finding condition model supports one explicit comparison or an unconditional finding:

```rust
pub enum FindingCondition {
    Always,
    Comparison(Comparison),
}
```

Comparison operators are equality, inequality, less/greater than, and inclusive less/greater than. Compared values must be compatible and profiles must explicitly normalize units before comparison; predicate evaluation performs no hidden conversion.

Nested `AND` and `OR` predicates are intentionally excluded. If a real compound rule is required later, it should use named condition nodes so intermediate decisions remain explainable.

A finding trace records both evaluated operands, the comparison, and its Boolean outcome. Active findings remain part of a successful `Evaluation`. Convenience methods may expose active findings and the highest active severity.

## Client profiles

A client profile owns typed inputs, profile metadata, and one reusable validated graph:

```rust
pub trait CalculatorProfile {
    type Inputs;

    fn metadata(&self) -> &ProfileMetadata;
    fn graph(&self) -> &ValidatedGraph;
    fn to_evaluation_inputs(&self, inputs: Self::Inputs) -> EvaluationInputs;
    fn evaluate(&self, inputs: Self::Inputs) -> Result<Evaluation, CalculationError>;
}
```

The initial `LibrdkafkaProfile` is constructed fallibly with `try_new`; construction builds and validates its graph once. An infallible `Default` implementation must not hide graph errors with `unwrap`.

Typed `LibrdkafkaInputs` include:

- Maximum message size
- Optional key/header size
- Optional headroom
- Producer queue message count
- Consumer queue target count

Optional values are omitted from the generic input map so binding can apply profile-owned defaults and record their origin.

Input mapping only translates typed inputs into input node values. It must never precompute a derived value outside the graph.

Future Java, Sarama, kafka-go, or other profiles reuse the same graph engine. Client-specific branches do not belong in generic expression evaluation.

## Initial librdkafka derivations

The initial profile models these main paths:

```text
ceil(
    (maximum message bytes + key/header bytes)
    * (100% + headroom)
)
    -> safe record bytes

safe record bytes * producer queue message count
    -> producer queue bytes
    -> queue.buffering.max.kbytes

producer queue message count
    -> queue.buffering.max.messages

safe record bytes * consumer queue target count
    -> consumer queue bytes
    -> queued.max.messages.kbytes
```

The producer KByte setting uses a cited 1,024-byte divisor. The consumer KByte setting uses a cited 1,000-byte divisor matching librdkafka behavior.

The profile must explain that `queued.max.messages.kbytes` is a byte threshold, not a strict consumer message-count limit, and that fetches may overshoot it. `queued.min.messages` must not be presented as an equivalent strict maximum.

Large-message compatibility settings and fetch limits are represented as separate settings and policies with their own cited constants, causal paths, and findings. Broker or topic limits are surfaced as compatibility findings rather than silently assumed to be configured.

## Error model

Errors remain separated by phase:

- `ValueError` for constructing exact domain values
- `IdentifierError` and `ExpressionError` for model construction
- `ProfileBuildError` for profile assembly
- `GraphValidationError` for invalid graph definitions
- `InputBindingError` for requests that do not satisfy a valid graph
- `EvaluationError` for checked arithmetic or invariant failures
- `CalculationError` as a convenience wrapper over binding and evaluation failures

Graph-definition errors indicate profile-author defects. Binding errors indicate invalid requests. Evaluation errors mean no trustworthy result could be produced. Findings describe successful calculations that discovered risks or unsupported recommendations.

Library errors use `thiserror` and retain structured fields rather than exposing only formatted strings. Invariant-bearing types keep private fields and expose fallible constructors and read-only accessors.

The crate forbids unsafe code.

## Testing strategy

Tests use explicit assertions and focused helpers. Snapshot testing and `insta` are not used.

Test layers cover:

- Exact decimal parsing, units, explicit rounding, and checked arithmetic
- Typed node construction, `NodeIdSuffix` validation, type-derived ID prefixes, and `AnyNode` erasure
- Typed expression APIs, constructor-enforced arity, deterministic operand iteration, and operation-local static type contracts
- Graph validation errors, cycle paths, reachability, and deterministic topological order
- Input defaults, origins, type checks, and constraints
- Every generic operation's result and operation-specific trace
- Edge derivation and causal paths
- Comparison outcomes and active/inactive findings
- One realistic end-to-end librdkafka calculation
- Only profile-specific edge cases that are not already covered by generic evaluator tests

The test suite explicitly verifies graph structure, values, edge roles, traces, findings, citations, outputs, and deterministic ordering. Coverage is used to identify missing required behavior, not as a reason to add redundant tests.

All builds, tests, formatting, linting, and coverage run through Bazel and Aspect according to repository policy.

## Planned module boundaries

The pure Rust core is organized around small domain modules:

```text
kafka_calculator/core/
    lib.rs
    value.rs
    node.rs
    expression.rs
    graph.rs
    binding.rs
    evaluator.rs
    trace.rs
    finding.rs
    error.rs
    profiles/
        mod.rs
        librdkafka.rs
```

The exact file split may evolve as implementation clarifies cohesion, but the boundaries between generic graph behavior and client-profile behavior must remain intact.
