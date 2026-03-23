# Nicknamer YAML Bulk Export

## Summary

Add functionality to bulk-export names from the nicknamer service as a YAML file, matching the existing bulk-import format for round-tripping.

## API

**`GET /api/v1/names/export`**
- Query param: `server_id` (optional) — filters by server
- Response: `Content-Type: application/x-yaml`, `Content-Disposition: attachment; filename="names.yaml"`
- Body: `discord_id: name` YAML mapping (matches bulk import format)
- Auth required
- Documented with utoipa for Swagger

### Example response

```yaml
123456789: Alice
987654321: Bob
```

## UI

Download button on the existing HTMX names list page that exports all names as a YAML file.

## Implementation touches

1. `nicknamer/server/lib/src/name/api/v1.rs` — new `export_names` handler
2. `nicknamer/server/lib/src/name/web.rs` — new handler for UI-triggered download
3. Template update — add download button to names list template
4. Router wiring in both API and web routers
