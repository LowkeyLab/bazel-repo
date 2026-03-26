# Nicknamer2-Web Import Format Compatibility Design

## Goal
Change nicknamer2-web's bulk import YAML parser to accept the nicknamer export format (flat map of discord_id → name), replacing the current array-of-objects format.

## Current State

**Nicknamer export format** (`/api/v1/names/export`):
```yaml
123456789: Alice
987654321: Bob
```

**Nicknamer2-web import format** (current):
```yaml
- discordId: "123456789"
  name: Alice
- discordId: "987654321"
  name: Bob
```

These are incompatible. Users cannot paste exported YAML directly into the import form.

## Design

### Approach
Change the frontend YAML parser in `batch-add-names.component.ts` to expect a flat map instead of an array. The parsed map entries get converted to `NameEntryInput[]` before calling the existing GraphQL mutation (unchanged).

### New expected input format
```yaml
123456789: Alice
987654321: Bob
```
A flat YAML map where keys are Discord user IDs (numbers) and values are display names (strings).

### Parsing logic
1. Parse YAML string — expect an object (not an array)
2. Validate it's a non-empty plain object (not null, not an array)
3. For each key-value pair:
   - Key must be a valid number (Discord ID) — convert to string for `NameEntryInput.discordId`
   - Value must be a non-empty string — maps to `NameEntryInput.name`
4. Build the `NameEntryInput[]` array and pass to existing `createNames` mutation (unchanged)

### Validation error messages
| Condition | Message |
|-----------|---------|
| Not an object | "YAML must be a mapping of discord IDs to names" |
| Empty object | "No entries found in YAML" |
| Invalid key | "Entry 'X': invalid Discord ID (must be a number)" |
| Invalid value | "Entry 'X': missing or invalid name" |

### What changes
- **batch-add-names.component.ts**: YAML parsing logic + placeholder text
- **batch-add-names.component.spec.ts**: Updated test cases

### What stays the same
- GraphQL mutation (`CreateNames`)
- `NameEntryInput` type
- Backend validation
- Component template/UI (except placeholder text)
