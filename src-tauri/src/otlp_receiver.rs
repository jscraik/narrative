use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::Utc;
use git2::{DiffOptions, Oid, Repository};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{
    any_value::Value as AnyValueKind, AnyValue, KeyValue,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    mem,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::{commands, git_diff};

const OTLP_PORT: u16 = 4318;
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const TRACE_EXTENSION: &str = ".agent-trace.json";
const TRACE_DIR: &str = "trace";

// Security: API key authentication for OTLP receiver
const API_KEY_HEADER: &str = "x-narrative-api-key";
// Default API key - users can override via environment variable
const DEFAULT_API_KEY: &str = "narrative-otel-dev-key-change-in-production";

// Security: Rate limiting configuration
const RATE_LIMIT_MAX_REQUESTS: u32 = 30;  // Max requests per window
const RATE_LIMIT_WINDOW_SECONDS: u64 = 1;  // 1 second sliding window

const COMMIT_KEYS: &[&str] = &[
    "commit_sha",
    "commitSha",
    "git.commit",
    "git.commit.id",
    "git.commit.sha",
    "git.commit_hash",
    "git.revision",
    "git.rev",
    "git.sha",
    "revision",
    "vcs.commit",
    "vcs.commit.id",
    "vcs.commit.sha",
    "vcs.revision",
];
const FILE_KEYS: &[&str] = &["file", "file_path", "path", "files", "file_paths"];
const MODEL_KEYS: &[&str] = &["model", "model_id", "codex.model", "openai.model"];
const CONVERSATION_KEYS: &[&str] = &[
    "conversation_id",
    "codex.conversation_id",
    "conversation.id",
];
const TOOL_VERSION_KEYS: &[&str] = &["app.version", "codex.version"];

#[derive(Clone, Default)]
pub struct OtelReceiverState {
    repo_root: Arc<Mutex<Option<String>>>,
    runtime: Arc<Mutex<Option<OtelReceiverRuntime>>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

#[derive(Clone)]
struct ReceiverContext {
    state: OtelReceiverState,
    app_handle: AppHandle,
}

struct OtelReceiverRuntime {
    shutdown: Option<oneshot::Sender<()>>,
}

// Simple in-memory rate limiter using a sliding window
#[derive(Default)]
struct RateLimiter {
    requests: Vec<Instant>,
}

impl RateLimiter {
    // Check if a request should be allowed based on rate limit
    // Returns true if allowed, false if rate limit exceeded
    fn check(&mut self) -> bool {
        let now = Instant::now();
        let window_start = now - Duration::from_secs(RATE_LIMIT_WINDOW_SECONDS);

        // Remove timestamps outside the current window
        self.requests.retain(|&t| t > window_start);

        // Check if under the limit
        if self.requests.len() < RATE_LIMIT_MAX_REQUESTS as usize {
            self.requests.push(now);
            true
        } else {
            false
        }
    }

    // Get current count for monitoring
    fn count(&self) -> usize {
        self.requests.len()
    }
}

#[derive(Clone)]
struct OtelEvent {
    timestamp_iso: String,
    attributes: HashMap<String, Vec<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverStatus {
    state: String,
    message: Option<String>,
    issues: Option<Vec<String>>,
    #[serde(rename = "lastSeenAtISO")]
    last_seen_at_iso: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestNotification {
    commit_shas: Vec<String>,
    records_written: usize,
    dropped: usize,
    issues: Vec<String>,
}

#[derive(Serialize)]
struct IngestResponse {
    accepted: usize,
    dropped: usize,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct TraceRecord {
    id: String,
    version: String,
    timestamp: String,
    vcs: TraceVcs,
    tool: Option<TraceTool>,
    files: Vec<TraceFile>,
    metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct TraceVcs {
    #[serde(rename = "type")]
    kind: String,
    revision: String,
}

#[derive(Serialize)]
struct TraceTool {
    name: Option<String>,
    version: Option<String>,
}

#[derive(Serialize)]
struct TraceFile {
    path: String,
    conversations: Vec<TraceConversation>,
}

#[derive(Serialize)]
struct TraceConversation {
    url: Option<String>,
    contributor: Option<TraceContributor>,
    ranges: Vec<TraceRange>,
}

#[derive(Serialize, Clone)]
struct TraceContributor {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "model_id")]
    model_id: Option<String>,
}

#[derive(Serialize)]
struct TraceRange {
    #[serde(rename = "start_line")]
    start_line: i64,
    #[serde(rename = "end_line")]
    end_line: i64,
    #[serde(rename = "content_hash")]
    content_hash: Option<String>,
    contributor: Option<TraceContributor>,
}

#[derive(Clone, Copy)]
enum OtelSignal {
    Logs,
    Traces,
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_active_repo_root(
    app_handle: AppHandle,
    state: tauri::State<OtelReceiverState>,
    repo_root: String,
) -> Result<(), String> {
    let mut guard = state.repo_root.lock().map_err(|e| e.to_string())?;
    *guard = Some(repo_root);

    let status = if is_receiver_running(state.inner()) {
        ReceiverStatus {
            state: "active".to_string(),
            message: Some("Listening for Codex logs...".to_string()),
            issues: None,
            last_seen_at_iso: None,
        }
    } else {
        ReceiverStatus {
            state: "inactive".to_string(),
            message: Some("Codex OTel receiver disabled".to_string()),
            issues: None,
            last_seen_at_iso: None,
        }
    };

    emit_status(&app_handle, status);

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_otlp_receiver_enabled(
    app_handle: AppHandle,
    state: tauri::State<OtelReceiverState>,
    enabled: bool,
) -> Result<(), String> {
    if enabled {
        start_otlp_receiver(app_handle, state.inner().clone())?;
    } else {
        stop_otlp_receiver(&app_handle, &state)?;
    }

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn run_otlp_smoke_test(
    app_handle: AppHandle,
    state: tauri::State<OtelReceiverState>,
    repo_root: String,
    commit_sha: String,
    file_paths: Vec<String>,
) -> Result<(), String> {
    let event = OtelEvent {
        timestamp_iso: Utc::now().to_rfc3339(),
        attributes: smoke_test_attributes(&commit_sha, &file_paths),
    };
    let context = ReceiverContext {
        state: state.inner().clone(),
        app_handle,
    };
    set_repo_root(&context.state, repo_root)?;
    ingest_events(&context, vec![event], OtelSignal::Traces)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn start_otlp_receiver(app_handle: AppHandle, state: OtelReceiverState) -> Result<(), String> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Atomically swap the runtime, handling any existing one
    // This fixes the race condition where concurrent calls could both pass is_some() check
    let old_runtime = {
        let mut guard = state.runtime.lock().map_err(|e| e.to_string())?;
        mem::replace(
            &mut *guard,
            Some(OtelReceiverRuntime {
                shutdown: Some(shutdown_tx),
            }),
        )
    };

    // Gracefully shut down any existing runtime
    if let Some(old_runtime) = old_runtime {
        if let Some(shutdown) = old_runtime.shutdown {
            let _ = shutdown.send(());
        }
    }

    let context = ReceiverContext {
        state: state.clone(),
        app_handle: app_handle.clone(),
    };

        tauri::async_runtime::spawn(async move {
            let runtime_state = context.state.clone();
            let router = Router::new()
                .route("/v1/logs", post(handle_logs))
                .route("/v1/traces", post(handle_traces))
                .with_state(context.clone());

            let addr = SocketAddr::from(([127, 0, 0, 1], OTLP_PORT));
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(err) => {
                    emit_status(
                        &context.app_handle,
                        ReceiverStatus {
                            state: "error".to_string(),
                            message: Some(format!("Codex OTel receiver failed to bind: {err}")),
                            issues: None,
                            last_seen_at_iso: None,
                        },
                    );
                    clear_receiver_runtime(&runtime_state);
                    return;
                }
            };

            emit_status(
                &context.app_handle,
                ReceiverStatus {
                    state: "active".to_string(),
                    message: Some("Listening for Codex logs...".to_string()),
                    issues: None,
                    last_seen_at_iso: None,
                },
            );

            let serve = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });

            if let Err(err) = serve.await {
                emit_status(
                    &context.app_handle,
                    ReceiverStatus {
                        state: "error".to_string(),
                        message: Some(format!("Codex OTel receiver stopped: {err}")),
                        issues: None,
                        last_seen_at_iso: None,
                    },
                );
            }

            clear_receiver_runtime(&runtime_state);
        });

    Ok(())
}

fn stop_otlp_receiver(
    app_handle: &AppHandle,
    state: &tauri::State<OtelReceiverState>,
) -> Result<(), String> {
    let mut guard = state.runtime.lock().map_err(|e| e.to_string())?;
    if let Some(runtime) = guard.take() {
        if let Some(shutdown) = runtime.shutdown {
            let _ = shutdown.send(());
        }
    }

    emit_status(
        app_handle,
        ReceiverStatus {
            state: "inactive".to_string(),
            message: Some("Codex OTel receiver disabled".to_string()),
            issues: None,
            last_seen_at_iso: None,
        },
    );

    Ok(())
}

// Get the expected API key from environment or use default
fn get_expected_api_key() -> String {
    std::env::var("NARRATIVE_OTEL_API_KEY")
        .unwrap_or_else(|_| DEFAULT_API_KEY.to_string())
}

// Validate API key from headers
fn validate_api_key(headers: &HeaderMap) -> Result<(), String> {
    let api_key = headers
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            format!(
                "Missing API key header: {API_KEY_HEADER}. \
                Set NARRATIVE_OTEL_API_KEY env var or use default key."
            )
        })?;

    let expected = get_expected_api_key();
    if api_key == expected {
        Ok(())
    } else {
        Err("Invalid API key".to_string())
    }
}

async fn handle_logs(
    State(context): State<ReceiverContext>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    handle_request(context, headers, body, OtelSignal::Logs).await
}

async fn handle_traces(
    State(context): State<ReceiverContext>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    handle_request(context, headers, body, OtelSignal::Traces).await
}

async fn handle_request(
    context: ReceiverContext,
    headers: HeaderMap,
    body: Bytes,
    signal: OtelSignal,
) -> impl IntoResponse {
    // Security: Validate API key first
    if let Err(err) = validate_api_key(&headers) {
        eprintln!("[OTLP Security] API key validation failed: {}", err);
        return response(
            StatusCode::UNAUTHORIZED,
            IngestResponse {
                accepted: 0,
                dropped: 0,
                errors: vec!["Unauthorized: Invalid or missing API key".to_string()],
            },
        );
    }

    // Security: Check rate limit
    {
        let rate_limiter = context.state.rate_limiter.lock().map_err(|e| e.to_string());
        let mut rate_limiter = match rate_limiter {
            Ok(rl) => rl,
            Err(err) => {
                eprintln!("[OTLP Security] Failed to acquire rate limiter lock: {}", err);
                return response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    IngestResponse {
                        accepted: 0,
                        dropped: 0,
                        errors: vec!["Internal server error".to_string()],
                    },
                );
            }
        };

        if !rate_limiter.check() {
            eprintln!(
                "[OTLP Security] Rate limit exceeded: {} requests in {} second window",
                rate_limiter.count(),
                RATE_LIMIT_WINDOW_SECONDS
            );
            return response(
                StatusCode::TOO_MANY_REQUESTS,
                IngestResponse {
                    accepted: 0,
                    dropped: 0,
                    errors: vec![format!(
                        "Rate limit exceeded: max {} requests per {} second",
                        RATE_LIMIT_MAX_REQUESTS, RATE_LIMIT_WINDOW_SECONDS
                    )],
                },
            );
        }
    }

    if body.len() > MAX_BODY_BYTES {
        return response(
            StatusCode::PAYLOAD_TOO_LARGE,
            IngestResponse {
                accepted: 0,
                dropped: 0,
                errors: vec!["Codex OTel payload too large".to_string()],
            },
        );
    }

    let events = match parse_otlp_events(&headers, &body, signal) {
        Ok(events) => events,
        Err(err) => {
            emit_status(
                &context.app_handle,
                ReceiverStatus {
                    state: "error".to_string(),
                    message: Some(err.clone()),
                    issues: None,
                    last_seen_at_iso: Some(Utc::now().to_rfc3339()),
                },
            );
            return response(
                StatusCode::BAD_REQUEST,
                IngestResponse {
                    accepted: 0,
                    dropped: 0,
                    errors: vec![err],
                },
            );
        }
    };

    match ingest_events(&context, events, signal) {
        Ok(outcome) => response(
            StatusCode::OK,
            IngestResponse {
                accepted: outcome.records_written,
                dropped: outcome.dropped,
                errors: outcome.issues,
            },
        ),
        Err(err) => response(
            StatusCode::INTERNAL_SERVER_ERROR,
            IngestResponse {
                accepted: 0,
                dropped: 0,
                errors: vec![err.to_string()],
            },
        ),
    }
}

fn response(status: StatusCode, payload: IngestResponse) -> impl IntoResponse {
    (status, Json(payload))
}

fn parse_otlp_events(
    headers: &HeaderMap,
    body: &Bytes,
    signal: OtelSignal,
) -> Result<Vec<OtelEvent>, String> {
    let is_json = header_is_json(headers);

    if is_json {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| format!("Invalid OTLP JSON payload: {e}"))?;
        return Ok(match signal {
            OtelSignal::Logs => otlp_logs_from_json(&value),
            OtelSignal::Traces => otlp_traces_from_json(&value),
        });
    }

    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        let events = match signal {
            OtelSignal::Logs => otlp_logs_from_json(&value),
            OtelSignal::Traces => otlp_traces_from_json(&value),
        };
        if !events.is_empty() {
            return Ok(events);
        }
    }

    match signal {
        OtelSignal::Logs => ExportLogsServiceRequest::decode(body.as_ref())
            .map(otlp_logs_from_proto)
            .map_err(|e| format!("Invalid OTLP logs protobuf: {e}")),
        OtelSignal::Traces => ExportTraceServiceRequest::decode(body.as_ref())
            .map(otlp_traces_from_proto)
            .map_err(|e| format!("Invalid OTLP traces protobuf: {e}")),
    }
}

