use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-slope";
pub const CONCERN: &str = "geometry: slope between two points";
pub const DEPENDS_ON: &[&str] = &["srvcs-floatsubtract", "srvcs-floatdivide"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub floatsubtract_url: String,
    pub floatdivide_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    /// The x-coordinate of the first point.
    #[schema(value_type = Object)]
    pub x1: Value,
    /// The y-coordinate of the first point.
    #[schema(value_type = Object)]
    pub y1: Value,
    /// The x-coordinate of the second point.
    #[schema(value_type = Object)]
    pub x2: Value,
    /// The y-coordinate of the second point.
    #[schema(value_type = Object)]
    pub y2: Value,
}

#[derive(Serialize, ToSchema)]
pub struct SlopeResponse {
    #[schema(value_type = Object)]
    pub x1: Value,
    #[schema(value_type = Object)]
    pub y1: Value,
    #[schema(value_type = Object)]
    pub x2: Value,
    #[schema(value_type = Object)]
    pub y2: Value,
    pub result: f64,
}

fn ok(req: &EvalRequest, result: f64) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "x1": req.x1,
            "y1": req.y1,
            "x2": req.x2,
            "y2": req.y2,
            "result": result,
        })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a numeric
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one dependency at `url` with `body`, mapping its outcome to either the
/// parsed response body (on `200`) or an early-return `Response` the caller
/// should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<Value, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body),
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute the slope of the line through two points.
///
/// This service owns the *control flow* but delegates every arithmetic step to
/// its dependencies, exactly as specified:
///
/// 1. `dy = floatsubtract(y2, y1)`;
/// 2. `dx = floatsubtract(x2, x1)`;
/// 3. `result = floatdivide(dy, dx)` — `srvcs-floatdivide` returns `422` on a
///    vertical line (`dx == 0`), which is forwarded verbatim.
///
/// Validation is not handled here: this service never calls `srvcs-isnumber`
/// directly. Its dependencies validate their own operands, and any `422` they
/// raise (a non-numeric coordinate, or a vertical line) is forwarded.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = SlopeResponse),
        (status = 422, description = "a dependency rejected an input, or the line is vertical (forwarded)"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    // 1. dy = floatsubtract(y2, y1).
    let dy_body = match ask(
        &deps.floatsubtract_url,
        &json!({ "a": req.y2, "b": req.y1 }),
        "srvcs-floatsubtract",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let dy = match dy_body.get("result").and_then(Value::as_f64) {
        Some(v) => v,
        None => return malformed("srvcs-floatsubtract"),
    };

    // 2. dx = floatsubtract(x2, x1).
    let dx_body = match ask(
        &deps.floatsubtract_url,
        &json!({ "a": req.x2, "b": req.x1 }),
        "srvcs-floatsubtract",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let dx = match dx_body.get("result").and_then(Value::as_f64) {
        Some(v) => v,
        None => return malformed("srvcs-floatsubtract"),
    };

    // 3. result = floatdivide(dy, dx). A vertical line (dx == 0) is rejected by
    //    floatdivide with a 422, which `ask` forwards verbatim.
    let div_body = match ask(
        &deps.floatdivide_url,
        &json!({ "a": dy, "b": dx }),
        "srvcs-floatdivide",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let result = match div_body.get("result").and_then(Value::as_f64) {
        Some(v) => v,
        None => return malformed("srvcs-floatdivide"),
    };

    ok(&req, result)
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, SlopeResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-slope");
        assert_eq!(info.concern, "geometry: slope between two points");
        assert_eq!(
            info.depends_on,
            vec!["srvcs-floatsubtract", "srvcs-floatdivide"]
        );
    }
}
