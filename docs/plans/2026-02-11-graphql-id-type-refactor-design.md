# GraphQL ID Type Refactor Design

**Date:** 2026-02-11  
**Project:** nicknamer2  
**Status:** Approved

## Overview

Refactor GraphQL endpoints in nicknamer2 to use the standard GraphQL `ID` scalar type instead of `UUID` and `String` types for identifiers. This change follows GraphQL best practices while maintaining strong typing in the domain layer.

## Motivation

The current GraphQL schema uses:

- `user_id: UUID` (custom scalar)
- `server_id: String` (u64 serialized as string)

This refactor uses the standard GraphQL `ID` type for both identifiers to:

- Follow GraphQL conventions and best practices
- Provide consistent identifier representation
- Improve API clarity (IDs are opaque identifiers, not arbitrary strings)
- Enable future compatibility with Relay-style patterns

## Current State

**GraphQL Schema:**

```graphql
type Query {
  name(userId: UUID!, serverId: String!): Name
}

type Name {
  userId: UUID!
  serverId: String!
  name: String!
  createdAt: DateTime!
  updatedAt: DateTime!
}
```

**Domain Models:**

- `user::User` has `id: Uuid`
- `name::Name` has `user_id: Uuid` and `server_id: u64`

**Current Issues:**

- Manual parsing of `server_id` string to u64 in resolver
- Inconsistent ID type representation (UUID vs String)
- Non-standard GraphQL types for identifiers

## Proposed Solution

### Architecture

Implement custom GraphQL scalars that serialize as `ID` but maintain strong typing:

```
┌─────────────────────────────────────────┐
│         GraphQL API Layer               │
│  (UuidAsId, U64AsId wrapping scalars)   │
└─────────────────┬───────────────────────┘
                  │ From/Into conversions
┌─────────────────▼───────────────────────┐
│         Domain Layer                    │
│        (Uuid, u64 native types)         │
└─────────────────────────────────────────┘
```

### Component Design

#### 1. Custom Scalar Definitions (`graphql/scalars.rs`)

Two wrapper types that implement custom GraphQL scalars:

```rust
/// Wrapper for Uuid that serializes as GraphQL ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UuidAsId(pub Uuid);

#[graphql_scalar(name = "ID", description = "Unique identifier (UUID)")]
impl<S> GraphQLScalar for UuidAsId where S: ScalarValue {
    fn resolve(&self) -> Value {
        Value::scalar(self.0.to_string())
    }

    fn from_input_value(value: &InputValue) -> Option<Self> {
        value.as_string_value()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(UuidAsId)
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        <String as ParseScalarValue<S>>::from_str(value)
    }
}

/// Wrapper for u64 that serializes as GraphQL ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct U64AsId(pub u64);

#[graphql_scalar(name = "ID", description = "Unique identifier (unsigned 64-bit integer)")]
impl<S> GraphQLScalar for U64AsId where S: ScalarValue {
    fn resolve(&self) -> Value {
        Value::scalar(self.0.to_string())
    }

    fn from_input_value(value: &InputValue) -> Option<Self> {
        value.as_string_value()
            .and_then(|s| s.parse::<u64>().ok())
            .map(U64AsId)
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        <String as ParseScalarValue<S>>::from_str(value)
    }
}
```

**Key Properties:**

- Both serialize to string representation (GraphQL ID standard)
- Parse from string input with validation
- Validation errors occur at input parsing stage (before resolver execution)
- Unwrap inner types to pass to domain layer

#### 2. GraphQL Model Updates (`graphql/model.rs`)

```rust
use graphql_scalars::{UuidAsId, U64AsId};

#[derive(GraphQLObject)]
pub struct Name {
    pub user_id: UuidAsId,      // Changed from Uuid
    pub server_id: U64AsId,     // Changed from String
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<name::Name> for Name {
    fn from(n: name::Name) -> Self {
        Self {
            user_id: UuidAsId(n.user_id),
            server_id: U64AsId(n.server_id),  // No more .to_string()
            name: n.name,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}
```

#### 3. Query Resolver Updates (`graphql/query.rs`)

```rust
use graphql_scalars::{UuidAsId, U64AsId};

#[graphql_object]
#[graphql(context = Context)]
impl QueryRoot {
    async fn name(
        context: &Context,
        user_id: UuidAsId,
        server_id: U64AsId,
    ) -> FieldResult<Option<Name>> {
        // Extract domain types - no parsing needed!
        let user_uuid = user_id.0;
        let server_u64 = server_id.0;

        let result = context
            .name_service
            .get_name(user_uuid, server_u64)
            .await
            .map_err(|e| format!("{e}"))?;

        Ok(result.map(Name::from))
    }
}
```

**Improvements:**

- No manual parsing with `.parse()`
- No custom error messages for invalid formats
- Cleaner resolver logic focused on business logic
- Type safety maintained end-to-end

#### 4. Module Structure (`graphql/mod.rs`)

```rust
pub mod context;
pub mod model;
pub mod query;
pub mod scalars;  // New module
pub mod schema;

pub use scalars::{UuidAsId, U64AsId} as graphql_scalars;
// ... other re-exports
```

### Error Handling

**Before:**

```rust
let server_id: u64 = server_id
    .parse()
    .map_err(|_| "server_id must be a valid u64")?;
```

**After:**