fn header_is_json(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_lowercase())
        .map(|value| value.contains("json"))
        .unwrap_or(false)
}

fn otlp_logs_from_json(root: &Value) -> Vec<OtelEvent> {
    let mut events = Vec::new();
    let resource_logs = get_array(root, &["resourceLogs", "resource_logs"]);

    for resource_log in resource_logs {
        let resource_attrs = get_object(resource_log, &["resource"])
            .map(|resource| get_array(resource, &["attributes"]))
            .unwrap_or_default();
        let resource_map = attributes_from_json(resource_attrs);

        let scope_logs = get_array(resource_log, &["scopeLogs", "scope_logs"]);
        for scope_log in scope_logs {
            let records = get_array(scope_log, &["logRecords", "log_records"]);
            for record in records {
                let record_attrs = get_array(record, &["attributes", "attributes"]);
                let record_map = attributes_from_json(record_attrs);
                let merged = merge_attributes(&resource_map, &record_map);
                let timestamp = parse_time_iso(record, &["timeUnixNano", "time_unix_nano"]);
                events.push(OtelEvent {
                    timestamp_iso: timestamp,
                    attributes: merged,
                });
            }
        }
    }

    events
}

fn otlp_traces_from_json(root: &Value) -> Vec<OtelEvent> {
    let mut events = Vec::new();
    let resource_spans = get_array(root, &["resourceSpans", "resource_spans"]);

    for resource_span in resource_spans {
        let resource_attrs = get_object(resource_span, &["resource"])
            .map(|resource| get_array(resource, &["attributes"]))
            .unwrap_or_default();
        let resource_map = attributes_from_json(resource_attrs);

        let scope_spans = get_array(resource_span, &["scopeSpans", "scope_spans"]);
        for scope_span in scope_spans {
            let spans = get_array(scope_span, &["spans", "spans"]);
            for span in spans {
                let span_attrs = get_array(span, &["attributes", "attributes"]);
                let span_map = attributes_from_json(span_attrs);
                let merged = merge_attributes(&resource_map, &span_map);
                let timestamp =
                    parse_time_iso(span, &["startTimeUnixNano", "start_time_unix_nano"]);
                events.push(OtelEvent {
                    timestamp_iso: timestamp,
                    attributes: merged,
                });
            }
        }
    }

    events
}

