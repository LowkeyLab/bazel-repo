# Nicknamer YAML Bulk Export Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a YAML bulk-export endpoint and UI download button to the nicknamer service, using the same `discord_id: name` format as bulk import.

**Architecture:** New `export_names` handler in the API layer serializes names to YAML via `serde_yaml`. A parallel web handler serves the same content with download headers. The existing names list template gets a download button.

**Tech Stack:** Rust, Axum, serde_yaml, Askama (HTMX templates), utoipa, insta (snapshot tests)

---

## File Structure

| Action | File                                               | Responsibility                                   |
| ------ | -------------------------------------------------- | ------------------------------------------------ |
| Modify | `nicknamer/server/lib/src/name/api/v1.rs`          | Add `export_names_handler`, wire into API router |
| Modify | `nicknamer/server/lib/src/name/web.rs`             | Add `export_names_handler` for web download      |
| Modify | `nicknamer/server/lib/templates/names.html`        | Add download button                              |
| Modify | `nicknamer/server/lib/src/web/api.rs`              | Register new endpoint in OpenAPI spec            |
| Modify | `nicknamer/server/lib/tests/name_service_tests.rs` | Add export endpoint tests                        |

---

## Chunk 1: API Export Endpoint

### Task 1: Add export handler to API

**Files:**

- Modify: `nicknamer/server/lib/src/name/api/v1.rs`
- Modify: `nicknamer/server/lib/src/web/api.rs`

- [ ] **Step 1: Write the failing test**

Add to `nicknamer/server/lib/tests/name_service_tests.rs` inside the `mod api { mod v1 { ... } }` block:

```rust
#[tokio::test]
async fn export_names_returns_yaml() {
    let ctx = setup().await;
    let name_service = NameService::new(ctx.db.clone());
    name_service
        .create_name(111, "Alice", "server1")
        .await
        .unwrap();
    name_service
        .create_name(222, "Bob", "server1")
        .await
        .unwrap();

    let app = create_test_app(ctx.db.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/names/export")
                .header("Authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/x-yaml"
    );
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("names.yaml"));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    // Verify YAML contains the discord_id: name mappings
    assert!(body_str.contains("111"));
    assert!(body_str.contains("Alice"));
    assert!(body_str.contains("222"));
    assert!(body_str.contains("Bob"));
}

#[tokio::test]
async fn export_names_filters_by_server_id() {
    let ctx = setup().await;
    let name_service = NameService::new(ctx.db.clone());
    name_service
        .create_name(111, "Alice", "server1")
        .await
        .unwrap();
    name_service
        .create_name(222, "Bob", "server2")
        .await
        .unwrap();

    let app = create_test_app(ctx.db.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/names/export?server_id=server1")
                .header("Authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Alice"));
    assert!(!body_str.contains("Bob"));
}

#[tokio::test]
async fn export_names_empty_returns_empty_yaml() {
    let ctx = setup().await;
    let app = create_test_app(ctx.db.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/names/export")
                .header("Authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body_str.trim(), "{}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `aspect test //nicknamer/server/lib/tests:tests --test_filter="export_names"`
Expected: FAIL — handler doesn't exist yet

- [ ] **Step 3: Implement the export handler**

In `nicknamer/server/lib/src/name/api/v1.rs`, add the handler:

```rust
use std::collections::BTreeMap;

/// Export all names as a YAML file
#[utoipa::path(
    get,
    path = "/api/v1/names/export",
    params(
        ("server_id" = Option<String>, Query, description = "Filter by server ID")
    ),
    responses(
        (status = 200, description = "YAML file with discord_id: name mappings", content_type = "application/x-yaml"),
        (status = 500, description = "Internal server error", body = ServerErrorResponse)
    ),
    tag = "names"
)]
pub async fn export_names_handler(
    State(db): State<DatabaseConnection>,
    Query(query): Query<NamesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ServerErrorResponse>)> {
    let name_service = NameService::new(db);
    let names = match &query.server_id {
        Some(server_id) => name_service.get_names_by_server(server_id).await,
        None => name_service.get_all_names().await,
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServerErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let yaml_map: BTreeMap<u64, String> = names
        .into_iter()
        .map(|n| (n.discord_id(), n.name().to_string()))
        .collect();

    let yaml = serde_yaml::to_string(&yaml_map).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServerErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"names.yaml\"",
            ),
        ],
        yaml,
    ))
}
```

