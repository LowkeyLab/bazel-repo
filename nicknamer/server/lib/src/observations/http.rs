//! Request context follows the polled async future, including across thread migration.
use super::{
    Fact, Method, Observation, ObservationContext, RequestId, RequestOutcome, Route, SharedObserver,
};
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use std::time::Instant;

tokio::task_local! { static CONTEXT: ObservationContext; }

/// Standalone handler/service callers still receive an independent operation context.
pub fn current_context() -> ObservationContext {
    CONTEXT.try_with(Clone::clone).unwrap_or_default()
}

struct Completion {
    observer: SharedObserver,
    context: ObservationContext,
    started: Instant,
    route: Route,
    method: Method,
    completed: bool,
}
impl Completion {
    fn record(&self, status: u16, outcome: RequestOutcome) {
        let mut context = self.context.clone().with_duration(self.started.elapsed());
        context.occurred_at = chrono::Utc::now();
        self.observer.record(&Observation {
            context,
            fact: Fact::RequestFinished {
                route: self.route,
                method: self.method,
                status,
                outcome,
            },
        });
    }
}
// Status 0 means that cancellation produced no HTTP response.
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.completed {
            self.record(0, RequestOutcome::Aborted);
        }
    }
}

pub async fn observe_request(
    State(observer): State<SharedObserver>,
    request: Request,
    next: Next,
) -> Response {
    let context = ObservationContext::for_request(RequestId::new());
    let route = match request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
    {
        Some("/health") => Route::Health,
        Some("/login") => Route::WebLogin,
        Some("/names" | "/names/table" | "/names/add") => Route::WebNames,
        Some("/names/{id}" | "/names/{id}/edit") => Route::WebName,
        Some("/names/bulk-add" | "/names/delete" | "/names/delete/table") => Route::WebBulk,
        Some("/names/export") => Route::WebExport,
        Some("/api/v1/login") => Route::ApiLogin,
        Some("/api/v1/names") => Route::ApiNames,
        Some("/api/v1/names/export") => Route::ApiExport,
        Some("/api/v1/names/{discord_id}/servers/{server_id}") => Route::ApiName,
        _ => Route::Other,
    };
    let method = match request.method().as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        _ => Method::Other,
    };
    let mut completion = Completion {
        observer,
        context: context.clone(),
        started: Instant::now(),
        route,
        method,
        completed: false,
    };
    let response = CONTEXT.scope(context, next.run(request)).await;
    completion.completed = true;
    completion.record(response.status().as_u16(), RequestOutcome::Completed);
    response
}