fn parse_time_iso(value: &Value, keys: &[&str]) -> String {
    let nanos = keys
        .iter()
        .find_map(|key| value.get(*key))
        .and_then(parse_unix_nano);
    nanos
        .map(to_iso_from_nanos)
        .unwrap_or_else(|| Utc::now().to_rfc3339())
}

fn parse_unix_nano(value: &Value) -> Option<u64> {
    match value {
        Value::String(v) => v.parse::<u64>().ok(),
        Value::Number(v) => v.as_u64(),
        _ => None,
    }
}

fn get_array<'a>(value: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    for key in keys {
        if let Some(Value::Array(items)) = value.get(*key) {
            return items.iter().collect();
        }
    }
    Vec::new()
}

fn get_object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(val) = value.get(*key) {
            return Some(val);
        }
    }
    None
}

fn attributes_from_json(list: Vec<&Value>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();

    for item in list {
        let key = item.get("key").and_then(Value::as_str);
        let value = item.get("value");
        if let (Some(key), Some(value)) = (key, value) {
            let values = json_value_strings(value);
            if values.is_empty() {
                continue;
            }
            out.entry(key.to_string())
                .or_default()
                .extend(values.into_iter());
        }
    }

    out
}

fn json_value_strings(value: &Value) -> Vec<String> {
    if let Some(s) = value.get("stringValue").and_then(Value::as_str) {
        return vec![s.to_string()];
    }
    if let Some(n) = value.get("intValue").and_then(Value::as_i64) {
        return vec![n.to_string()];
    }
    if let Some(b) = value.get("boolValue").and_then(Value::as_bool) {
        return vec![b.to_string()];
    }
    if let Some(n) = value.get("doubleValue").and_then(Value::as_f64) {
        return vec![n.to_string()];
    }
    if let Some(array) = value.get("arrayValue") {
        if let Some(values) = array.get("values").and_then(Value::as_array) {
            return values.iter().flat_map(json_value_strings).collect();
        }
    }
    Vec::new()
}