- Invalid IDs rejected automatically by Juniper scalar parsing
- Standard GraphQL error format
- Errors occur at input validation stage (fail fast)
- Resolvers only handle business logic errors

**Error Response Example:**

```json
{
  "errors": [
    {
      "message": "Invalid value for argument \"userId\": failed to parse ID",
      "locations": [{ "line": 2, "column": 14 }],
      "path": ["name"]
    }
  ],
  "data": null
}
```

### Schema Changes

**Before:**

```graphql
type Query {
  name(userId: UUID!, serverId: String!): Name
}

type Name {
  userId: UUID!
  serverId: String!
  name: String!
  createdAt: DateTime!
  updatedAt: DateTime!
}

scalar UUID
```

**After:**

```graphql
type Query {
  name(userId: ID!, serverId: ID!): Name
}

type Name {
  userId: ID!
  serverId: ID!
  name: String!
  createdAt: DateTime!
  updatedAt: DateTime!
}
```

**Benefits:**

- Standard GraphQL `ID` type
- Consistent identifier representation
- Simpler schema (no custom UUID scalar)
- Follows GraphQL best practices

## Implementation Plan

### Phase 1: Create Scalar Infrastructure

1. Create `nicknamer2/src/graphql/scalars.rs`
2. Implement `UuidAsId` scalar with unit tests
3. Implement `U64AsId` scalar with unit tests
4. Add module to `graphql/mod.rs` with re-exports

### Phase 2: Update GraphQL Layer

5. Update `graphql/model.rs` to use new scalar types
6. Update `From<name::Name>` conversion implementation
7. Update `graphql/query.rs` resolver parameters
8. Remove manual parsing logic from resolver

### Phase 3: Update Tests

9. Update existing integration tests in `graphql/tests.rs`
10. Add test for ID variables: `test_query_with_id_variables`
11. Update invalid format tests to verify early validation
12. Verify error messages match expected GraphQL format

### Phase 4: Verification

13. Run `aspect test //nicknamer2/...`
14. Run `aspect build //nicknamer2/...`
15. Verify generated GraphQL schema via introspection
16. Run `format` for code style compliance

### Phase 5: Documentation (Optional)

17. Update API documentation if it exists
18. Add inline comments explaining scalar wrapper pattern

## Testing Strategy

**Existing Tests (minimal changes):**

- Integration tests continue to work (ID accepts string input)
- Assertions remain the same (ID serializes as string)
- Query format unchanged

**New Test Cases:**

```rust
#[tokio::test]
async fn test_query_with_id_variables() {
    // Verify GraphQL variables work with ID type
    let query = r#"
        query GetName($userId: ID!, $serverId: ID!) {
            name(userId: $userId, serverId: $serverId) { name }
        }
    "#;
}

#[tokio::test]
async fn test_invalid_uuid_id_format() {
    // Verify proper error for malformed UUID
    // Should get GraphQL error before reaching resolver
}

#[tokio::test]
async fn test_invalid_server_id_format() {
    // Verify proper error for malformed server ID
    // Should get GraphQL error before reaching resolver
}
```

## Migration Strategy

**Breaking Change (Clean Cutover):**

- Update schema and implementation in single change
- No deprecation period needed (early stage API)
- Update all tests simultaneously
- Document breaking change in commit message

**Client Impact:**

- Clients using string representations: no changes needed
- Clients using typed GraphQL clients: regenerate schema types
- Error messages may change for invalid inputs

## Benefits

**GraphQL Best Practices:**

- Standard `ID` type for all identifiers
- Consistent API design
- Future-proof for Relay patterns

**Type Safety:**

- Strong typing maintained in domain layer
- Compile-time guarantees for conversions
- Clear API ↔ domain boundaries

**Code Quality:**

- Cleaner resolver logic (no manual parsing)
- Earlier validation (fail fast)
- Better error handling with standard format
- Less boilerplate in resolvers

**Maintainability:**

- Explicit type conversions via wrappers
- Single responsibility (scalars handle parsing)
- Testable validation logic
- Clear separation of concerns

## Alternatives Considered

**Alternative 1: Direct juniper::ID Usage**

- Use `juniper::ID` type directly in models and resolvers
- Parse to domain types at runtime in resolvers
- **Rejected:** Loses type safety, runtime parsing in multiple places

**Alternative 2: Type Aliases with Annotations**

- Keep domain types with `#[graphql(scalar = "ID")]` annotations
- **Rejected:** Not directly supported by Juniper for UUID/u64, still needs custom scalars

## Risks and Mitigations

**Risk:** Breaking change for existing clients  
**Mitigation:** API is early stage, clean break is acceptable

**Risk:** Both scalars use same `ID` name  
**Mitigation:** GraphQL allows this - scalars are context-specific (input vs output position)

**Risk:** Confusion about which ID type to use  
**Mitigation:** Clear naming (`UuidAsId`, `U64AsId`) and documentation

## Success Criteria

- All tests pass after refactor
- GraphQL schema uses `ID` type for identifiers
- No manual parsing in resolvers
- Domain types remain unchanged
- Error messages follow GraphQL standards
- Code formatted and builds successfully

## Future Enhancements

- Add more ID types as needed (e.g., `I64AsId` for signed integers)
- Consider Relay Global Object Identification pattern
- Add GraphQL documentation for ID format expectations
- Create helper functions for common ID operations
