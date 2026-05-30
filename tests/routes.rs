use axum::body::Body;
use axum::extract::Json as AxumJson;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_slope::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

// --- Computing mocks for every srvcs primitive this family composes over.
//
// Each reads its operands from the request body and returns the *real* answer,
// so the orchestration is genuinely exercised rather than fed a canned value.
// slope only calls `srvcs-floatsubtract` and `srvcs-floatdivide`; the rest are
// provided for completeness of the float family's contract.

/// `srvcs-floatadd`: reads `{a, b}` -> `{"result": a + b}` (as f64).
#[allow(dead_code)]
async fn spawn_floatadd() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a + b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatsubtract`: reads `{a, b}` -> `{"result": a - b}` (as f64).
async fn spawn_floatsubtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a - b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatmultiply`: reads `{a, b}` -> `{"result": a * b}` (as f64).
#[allow(dead_code)]
async fn spawn_floatmultiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// `srvcs-floatdivide`: reads `{a, b}` -> `{"result": a / b}` (as f64), but
/// rejects division by zero with `422` — exactly as the real primitive does on
/// a vertical line (`dx == 0`).
async fn spawn_floatdivide() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            if b == 0.0 {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "division by zero" })),
                );
            }
            (StatusCode::OK, Json(json!({ "result": a / b })))
        }),
    );
    serve(app).await
}

/// `srvcs-sqrt`: reads `{value}` -> `{"result": sqrt(value)}` (as f64).
#[allow(dead_code)]
async fn spawn_sqrt() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": value.sqrt() }))
        }),
    );
    serve(app).await
}

/// `srvcs-sin`: reads `{value}` -> `{"result": sin(value)}` (as f64).
#[allow(dead_code)]
async fn spawn_sin() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": value.sin() }))
        }),
    );
    serve(app).await
}

/// `srvcs-cos`: reads `{value}` -> `{"result": cos(value)}` (as f64).
#[allow(dead_code)]
async fn spawn_cos() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": value.cos() }))
        }),
    );
    serve(app).await
}

/// `srvcs-tan`: reads `{value}` -> `{"result": tan(value)}` (as f64).
#[allow(dead_code)]
async fn spawn_tan() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": value.tan() }))
        }),
    );
    serve(app).await
}

/// `srvcs-pi`: returns `{"result": PI}` for any body.
#[allow(dead_code)]
async fn spawn_pi() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|| async move { Json(json!({ "result": std::f64::consts::PI })) }),
    );
    serve(app).await
}

/// Spawn a mock returning a fixed status + body (used for error-path tests).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(floatsubtract_url: &str, floatdivide_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            floatsubtract_url: floatsubtract_url.to_string(),
            floatdivide_url: floatdivide_url.to_string(),
        },
    )
}

async fn slope(
    floatsubtract_url: &str,
    floatdivide_url: &str,
    x1: Value,
    y1: Value,
    x2: Value,
    y2: Value,
) -> (StatusCode, Value) {
    let res = app(floatsubtract_url, floatdivide_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "x1": x1, "y1": y1, "x2": x2, "y2": y2 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn result_f64(body: &Value) -> f64 {
    body["result"].as_f64().expect("result is a JSON number")
}

// --- Standard endpoints. ---

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.headers().contains_key("x-request-id"),
        "response must carry a generated x-request-id"
    );
}

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-slope");
    assert_eq!(body["concern"], "geometry: slope between two points");
    assert_eq!(
        body["depends_on"],
        json!(["srvcs-floatsubtract", "srvcs-floatdivide"])
    );
}

// --- Correctness cases, against the computing mocks. ---