fn otlp_logs_from_proto(payload: ExportLogsServiceRequest) -> Vec<OtelEvent> {
    let mut events = Vec::new();

    for resource_log in payload.resource_logs {
        let resource_map = attributes_from_resource(resource_log.resource);
        for scope_log in resource_log.scope_logs {
            for record in scope_log.log_records {
                let record_map = attributes_from_kv(&record.attributes);
                let merged = merge_attributes(&resource_map, &record_map);
                let timestamp = to_iso_from_nanos(record.time_unix_nano);
                events.push(OtelEvent {
                    timestamp_iso: timestamp,
                    attributes: merged,
                });
            }
        }
    }

    events
}

fn otlp_traces_from_proto(payload: ExportTraceServiceRequest) -> Vec<OtelEvent> {
    let mut events = Vec::new();

    for resource_span in payload.resource_spans {
        let resource_map = attributes_from_resource(resource_span.resource);
        for scope_span in resource_span.scope_spans {
            for span in scope_span.spans {
                let span_map = attributes_from_kv(&span.attributes);
                let merged = merge_attributes(&resource_map, &span_map);
                let timestamp = to_iso_from_nanos(span.start_time_unix_nano);
                events.push(OtelEvent {
                    timestamp_iso: timestamp,
                    attributes: merged,
                });
            }
        }
    }

    events
}