Add `use axum::http::header;` to the imports if not already present.

- [ ] **Step 4: Wire the handler into the API router**

In the `create_api_router()` function in `nicknamer/server/lib/src/name/api/v1.rs`, add the route:

```rust
.route("/names/export", get(export_names_handler))
```

**Important:** This must come BEFORE any `/names/{path_param}` routes to avoid path conflicts.

- [ ] **Step 5: Register in OpenAPI spec**

In `nicknamer/server/lib/src/web/api.rs`, add `export_names_handler` to the `paths()` list in the `#[openapi]` macro on `ApiDoc`.

- [ ] **Step 6: Run tests to verify they pass**

Run: `aspect test //nicknamer/server/lib/tests:tests --test_filter="export_names"`
Expected: PASS (all 3 tests)

- [ ] **Step 7: Build and verify**

Run: `bazel run gazelle && format && aspect build //nicknamer/...`

- [ ] **Step 8: Commit**

```bash
git add nicknamer/server/lib/src/name/api/v1.rs nicknamer/server/lib/src/web/api.rs nicknamer/server/lib/tests/name_service_tests.rs
git commit -m "feat(nicknamer): add YAML export API endpoint"
```

---

## Chunk 2: Web UI Download Button

### Task 2: Add download button to names page

**Files:**

- Modify: `nicknamer/server/lib/src/name/web.rs`
- Modify: `nicknamer/server/lib/templates/names.html`

- [ ] **Step 1: Add the web export handler**

In `nicknamer/server/lib/src/name/web.rs`, add a handler that serves the YAML as a file download:

```rust
use std::collections::BTreeMap;

pub async fn export_names_handler(
    State(state): State<NameState>,
) -> Result<impl IntoResponse, StatusCode> {
    let name_service = NameService::new(state.db);
    let names = name_service
        .get_all_names()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let yaml_map: BTreeMap<u64, String> = names
        .into_iter()
        .map(|n| (n.discord_id(), n.name().to_string()))
        .collect();

    let yaml = serde_yaml::to_string(&yaml_map)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"names.yaml\"",
            ),
        ],
        yaml,
    ))
}
```

- [ ] **Step 2: Wire the web export route**

In `create_name_router()` in `nicknamer/server/lib/src/name/web.rs`, add:

```rust
.route("/names/export", get(export_names_handler))
```

**Important:** Place before any `/names/{id}` routes.

- [ ] **Step 3: Add download button to the template**

In `nicknamer/server/lib/templates/names.html`, add a download button alongside the existing action buttons (Add Name, Bulk Add, Bulk Delete). It should be a plain `<a>` tag (not HTMX) since it triggers a file download:

```html
<a href="/names/export" class="btn btn-secondary"> Export YAML </a>
```

Place this in the button group at the top of the names page.

- [ ] **Step 4: Run the full test suite**

Run: `aspect test //nicknamer/server/lib/tests:tests`
Expected: All tests pass (existing + new)

- [ ] **Step 5: Update any affected insta snapshots**

If snapshot tests fail due to the new button in the template:

Run: `INSTA_UPDATE=always aspect test //nicknamer/server/lib/tests:tests`

Review the snapshot diffs to confirm they only show the new export button.

- [ ] **Step 6: Build and verify**

Run: `bazel run gazelle && format && aspect build //nicknamer/...`

- [ ] **Step 7: Commit**

```bash
git add nicknamer/server/lib/src/name/web.rs nicknamer/server/lib/templates/names.html
git commit -m "feat(nicknamer): add YAML export download button to web UI"
```