#[tokio::test]
async fn slope_0_0_2_4_is_2() {
    let (s, d) = (spawn_floatsubtract().await, spawn_floatdivide().await);
    let (status, body) = slope(&s, &d, json!(0), json!(0), json!(2), json!(4)).await;
    assert_eq!(status, StatusCode::OK);
    // dy = 4 - 0 = 4; dx = 2 - 0 = 2; 4 / 2 = 2.0
    assert!((result_f64(&body) - 2.0).abs() < 1e-9);
    // The coordinates are echoed back verbatim.
    assert_eq!(body["x1"], json!(0));
    assert_eq!(body["y1"], json!(0));
    assert_eq!(body["x2"], json!(2));
    assert_eq!(body["y2"], json!(4));
}

#[tokio::test]
async fn slope_horizontal_line_is_zero() {
    let (s, d) = (spawn_floatsubtract().await, spawn_floatdivide().await);
    let (status, body) = slope(&s, &d, json!(-3), json!(5), json!(7), json!(5)).await;
    assert_eq!(status, StatusCode::OK);
    // dy = 5 - 5 = 0; dx = 7 - (-3) = 10; 0 / 10 = 0.0
    assert!(result_f64(&body).abs() < 1e-9);
}

#[tokio::test]
async fn slope_negative() {
    let (s, d) = (spawn_floatsubtract().await, spawn_floatdivide().await);
    let (status, body) = slope(&s, &d, json!(0), json!(4), json!(2), json!(0)).await;
    assert_eq!(status, StatusCode::OK);
    // dy = 0 - 4 = -4; dx = 2 - 0 = 2; -4 / 2 = -2.0
    assert!((result_f64(&body) + 2.0).abs() < 1e-9);
}

#[tokio::test]
async fn slope_fractional() {
    let (s, d) = (spawn_floatsubtract().await, spawn_floatdivide().await);
    let (status, body) = slope(&s, &d, json!(1.0), json!(1.0), json!(4.0), json!(2.0)).await;
    assert_eq!(status, StatusCode::OK);
    // dy = 2 - 1 = 1; dx = 4 - 1 = 3; 1 / 3 = 0.3333...
    assert!((result_f64(&body) - (1.0 / 3.0)).abs() < 1e-9);
}

// --- Error / edge cases. ---

#[tokio::test]
async fn vertical_line_forwards_422_from_floatdivide() {
    // dx == 0: floatsubtract succeeds, but floatdivide rejects division by zero
    // with a 422 that slope forwards verbatim.
    let (s, d) = (spawn_floatsubtract().await, spawn_floatdivide().await);
    let (status, body) = slope(&s, &d, json!(3), json!(1), json!(3), json!(9)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "division by zero");
}

#[tokio::test]
async fn forwards_422_from_floatsubtract() {
    // A non-numeric coordinate is rejected by floatsubtract; slope forwards it.
    let d = spawn_floatdivide().await;
    let s = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a number" }),
    )
    .await;
    let (status, body) = slope(&s, &d, json!(0), json!(0), json!("nope"), json!(4)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "value is not a number");
}

#[tokio::test]
async fn degrades_when_floatsubtract_unreachable() {
    let d = spawn_floatdivide().await;
    let (status, body) = slope(DEAD_URL, &d, json!(0), json!(0), json!(2), json!(4)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatsubtract");
}

#[tokio::test]
async fn degrades_when_floatdivide_unreachable() {
    // floatsubtract is reachable so both differences compute and the pipeline
    // reaches the floatdivide call, which then degrades.
    let s = spawn_floatsubtract().await;
    let (status, body) = slope(&s, DEAD_URL, json!(0), json!(0), json!(2), json!(4)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}

#[tokio::test]
async fn malformed_floatsubtract_result_is_500() {
    let d = spawn_floatdivide().await;
    let s = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let (status, body) = slope(&s, &d, json!(0), json!(0), json!(2), json!(4)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatsubtract");
}

#[tokio::test]
async fn malformed_floatdivide_result_is_500() {
    let s = spawn_floatsubtract().await;
    let d = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let (status, body) = slope(&s, &d, json!(0), json!(0), json!(2), json!(4)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}