fn attributes_from_resource(resource: Option<Resource>) -> HashMap<String, Vec<String>> {
    resource
        .map(|resource| attributes_from_kv(&resource.attributes))
        .unwrap_or_default()
}

fn attributes_from_kv(attrs: &[KeyValue]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for attr in attrs {
        let values = attr
            .value
            .as_ref()
            .map(any_value_strings)
            .unwrap_or_default();
        if values.is_empty() {
            continue;
        }
        out.entry(attr.key.clone()).or_default().extend(values);
    }
    out
}

fn any_value_strings(value: &AnyValue) -> Vec<String> {
    let Some(inner) = &value.value else {
        return Vec::new();
    };

    match inner {
        AnyValueKind::StringValue(v) => vec![v.clone()],
        AnyValueKind::BoolValue(v) => vec![v.to_string()],
        AnyValueKind::IntValue(v) => vec![v.to_string()],
        AnyValueKind::DoubleValue(v) => vec![v.to_string()],
        AnyValueKind::ArrayValue(list) => list.values.iter().flat_map(any_value_strings).collect(),
        AnyValueKind::BytesValue(bytes) => vec![format!("bytes:{}", bytes.len())],
        _ => Vec::new(),
    }
}

fn merge_attributes(
    base: &HashMap<String, Vec<String>>,
    overlay: &HashMap<String, Vec<String>>,
) -> HashMap<String, Vec<String>> {
    let mut out = base.clone();
    for (key, values) in overlay {
        out.entry(key.clone())
            .or_default()
            .extend(values.iter().cloned());
    }
    out
}

fn to_iso_from_nanos(nanos: u64) -> String {
    if nanos == 0 {
        return Utc::now().to_rfc3339();
    }

    let secs = (nanos / 1_000_000_000) as i64;
    let sub = (nanos % 1_000_000_000) as u32;
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, sub).map(|dt| dt.to_rfc3339());
    dt.unwrap_or_else(|| Utc::now().to_rfc3339())
}

fn resolve_head_commit(repo_root: &str) -> Option<String> {
    let repo = Repository::open(repo_root).ok()?;
    let head = repo.head().ok()?;
    let target = head.target()?;
    Some(target.to_string())
}

fn ingest_events(
    context: &ReceiverContext,
    events: Vec<OtelEvent>,
    signal: OtelSignal,
) -> Result<IngestNotification, String> {
    let repo_root = active_repo_root(&context.state)?;
    commands::ensure_narrative_dirs(repo_root.clone())?;
    let fallback_commit = resolve_head_commit(&repo_root);

    let mut grouped: HashMap<String, Vec<OtelEvent>> = HashMap::new();
    let mut issues: Vec<String> = Vec::new();
    let mut dropped = 0;
    let mut missing_commit_count = 0;
    let mut fallback_note: Option<String> = None;

    for event in events {
        let commit = pick_first(&event.attributes, COMMIT_KEYS);
        if let Some(commit) = commit {
            grouped.entry(commit).or_default().push(event);
            continue;
        }

        missing_commit_count += 1;
        if let Some(fallback) = fallback_commit.as_deref() {
            grouped.entry(fallback.to_string()).or_default().push(event);
        } else {
            dropped += 1;
        }
    }

    let mut records_written = 0;
    let mut commit_shas = Vec::new();

    for (commit_sha, commit_events) in grouped {
        match build_trace_record(&repo_root, &commit_sha, &commit_events, signal) {
            Ok(record) => {
                let rel_path = write_trace_record(&repo_root, &record)?;
                records_written += 1;
                commit_shas.push(commit_sha);
                let _ = rel_path;
            }
            Err(err) => {
                dropped += 1;
                issues.push(format!("{commit_sha}: {err}"));
            }
        }
    }

    if missing_commit_count > 0 {
        if let Some(fallback) = fallback_commit.as_deref() {
            fallback_note = Some(format!(
                "{missing_commit_count} event(s) missing commit SHA; attributed to repo HEAD {fallback}"
            ));
        } else {
            issues.push(format!(
                "{missing_commit_count} event(s) missing commit SHA in Codex OTel attributes"
            ));
        }
    }

    let base_message = format!("Codex OTel ingest: wrote {records_written} record(s)");
    let active_message = fallback_note
        .as_ref()
        .map(|note| format!("{base_message}. {note}"))
        .unwrap_or(base_message);

    let status = if issues.is_empty() {
        ReceiverStatus {
            state: "active".to_string(),
            message: Some(active_message),
            issues: None,
            last_seen_at_iso: Some(Utc::now().to_rfc3339()),
        }
    } else {
        let mut message = format!("Codex OTel ingest completed with {} issue(s)", issues.len());
        if let Some(note) = fallback_note.as_ref() {
            message = format!("{message}. {note}");
        }
        ReceiverStatus {
            state: "partial".to_string(),
            message: Some(message),
            issues: Some(issues.clone()),
            last_seen_at_iso: Some(Utc::now().to_rfc3339()),
        }
    };

    emit_status(&context.app_handle, status);

    let notification = IngestNotification {
        commit_shas,
        records_written,
        dropped,
        issues,
    };

    if let Err(err) = context
        .app_handle
        .emit("otel-trace-ingested", &notification)
    {
        eprintln!("Failed to emit otel-trace-ingested: {err}");
    }

    Ok(notification)
}

fn build_trace_record(
    repo_root: &str,
    commit_sha: &str,
    events: &[OtelEvent],
    signal: OtelSignal,
) -> Result<TraceRecord, String> {
    let model_id = events
        .iter()
        .find_map(|event| pick_first(&event.attributes, MODEL_KEYS));
    let conversation_id = events
        .iter()
        .find_map(|event| pick_first(&event.attributes, CONVERSATION_KEYS));
    let tool_version = events
        .iter()
        .find_map(|event| pick_first(&event.attributes, TOOL_VERSION_KEYS));
    let file_hints = collect_file_hints(events);

    let files = build_trace_files(repo_root, commit_sha, &file_hints, model_id.clone())?;
    if files.is_empty() {
        return Err("No trace files generated for commit".to_string());
    }

    let metadata = serde_json::json!({
        "dev.narrative": {
            "derived": true,
            "source": match signal {
                OtelSignal::Logs => "otlp-log",
                OtelSignal::Traces => "otlp-trace",
            },
            "conversationId": conversation_id,
        }
    });

    let id_stamp = events
        .first()
        .map(|event| event.timestamp_iso.replace([':', '.'], "-"))
        .unwrap_or_else(|| Utc::now().to_rfc3339().replace([':', '.'], "-"));

    Ok(TraceRecord {
        id: format!("otlp-{}-{}", commit_sha, id_stamp),
        version: "0.1.0".to_string(),
        timestamp: events
            .first()
            .map(|event| event.timestamp_iso.clone())
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        vcs: TraceVcs {
            kind: "git".to_string(),
            revision: commit_sha.to_string(),
        },
        tool: Some(TraceTool {
            name: Some("codex".to_string()),
            version: tool_version,
        }),
        files,
        metadata: Some(metadata),
    })
}

fn build_trace_files(
    repo_root: &str,
    commit_sha: &str,
    file_hints: &[String],
    model_id: Option<String>,
) -> Result<Vec<TraceFile>, String> {
    let commit_files = list_commit_files(repo_root, commit_sha)?;
    let hint_set: HashSet<&String> = file_hints.iter().collect();
    let files_to_use = if hint_set.is_empty() {
        commit_files
    } else {
        commit_files
            .into_iter()
            .filter(|path| hint_set.contains(path))
            .collect()
    };

    let contributor = TraceContributor {
        kind: if model_id.is_some() {
            "ai".to_string()
        } else {
            "unknown".to_string()
        },
        model_id: model_id.clone(),
    };

    let mut trace_files = Vec::new();

    for path in files_to_use {
        let ranges = git_diff::get_commit_added_ranges(
            repo_root.to_string(),
            commit_sha.to_string(),
            path.clone(),
        )?;
        if ranges.is_empty() {
            continue;
        }

        let range_entries = ranges
            .into_iter()
            .map(|range| TraceRange {
                start_line: range.start,
                end_line: range.end,
                content_hash: None,
                contributor: Some(contributor.clone()),
            })
            .collect();

        trace_files.push(TraceFile {
            path,
            conversations: vec![TraceConversation {
                url: None,
                contributor: Some(contributor.clone()),
                ranges: range_entries,
            }],
        });
    }

    if trace_files.is_empty() {
        let fallback_path = file_hints
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        trace_files.push(TraceFile {
            path: fallback_path,
            conversations: vec![TraceConversation {
                url: None,
                contributor: Some(contributor.clone()),
                ranges: vec![TraceRange {
                    start_line: 1,
                    end_line: 1,
                    content_hash: None,
                    contributor: Some(contributor.clone()),
                }],
            }],
        });
    }

    Ok(trace_files)
}

fn list_commit_files(repo_root: &str, commit_sha: &str) -> Result<Vec<String>, String> {
    let repo = Repository::open(repo_root).map_err(|e| e.to_string())?;
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut options = DiffOptions::new();
    options.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut options))
        .map_err(|e| e.to_string())?;

    let mut paths = HashSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            paths.insert(path.to_string_lossy().to_string());
        }
    }

    Ok(paths.into_iter().collect())
}

fn write_trace_record(repo_root: &str, record: &TraceRecord) -> Result<String, String> {
    let file_name = format!(
        "{}_{}{}",
        record.timestamp.replace([':', '.'], "-"),
        record.id,
        TRACE_EXTENSION
    );
    let rel_path = format!("{TRACE_DIR}/{file_name}");
    let json = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    commands::write_narrative_file(repo_root.to_string(), rel_path.clone(), json)?;
    Ok(rel_path)
}

fn pick_first(attrs: &HashMap<String, Vec<String>>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(values) = attrs.get(*key) {
            if let Some(first) = values.first() {
                return Some(first.clone());
            }
        }
    }
    None
}

fn collect_file_hints(events: &[OtelEvent]) -> Vec<String> {
    let mut hints: Vec<String> = Vec::new();
    for event in events {
        for key in FILE_KEYS {
            if let Some(values) = event.attributes.get(*key) {
                for value in values {
                    if !value.trim().is_empty() && !hints.contains(value) {
                        hints.push(value.trim().to_string());
                    }
                }
            }
        }
    }
    hints
}

fn emit_status(app_handle: &AppHandle, status: ReceiverStatus) {
    if let Err(err) = app_handle.emit("otel-receiver-status", status) {
        eprintln!("Failed to emit otel-receiver-status: {err}");
    }
}

fn is_receiver_running(state: &OtelReceiverState) -> bool {
    state
        .runtime
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

fn clear_receiver_runtime(state: &OtelReceiverState) {
    if let Ok(mut guard) = state.runtime.lock() {
        *guard = None;
    }
}

fn active_repo_root(state: &OtelReceiverState) -> Result<String, String> {
    state
        .repo_root
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "No active repo root set for Codex OTel receiver".to_string())
}

fn set_repo_root(state: &OtelReceiverState, repo_root: String) -> Result<(), String> {
    let mut guard = state.repo_root.lock().map_err(|e| e.to_string())?;
    *guard = Some(repo_root);
    Ok(())
}

fn smoke_test_attributes(commit_sha: &str, file_paths: &[String]) -> HashMap<String, Vec<String>> {
    let mut attrs = HashMap::new();
    attrs.insert("commit_sha".to_string(), vec![commit_sha.to_string()]);
    if !file_paths.is_empty() {
        attrs.insert("file_paths".to_string(), file_paths.to_vec());
    }
    attrs.insert("model_id".to_string(), vec!["smoke-test".to_string()]);
    attrs
}
