use crate::recovery_checkpoint::{
    begin_fresh_retry, checkpoint_from_thread_snapshot_result, extract_optional_string,
    load_recovery_checkpoint, new_recovery_checkpoint, requires_fresh_retry,
    upsert_recovery_checkpoint, RecoveryCheckpoint, TRUST_PAUSE_REASON_HYDRATE_FAILED,
    TRUST_PAUSE_REASON_SNAPSHOT_TIMEOUT, TRUST_PAUSE_REASON_THREAD_MISMATCH,
};
use crate::secret_store;
use rand::{distr::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{command, AppHandle, Emitter, Manager, State};

const RESTART_BUDGET: usize = 3;
const RESTART_WINDOW_SECS: i64 = 60;
const MAX_DEDUPE_LOG: usize = 200;
const MAX_DEDUPE_SOURCES: usize = 10_000;
const MAX_TRANSITIONS: usize = 100;
const MONITOR_POLL_INTERVAL_MS: u64 = 250;
const RESTART_COOLDOWN_MS: u64 = 500;
const SHUTDOWN_GRACE_MS: u64 = 1_500;
const DEFAULT_LIVE_SESSIONS_TTL_HOURS: i64 = 72;
const DEFAULT_LIVE_SESSIONS_MAX_ROWS: i64 = 10_000;
const APP_SERVER_PROTOCOL_TARGET: &str = "v2";

// Reserved for future reconnect validation (see P2-039 agent trust state recovery)
// These will be used when codex_app_server_receive_live_event is wired for live reconnection.
#[allow(dead_code)]
const RECONNECT_REASON_SCHEMA_MISMATCH: &str = "schema_version_mismatch";
#[allow(dead_code)]
const RECONNECT_REASON_SESSION_INVALID: &str = "session_invalid";
#[allow(dead_code)]
const RECONNECT_REASON_TOKEN_INVALID: &str = "token_invalid";
#[allow(dead_code)]
const APP_SERVER_SCHEMA_SUPPORTED: &[&str] = &["v1"];
#[allow(dead_code)]
const APP_SERVER_SCHEMA_DEPRECATED: &[&str] = &[];
const RPC_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MAX_PENDING_RPC_REQUESTS: usize = 256;
const MAX_PENDING_APPROVAL_WAITERS: usize = 128;
const MAX_SIDECAR_JSONL_BYTES: usize = 256 * 1024;
const MAX_SIDECAR_JSON_DEPTH: usize = 32;
const MAX_SIDECAR_STDERR_LINES: usize = 256;
const MAX_APPROVAL_DECISIONS: usize = 1_000;
const APPROVAL_TOKEN_BYTES: usize = 32;
const MAIN_WINDOW_LABEL: &str = "main";
const SUPPORTED_AUTH_MODES: &[&str] = &["apikey", "chatgpt", "chatgptAuthTokens"];
const BLOCKED_SIDECAR_ENV_OVERRIDES: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
];
const SIDECAR_MANIFEST_FILE: &str = "codex-app-server-manifest.json";
const SIDECAR_MANIFEST_SIGNATURE_SALT: &str = "narrative-codex-sidecar-signature-v1";
// SHA-256 of the canonical manifest file, baked into the binary so that
// an attacker who can modify the manifest file (but not the code-signed binary)
// cannot forge a trusted signature by recomputing payloadHash + signature.
// Update this constant (and the matching JS constant) when the manifest changes.
const SIDECAR_MANIFEST_FILE_SHA256: &str =
    "8ec2075e5807a875cdf4979d8621f03090178997af55f7f87f955db3777b017a";
const SIDECAR_MANIFEST_SCHEMA_VERSION: u64 = 1;
const SIDECAR_MANIFEST_MIN_VERSION_FLOOR: u64 = 2026022501;
const SIDECAR_MINIMUM_VERSION_FLOOR: &str = "0.97.0";
const TRUSTED_SIDECAR_SIGNER_IDS: &[&str] = &[
    "narrative-codex-sidecar-2026q1",
    "narrative-codex-sidecar-2026q2",
];
const REVOKED_SIDECAR_SIGNER_IDS: &[&str] = &["narrative-codex-sidecar-2025q4"];
const SIDECAR_OVERRIDE_ENV: &str = "NARRATIVE_CODEX_APP_SERVER_BIN";
const SIDECAR_FORCE_PRODUCTION_ENV: &str = "NARRATIVE_CODEX_APP_SERVER_FORCE_PRODUCTION";
const SIDECAR_FORCE_DEVELOPMENT_ENV: &str = "NARRATIVE_CODEX_APP_SERVER_FORCE_DEVELOPMENT";
const METHOD_INITIALIZE: &str = "initialize";
const METHOD_INITIALIZED: &str = "initialized";
const METHOD_ACCOUNT_READ: &str = "account/read";
const METHOD_ACCOUNT_LOGIN_START: &str = "account/login/start";
const METHOD_ACCOUNT_CHATGPT_TOKENS_REFRESH: &str = "account/chatgptAuthTokens/refresh";
const METHOD_THREAD_READ: &str = "thread/read";
const METHOD_APPROVAL_SUBMIT: &str = "approval/submit";
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const JSONRPC_OVERLOADED: i64 = -32001;
const ALLOWED_SIDECAR_NOTIFICATION_METHODS: &[&str] = &[
    "account/updated",
    "account/login/completed",
    "item/commandExecution/requestApproval",
    "item/fileChange/requestApproval",
    "item/started",
    "item/completed",
    "item/agentMessage/delta",
    "turn/started",
    "turn/completed",
];
// Reserved for capability negotiation when connecting to app server (P2-039)
#[allow(dead_code)]
const REQUIRED_APP_SERVER_METHODS: &[&str] = &[
    METHOD_INITIALIZE,
    METHOD_INITIALIZED,
    METHOD_ACCOUNT_READ,
    METHOD_ACCOUNT_LOGIN_START,
    METHOD_ACCOUNT_CHATGPT_TOKENS_REFRESH,
    METHOD_THREAD_READ,
    METHOD_APPROVAL_SUBMIT,
];

pub const LIVE_SESSION_EVENT: &str = "session:live:event";

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const SIDECAR_TARGET_TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const SIDECAR_TARGET_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SIDECAR_TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const SIDECAR_TARGET_TRIPLE: &str = "aarch64-unknown-linux-gnu";
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const SIDECAR_TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
const SIDECAR_TARGET_TRIPLE: &str = "unsupported-target";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarArtifactManifest {
    target: String,
    file: String,
    sha256: String,
    sidecar_version: String,
    minimum_sidecar_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarBinaryManifest {
    schema_version: u64,
    manifest_version: u64,
    minimum_manifest_version: u64,
    minimum_sidecar_version: String,
    active_signer: String,
    payload_hash: String,
    signature: String,
    artifacts: Vec<SidecarArtifactManifest>,
}

/// Schema version classification for reconnect validation (reserved, P2-039)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SchemaVersionPolicy {
    Supported,
    Deprecated,
    Rejected,
}

#[derive(Clone, Default)]
pub struct CodexAppServerState {
    inner: Arc<Mutex<CodexAppServerRuntime>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessState {
    Inactive,
    Starting,
    Running,
    Degraded,
    CrashLoop,
    Stopping,
}

impl ProcessState {
    fn as_status(self) -> &'static str {
        match self {
            ProcessState::Inactive => "inactive",
            ProcessState::Starting => "starting",
            ProcessState::Running => "running",
            ProcessState::Degraded => "degraded",
            ProcessState::CrashLoop => "crash_loop",
            ProcessState::Stopping => "stopping",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthState {
    NeedsLogin,
    Authenticating,
    Authenticated,
    LoggedOut,
}

impl AuthState {
    fn as_status(self) -> &'static str {
        match self {
            AuthState::NeedsLogin => "needs_login",
            AuthState::Authenticating => "authenticating",
            AuthState::Authenticated => "authenticated",
            AuthState::LoggedOut => "logged_out",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandshakeState {
    NotStarted,
    InitializeSent,
    Initialized,
}

impl HandshakeState {
    fn initialize_sent(self) -> bool {
        matches!(
            self,
            HandshakeState::InitializeSent | HandshakeState::Initialized
        )
    }

    fn initialized(self) -> bool {
        matches!(self, HandshakeState::Initialized)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamSessionState {
    Disabled,
    Expected,
    Alive,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppServerStatus {
    pub state: String, // inactive | starting | running | degraded | crash_loop | error | stopping
    pub initialized: bool,
    pub initialize_sent: bool,
    pub auth_state: String, // needs_login | authenticating | authenticated | logged_out
    pub auth_mode: String,
    pub stream_healthy: bool,
    pub stream_kill_switch: bool,
    pub restart_budget: usize,
    pub restart_attempts_in_window: usize,
    pub last_error: Option<String>,
    pub last_transition_at_iso: Option<String>,
}

impl Default for CodexAppServerStatus {
    fn default() -> Self {
        Self {
            state: "inactive".to_string(),
            initialized: false,
            initialize_sent: false,
            auth_state: "needs_login".to_string(),
            auth_mode: "chatgpt".to_string(),
            stream_healthy: false,
            stream_kill_switch: false,
            restart_budget: RESTART_BUDGET,
            restart_attempts_in_window: 0,
            last_error: None,
            last_transition_at_iso: Some(now_iso()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountStatus {
    pub auth_state: String,
    pub auth_mode: String,
    pub interactive_login_required: bool,
    pub supported_modes: Vec<String>,
}

/// Recovery checkpoint status for a thread, used at startup/restart to determine
/// trust state before hydrating. Call this after handshake completes to check
/// whether a prior session left recoverable state or requires fresh retry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexThreadRecoveryCheckpointStatus {
    /// The thread ID queried
    pub thread_id: String,
    /// Whether a durable checkpoint exists for this thread
    pub checkpoint_exists: bool,
    /// Whether the checkpoint requires fresh retry (incompatible, corrupted, or prior failure)
    pub requires_fresh_retry: bool,
    /// Trust state recommendation: "none" | "hydrating" | "replaying" | "live_trusted" | "trust_paused"
    /// At startup, this will be "none" (no checkpoint) or "trust_paused" (checkpoint requires fresh retry)
    pub trust_state_recommendation: String,
    /// The checkpoint details if one exists
    pub checkpoint: Option<RecoveryCheckpoint>,
    /// Reason code if requires_fresh_retry is true
    pub fresh_retry_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStreamEventInput {
    pub provider: String,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub event_type: String,
    pub source: String, // otel | app_server_stream
    #[allow(dead_code)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStreamIngestResult {
    pub key: String,
    pub decision: String, // accepted | duplicate | replaced | dropped
    pub chosen_source: String,
    pub replaced_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexStreamDedupeDecision {
    pub at_iso: String,
    pub key: String,
    pub decision: String,
    pub incoming_source: String,
    pub chosen_source: String,
    pub replaced_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReliabilityMetrics {
    pub stream_events_accepted: u64,
    pub stream_events_duplicates: u64,
    pub stream_events_dropped: u64,
    pub stream_events_replaced: u64,
    pub parser_validation_errors_total: u64,
    pub rpc_timeouts_total: u64,
    pub approval_timeouts_total: u64,
    pub restart_events_total: u64,
    pub pending_rpcs: usize,
    pub pending_approvals: usize,
    pub sidecar_stderr_buffered: usize,
    pub sidecar_stderr_dropped: u64,
    pub time_since_last_stream_event_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureModeTransition {
    pub at_iso: String,
    pub from_mode: Option<String>,
    pub to_mode: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReliabilityStatus {
    pub mode: String, // OTEL_ONLY | HYBRID_ACTIVE | DEGRADED_STREAMING | FAILURE
    pub otel_baseline_healthy: bool,
    pub stream_expected: bool,
    pub stream_healthy: bool,
    pub reasons: Vec<String>,
    pub metrics: CaptureReliabilityMetrics,
    pub transitions: Vec<CaptureModeTransition>,
    pub app_server: CodexAppServerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LiveSessionEventPayload {
    #[serde(rename_all = "camelCase")]
    SessionDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        event_type: String,
        source: String,
        sequence_id: u64,
        received_at_iso: String,
        payload: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    ApprovalRequest {
        request_id: String,
        thread_id: String,
        turn_id: String,
        command: String,
        options: Vec<String>,
        timeout_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rpc_request_id: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_token: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ApprovalResult {
        request_id: String,
        thread_id: String,
        approved: bool,
        decided_at_iso: String,
        decided_by: Option<String>,
        reason: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ParserValidationError {
        kind: String, // schema_mismatch | missing_fields | protocol_violation
        raw_preview: String,
        reason: String,
        occurred_at_iso: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionsCleanupResult {
    pub removed_rows: u64,
    pub ttl_hours: i64,
    pub max_rows: i64,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    thread_id: String,
    created_at_epoch_ms: i64,
    timeout_ms: u64,
    decision_token: String,
    approval_window_id: u64,
    rpc_request_id: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
enum PendingRpcKind {
    Initialize,
    AccountRead,
    AccountLoginStart,
    AccountChatgptTokensRefresh,
    ThreadRead,
}

#[derive(Debug, Clone)]
struct PendingRpcRequest {
    kind: PendingRpcKind,
    sent_at_epoch_ms: i64,
    timeout_ms: u64,
}

#[derive(Debug, Clone)]
struct ApprovalDecisionRecord {
    decision_token_hash: String,
    approval_window_id: u64,
}

struct SidecarProcess {
    child: Child,
}

struct CodexAppServerRuntime {
    status: CodexAppServerStatus,
    process_state: ProcessState,
    auth_state: AuthState,
    handshake_state: HandshakeState,
    stream_session_state: StreamSessionState,
    restart_attempts: VecDeque<i64>,
    sidecar_path: Option<PathBuf>,
    sidecar: Option<SidecarProcess>,
    monitor_cancel: Arc<AtomicBool>,
    monitor_handle: Option<thread::JoinHandle<()>>,
    dedupe_sources: HashMap<String, String>,
    dedupe_key_order: VecDeque<String>,
    recent_dedupe: VecDeque<CodexStreamDedupeDecision>,
    stream_events_accepted: u64,
    stream_events_duplicates: u64,
    stream_events_dropped: u64,
    stream_events_replaced: u64,
    transitions: VecDeque<CaptureModeTransition>,
    last_mode: Option<String>,
    approval_waiters: HashMap<String, PendingApproval>,
    approval_decisions: HashMap<String, ApprovalDecisionRecord>,
    approval_decision_order: VecDeque<String>,
    parse_error_total: HashMap<String, u64>,
    approval_result_total: HashMap<String, u64>,
    restart_total: HashMap<String, u64>,
    event_sequence_id: u64,
    next_rpc_id: i64,
    pending_rpcs: HashMap<String, PendingRpcRequest>,
    sidecar_stderr_ring: VecDeque<String>,
    sidecar_stderr_dropped: u64,
    last_stream_event_at_epoch_ms: Option<i64>,
    approval_window_id: u64,
    thread_read_results: HashMap<String, serde_json::Value>,
    thread_read_errors: HashMap<String, String>,
}

impl Default for CodexAppServerRuntime {
    fn default() -> Self {
        Self {
            status: CodexAppServerStatus::default(),
            process_state: ProcessState::Inactive,
            auth_state: AuthState::NeedsLogin,
            handshake_state: HandshakeState::NotStarted,
            stream_session_state: StreamSessionState::Disabled,
            restart_attempts: VecDeque::new(),
            sidecar_path: None,
            sidecar: None,
            monitor_cancel: Arc::new(AtomicBool::new(false)),
            monitor_handle: None,
            dedupe_sources: HashMap::new(),
            dedupe_key_order: VecDeque::new(),
            recent_dedupe: VecDeque::new(),
            stream_events_accepted: 0,
            stream_events_duplicates: 0,
            stream_events_dropped: 0,
            stream_events_replaced: 0,
            transitions: VecDeque::new(),
            last_mode: None,
            approval_waiters: HashMap::new(),
            approval_decisions: HashMap::new(),
            approval_decision_order: VecDeque::new(),
            parse_error_total: HashMap::new(),
            approval_result_total: HashMap::new(),
            restart_total: HashMap::new(),
            event_sequence_id: 0,
            next_rpc_id: 1,
            pending_rpcs: HashMap::new(),
            sidecar_stderr_ring: VecDeque::new(),
            sidecar_stderr_dropped: 0,
            last_stream_event_at_epoch_ms: None,
            approval_window_id: rand::rng().random(),
            thread_read_results: HashMap::new(),
            thread_read_errors: HashMap::new(),
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_epoch_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn normalize_source(input: &str) -> String {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "app_server_stream" | "appserver" | "stream" => "app_server_stream".to_string(),
        "otel" | "codex_otel" => "otel".to_string(),
        _ => normalized,
    }
}

fn normalize_auth_mode(input: &str) -> String {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "" | "none" | "null" => "none".to_string(),
        "api_key" | "apikey" => "apikey".to_string(),
        "chatgpt" => "chatgpt".to_string(),
        "chatgptauthtokens" | "chatgpt_auth_tokens" => "chatgptAuthTokens".to_string(),
        _ => normalized,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_lowercase();
    normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("authorization")
        || normalized.contains("apikey")
        || normalized.contains("api_key")
        || normalized.contains("authurl")
}

fn redact_sensitive_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(redact_sensitive_json)
                .collect::<Vec<serde_json::Value>>(),
        ),
        serde_json::Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in map {
                if is_sensitive_key(key) {
                    redacted.insert(
                        key.clone(),
                        serde_json::Value::String("[REDACTED]".to_string()),
                    );
                } else {
                    redacted.insert(key.clone(), redact_sensitive_json(value));
                }
            }
            serde_json::Value::Object(redacted)
        }
        _ => value.clone(),
    }
}

fn format_redacted_json(value: &serde_json::Value) -> String {
    serde_json::to_string(&redact_sensitive_json(value))
        .unwrap_or_else(|_| "<unserializable>".to_string())
}

fn redact_sensitive_stderr_line(line: &str) -> String {
    let normalized = line.to_lowercase();
    if normalized.contains("token")
        || normalized.contains("authorization")
        || normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized.contains("secret")
    {
        "[REDACTED STDERR LINE: potential credential material]".to_string()
    } else {
        line.to_string()
    }
}

fn validate_and_redact_auth_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") {
        return Err("authUrl must use https scheme".to_string());
    }

    let host_and_path = &trimmed["https://".len()..];
    let host = host_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if host.is_empty() {
        return Err("authUrl missing host".to_string());
    }

    let host_lc = host.to_lowercase();
    let host_allowed = matches!(
        host_lc.as_str(),
        "openai.com" | "chatgpt.com" | "auth.openai.com" | "chat.openai.com"
    ) || host_lc.ends_with(".openai.com")
        || host_lc.ends_with(".chatgpt.com");
    if !host_allowed {
        return Err(format!("authUrl host is not allowlisted: {host}"));
    }

    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    Ok(without_query.to_string())
}

fn generate_approval_window_id() -> u64 {
    rand::rng().random()
}

fn generate_approval_decision_token() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(APPROVAL_TOKEN_BYTES)
        .map(char::from)
        .collect()
}

fn hash_opaque_token(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn auth_mode_to_login_start_type(auth_mode: &str) -> Option<&'static str> {
    match auth_mode {
        "apikey" => Some("apiKey"),
        "chatgpt" => Some("chatgpt"),
        "chatgptAuthTokens" => Some("chatgptAuthTokens"),
        _ => None,
    }
}

fn supported_auth_modes() -> Vec<String> {
    SUPPORTED_AUTH_MODES
        .iter()
        .map(|mode| mode.to_string())
        .collect()
}

fn is_supported_auth_mode(auth_mode: &str) -> bool {
    SUPPORTED_AUTH_MODES.contains(&auth_mode)
}

fn event_identity_key(input: &CodexStreamEventInput) -> String {
    format!(
        "v1|{}|{}|{}|{}|{}",
        input.provider.trim().to_lowercase(),
        input.thread_id.trim(),
        input.turn_id.trim(),
        input.item_id.trim(),
        input.event_type.trim().to_lowercase(),
    )
}

fn source_priority(event_type: &str, source: &str) -> i32 {
    let event_type = event_type.to_lowercase();
    if event_type.contains("delta") {
        return if source == "app_server_stream" {
            30
        } else {
            10
        };
    }
    if event_type.contains("completed") {
        return if source == "otel" { 30 } else { 20 };
    }
    if source == "app_server_stream" {
        20
    } else {
        10
    }
}

fn increment_labeled_counter(counter: &mut HashMap<String, u64>, key: &str) {
    let entry = counter.entry(key.to_string()).or_insert(0);
    *entry += 1;
}

fn counter_total(counter: &HashMap<String, u64>) -> u64 {
    counter.values().copied().sum()
}

fn sync_status(runtime: &mut CodexAppServerRuntime) {
    runtime.status.state = runtime.process_state.as_status().to_string();
    runtime.status.auth_state = runtime.auth_state.as_status().to_string();
    runtime.status.initialize_sent = runtime.handshake_state.initialize_sent();
    runtime.status.initialized = runtime.handshake_state.initialized();
    runtime.status.restart_attempts_in_window = runtime.restart_attempts.len();
    runtime.status.restart_budget = RESTART_BUDGET;
    runtime.status.last_transition_at_iso = Some(now_iso());
}

fn set_process_state(
    runtime: &mut CodexAppServerRuntime,
    process_state: ProcessState,
    last_error: Option<String>,
) {
    runtime.process_state = process_state;
    runtime.status.last_error = last_error;
    sync_status(runtime);
}

fn emit_status(app_handle: &AppHandle, status: &CodexAppServerStatus) {
    let _ = app_handle.emit_to(MAIN_WINDOW_LABEL, "codex-app-server-status", status.clone());
}

fn emit_live_session_event(app_handle: &AppHandle, payload: &LiveSessionEventPayload) {
    let _ = app_handle.emit_to(MAIN_WINDOW_LABEL, LIVE_SESSION_EVENT, payload.clone());
}

fn is_truthy_env(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn sidecar_is_production_mode() -> bool {
    if is_truthy_env(SIDECAR_FORCE_DEVELOPMENT_ENV) {
        return false;
    }
    if is_truthy_env(SIDECAR_FORCE_PRODUCTION_ENV) {
        return true;
    }
    !cfg!(debug_assertions)
}

fn sidecar_override_matches(path: &Path) -> bool {
    let Ok(override_path) = std::env::var(SIDECAR_OVERRIDE_ENV) else {
        return false;
    };
    if override_path.trim().is_empty() {
        return false;
    }
    PathBuf::from(override_path.trim()) == path
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn parse_version_tuple(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.trim().split('.');
    let major = parts
        .next()?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    let minor = parts
        .next()?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    let patch = parts
        .next()?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

fn version_at_least(current: &str, minimum: &str) -> bool {
    match (parse_version_tuple(current), parse_version_tuple(minimum)) {
        (Some(current_parts), Some(minimum_parts)) => current_parts >= minimum_parts,
        _ => false,
    }
}

fn compute_manifest_payload_hash(manifest: &SidecarBinaryManifest) -> String {
    let mut artifact_rows = manifest
        .artifacts
        .iter()
        .map(|artifact| {
            format!(
                "{}|{}|{}|{}|{}",
                artifact.target,
                artifact.file,
                artifact.sha256.to_ascii_lowercase(),
                artifact.sidecar_version,
                artifact
                    .minimum_sidecar_version
                    .as_deref()
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    artifact_rows.sort_unstable();

    let payload = format!(
        "schemaVersion={}\nmanifestVersion={}\nminimumManifestVersion={}\nminimumSidecarVersion={}\nactiveSigner={}\n{}\n",
        manifest.schema_version,
        manifest.manifest_version,
        manifest.minimum_manifest_version,
        manifest.minimum_sidecar_version,
        manifest.active_signer,
        artifact_rows.join("\n"),
    );
    sha256_hex(payload.as_bytes())
}

fn compute_manifest_signature(payload_hash: &str, signer: &str) -> String {
    let raw = format!(
        "payloadHash={payload_hash}|signer={signer}|salt={SIDECAR_MANIFEST_SIGNATURE_SALT}"
    );
    sha256_hex(raw.as_bytes())
}

fn verify_sidecar_manifest_for_path(sidecar_path: &Path) -> Result<(), String> {
    let manifest_path = locate_sidecar_manifest_path(sidecar_path).ok_or_else(|| {
        "Failed to locate codex-app-server manifest in supported bundle/runtime locations"
            .to_string()
    })?;

    let raw_manifest = std::fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "Failed to read sidecar manifest at {}: {error}",
            manifest_path.display()
        )
    })?;

    let manifest_file_hash = sha256_hex(raw_manifest.as_bytes());
    if manifest_file_hash != SIDECAR_MANIFEST_FILE_SHA256 {
        return Err(format!(
            "Sidecar manifest file digest mismatch (expected {}, got {})",
            SIDECAR_MANIFEST_FILE_SHA256, manifest_file_hash
        ));
    }

    let manifest: SidecarBinaryManifest = serde_json::from_str(&raw_manifest).map_err(|error| {
        format!(
            "Failed to parse sidecar manifest at {}: {error}",
            manifest_path.display()
        )
    })?;

    if manifest.schema_version != SIDECAR_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported sidecar manifest schema: {}",
            manifest.schema_version
        ));
    }
    if manifest.manifest_version < SIDECAR_MANIFEST_MIN_VERSION_FLOOR {
        return Err(format!(
            "Sidecar manifest version {} is below build floor {}",
            manifest.manifest_version, SIDECAR_MANIFEST_MIN_VERSION_FLOOR
        ));
    }
    if manifest.manifest_version < manifest.minimum_manifest_version {
        return Err(format!(
            "Sidecar manifest version {} is below minimum {}",
            manifest.manifest_version, manifest.minimum_manifest_version
        ));
    }
    if !TRUSTED_SIDECAR_SIGNER_IDS.contains(&manifest.active_signer.as_str()) {
        return Err(format!(
            "Active sidecar signer is not trusted: {}",
            manifest.active_signer
        ));
    }
    if REVOKED_SIDECAR_SIGNER_IDS.contains(&manifest.active_signer.as_str()) {
        return Err(format!(
            "Active sidecar signer is revoked: {}",
            manifest.active_signer
        ));
    }

    let payload_hash = compute_manifest_payload_hash(&manifest);
    if payload_hash != manifest.payload_hash {
        return Err("Manifest payload hash mismatch".to_string());
    }
    let expected_signature = compute_manifest_signature(&payload_hash, &manifest.active_signer);
    if expected_signature != manifest.signature {
        return Err("Manifest signature mismatch".to_string());
    }

    let file_name = sidecar_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Sidecar file name is invalid UTF-8".to_string())?;

    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.target == SIDECAR_TARGET_TRIPLE && artifact.file == file_name)
        .or_else(|| {
            if SIDECAR_TARGET_TRIPLE == "unsupported-target" {
                manifest
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.file == file_name)
                    .or_else(|| {
                        manifest
                            .artifacts
                            .iter()
                            .find(|artifact| artifact.target == "generic" && artifact.file == file_name)
                    })
            } else {
                None
            }
        })
        .ok_or_else(|| {
            format!(
                "Sidecar manifest missing artifact entry for target {SIDECAR_TARGET_TRIPLE} and file {file_name}"
            )
        })?;

    if !version_at_least(&artifact.sidecar_version, &manifest.minimum_sidecar_version) {
        return Err(format!(
            "Sidecar version {} is below manifest minimum {}",
            artifact.sidecar_version, manifest.minimum_sidecar_version
        ));
    }
    if let Some(minimum_artifact_version) = artifact.minimum_sidecar_version.as_deref() {
        if !version_at_least(&artifact.sidecar_version, minimum_artifact_version) {
            return Err(format!(
                "Sidecar version {} is below artifact minimum {}",
                artifact.sidecar_version, minimum_artifact_version
            ));
        }
    }
    if !version_at_least(&artifact.sidecar_version, SIDECAR_MINIMUM_VERSION_FLOOR) {
        return Err(format!(
            "Sidecar version {} is below build floor {}",
            artifact.sidecar_version, SIDECAR_MINIMUM_VERSION_FLOOR
        ));
    }

    let sidecar_bytes = std::fs::read(sidecar_path).map_err(|error| {
        format!(
            "Failed to read sidecar binary at {}: {error}",
            sidecar_path.display()
        )
    })?;
    let sidecar_sha = sha256_hex(&sidecar_bytes);
    if artifact.sha256.to_ascii_lowercase() != sidecar_sha {
        return Err(format!(
            "Sidecar checksum mismatch for {}",
            sidecar_path.display()
        ));
    }
    Ok(())
}

fn locate_sidecar_manifest_path(sidecar_path: &Path) -> Option<PathBuf> {
    let Some(sidecar_dir) = sidecar_path.parent() else {
        return None;
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    for ancestor in sidecar_dir.ancestors() {
        candidates.push(ancestor.join(SIDECAR_MANIFEST_FILE));
        candidates.push(ancestor.join("bin").join(SIDECAR_MANIFEST_FILE));
        candidates.push(
            ancestor
                .join("Resources")
                .join("bin")
                .join(SIDECAR_MANIFEST_FILE),
        );
        candidates.push(
            ancestor
                .join("..")
                .join("Resources")
                .join("bin")
                .join(SIDECAR_MANIFEST_FILE),
        );
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn detect_sidecar_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(override_path) = std::env::var(SIDECAR_OVERRIDE_ENV) {
        if sidecar_is_production_mode() {
            return Err(format!(
                "{SIDECAR_OVERRIDE_ENV} override is blocked in production mode"
            ));
        }
        let override_path = override_path.trim();
        if override_path.is_empty() {
            return Err(format!("{SIDECAR_OVERRIDE_ENV} override is empty"));
        }
        let path = PathBuf::from(override_path);
        if !path.exists() {
            return Err(format!(
                "{SIDECAR_OVERRIDE_ENV} override does not exist: {}",
                path.display()
            ));
        }
        if !is_executable_candidate(&path) {
            return Err(format!(
                "{SIDECAR_OVERRIDE_ENV} override is not executable: {}",
                path.display()
            ));
        }
        verify_sidecar_manifest_for_path(&path).map_err(|error| {
            format!(
                "{SIDECAR_OVERRIDE_ENV} override failed sidecar trust policy: {error}"
            )
        })?;
        return Ok(path);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    // Build target-suffixed binary name for non-Apple-Silicon hosts
    let target_suffix = format!("-{}", SIDECAR_TARGET_TRIPLE);

    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        // Unsuffixed candidates (for Apple Silicon where codex-app-server is the primary artifact)
        candidates.push(resource_dir.join("codex-app-server"));
        candidates.push(resource_dir.join("codex-app-server.exe"));
        candidates.push(resource_dir.join("bin/codex-app-server"));
        candidates.push(resource_dir.join("bin/codex-app-server.exe"));

        // Target-suffixed candidates (for x86_64 and Linux hosts)
        candidates.push(resource_dir.join(format!("codex-app-server{}", target_suffix)));
        candidates.push(resource_dir.join(format!("codex-app-server{}.exe", target_suffix)));
        candidates.push(resource_dir.join(format!("bin/codex-app-server{}", target_suffix)));
        candidates.push(resource_dir.join(format!("bin/codex-app-server{}.exe", target_suffix)));

        if let Some(app_dir) = resource_dir.parent() {
            candidates.push(app_dir.join("codex-app-server"));
            candidates.push(app_dir.join("codex-app-server.exe"));
            candidates.push(app_dir.join("MacOS").join("codex-app-server"));
            candidates.push(app_dir.join("MacOS").join("codex-app-server.exe"));

            // Target-suffixed for app_dir variants
            candidates.push(app_dir.join(format!("codex-app-server{}", target_suffix)));
            candidates.push(app_dir.join(format!("codex-app-server{}.exe", target_suffix)));
            candidates.push(
                app_dir
                    .join("MacOS")
                    .join(format!("codex-app-server{}", target_suffix)),
            );
            candidates.push(
                app_dir
                    .join("MacOS")
                    .join(format!("codex-app-server{}.exe", target_suffix)),
            );
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(bin_dir) = current_exe.parent() {
            candidates.push(bin_dir.join("codex-app-server"));
            candidates.push(bin_dir.join("codex-app-server.exe"));
            // Target-suffixed for current_exe variants
            candidates.push(bin_dir.join(format!("codex-app-server{}", target_suffix)));
            candidates.push(bin_dir.join(format!("codex-app-server{}.exe", target_suffix)));
        }
    }

    let mut trust_errors = Vec::new();

    for candidate in candidates.iter() {
        let candidate = candidate.clone();
        if !candidate.exists() || !is_executable_candidate(&candidate) {
            continue;
        }
        match verify_sidecar_manifest_for_path(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) => {
                trust_errors.push(format!("{} => {error}", candidate.display()));
            }
        }
    }

    if !trust_errors.is_empty() {
        return Err(format!(
            "Sidecar trust policy rejected all candidates: {}",
            trust_errors.join("; ")
        ));
    }

    let mut checked_candidates: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.display().to_string())
        .collect();
    if checked_candidates.is_empty() {
        checked_candidates.push("<none>".to_string());
    }
    Err(format!(
        "Codex App Server sidecar binary not found in known bundle/runtime locations: checked {}",
        checked_candidates.join(", ")
    ))
}

fn is_executable_candidate(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            return mode & 0o111 != 0;
        }
    }
    true
}

fn prune_restart_attempts(runtime: &mut CodexAppServerRuntime, now_epoch: i64) {
    while let Some(front) = runtime.restart_attempts.front() {
        if now_epoch - *front > RESTART_WINDOW_SECS {
            runtime.restart_attempts.pop_front();
        } else {
            break;
        }
    }
    runtime.status.restart_attempts_in_window = runtime.restart_attempts.len();
}

fn register_start_failure(runtime: &mut CodexAppServerRuntime, message: String, cause: &str) {
    let now_epoch = chrono::Utc::now().timestamp();
    runtime.restart_attempts.push_back(now_epoch);
    prune_restart_attempts(runtime, now_epoch);

    increment_labeled_counter(&mut runtime.restart_total, cause);

    runtime.status.stream_healthy = false;
    runtime.stream_session_state = StreamSessionState::Failed;
    if runtime.restart_attempts.len() >= RESTART_BUDGET {
        set_process_state(runtime, ProcessState::CrashLoop, Some(message));
    } else {
        set_process_state(runtime, ProcessState::Degraded, Some(message));
    }
}

fn remember_dedupe_source(runtime: &mut CodexAppServerRuntime, key: &str, source: &str) {
    runtime
        .dedupe_sources
        .insert(key.to_string(), source.to_string());

    if let Some(pos) = runtime
        .dedupe_key_order
        .iter()
        .position(|existing| existing == key)
    {
        runtime.dedupe_key_order.remove(pos);
    }
    runtime.dedupe_key_order.push_back(key.to_string());

    while runtime.dedupe_key_order.len() > MAX_DEDUPE_SOURCES {
        if let Some(evicted) = runtime.dedupe_key_order.pop_front() {
            runtime.dedupe_sources.remove(&evicted);
        }
    }
}

fn apply_account_updated(
    runtime: &mut CodexAppServerRuntime,
    auth_mode: &str,
    authenticated: bool,
) {
    let mode = normalize_auth_mode(auth_mode);
    if mode != "none" && !is_supported_auth_mode(&mode) {
        runtime.status.auth_mode = mode.clone();
        runtime.auth_state = AuthState::NeedsLogin;
        runtime.status.stream_healthy = false;
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error = Some(format!(
            "Unsupported auth mode for v1: {mode}. Expected one of {}",
            SUPPORTED_AUTH_MODES.join(", ")
        ));
        sync_status(runtime);
        return;
    }

    runtime.status.auth_mode = mode;
    if authenticated {
        runtime.auth_state = AuthState::Authenticated;
        runtime.status.last_error = None;
        if runtime.process_state == ProcessState::Running && runtime.status.stream_healthy {
            runtime.stream_session_state = StreamSessionState::Alive;
        }
    } else {
        runtime.auth_state = AuthState::NeedsLogin;
        runtime.status.stream_healthy = false;
        runtime.stream_session_state = StreamSessionState::Failed;
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error =
            Some("Authentication required via sidecar account update".to_string());
    }
    sync_status(runtime);
}

fn emit_mode_transition(
    app_handle: &AppHandle,
    runtime: &mut CodexAppServerRuntime,
    mode: &str,
    reason: &str,
) {
    let previous = runtime.last_mode.clone();
    if previous.as_deref() == Some(mode) {
        return;
    }

    let transition = CaptureModeTransition {
        at_iso: now_iso(),
        from_mode: previous,
        to_mode: mode.to_string(),
        reason: reason.to_string(),
    };

    runtime.last_mode = Some(mode.to_string());
    runtime.transitions.push_front(transition.clone());
    while runtime.transitions.len() > MAX_TRANSITIONS {
        runtime.transitions.pop_back();
    }

    let _ = app_handle.emit_to(
        MAIN_WINDOW_LABEL,
        "capture-reliability-transition",
        transition,
    );
}

fn handle_sidecar_exit(
    runtime: &mut CodexAppServerRuntime,
    exit_status: std::process::ExitStatus,
) -> bool {
    let was_crash_loop = runtime.process_state == ProcessState::CrashLoop;
    let cancelled = cancel_pending_rpcs(runtime, "rpc_cancelled_process_exit");
    runtime.sidecar = None;
    runtime.next_rpc_id = 1;
    runtime.status.stream_healthy = false;
    runtime.stream_session_state = StreamSessionState::Failed;
    register_start_failure(
        runtime,
        format!("Codex App Server sidecar exited: {exit_status}"),
        "process_exit",
    );

    let should_restart = !was_crash_loop
        && runtime.process_state != ProcessState::CrashLoop
        && !runtime.status.stream_kill_switch;

    if cancelled > 0 {
        runtime.status.last_error = Some(format!(
            "Codex App Server sidecar exited: {exit_status}; cancelled {cancelled} pending RPC(s)"
        ));
        sync_status(runtime);
    }

    should_restart
}

fn apply_restart_reentry_state(runtime: &mut CodexAppServerRuntime) {
    runtime.process_state = ProcessState::Running;
    runtime.stream_session_state = StreamSessionState::Expected;
    runtime.handshake_state = HandshakeState::NotStarted;
    runtime.auth_state = AuthState::NeedsLogin;
    runtime.status.stream_healthy = false;
    runtime.approval_waiters.clear();
    runtime.approval_window_id = generate_approval_window_id();
    runtime.thread_read_results.clear();
    runtime.thread_read_errors.clear();
    runtime.status.last_error = Some(format!(
        "Sidecar restarted; waiting for {METHOD_INITIALIZE}/{METHOD_INITIALIZED}/{METHOD_ACCOUNT_READ} re-entry sequence"
    ));
    sync_status(runtime);
}

fn remember_sidecar_stderr_line(runtime: &mut CodexAppServerRuntime, line: &str) {
    runtime.sidecar_stderr_ring.push_back(
        redact_sensitive_stderr_line(line)
            .trim()
            .chars()
            .take(1_024)
            .collect(),
    );
    while runtime.sidecar_stderr_ring.len() > MAX_SIDECAR_STDERR_LINES {
        runtime.sidecar_stderr_ring.pop_front();
        runtime.sidecar_stderr_dropped = runtime.sidecar_stderr_dropped.saturating_add(1);
    }
}

fn harden_sidecar_command(command: &mut Command, sidecar_path: &Path) {
    if let Some(parent) = sidecar_path.parent() {
        command.current_dir(parent);
    } else if let Ok(cwd) = std::env::current_dir() {
        command.current_dir(cwd);
    }

    for env_key in BLOCKED_SIDECAR_ENV_OVERRIDES {
        command.env_remove(env_key);
    }
}

fn spawn_sidecar_process(path: &Path) -> Result<SidecarProcess, String> {
    let mut command = Command::new(path);
    harden_sidecar_command(&mut command, path);
    command.arg("app-server");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Inject OTLP API key from keychain so the placeholder in config resolves.
    match secret_store::get_otlp_api_key() {
        Ok(Some(key)) => {
            command.env("NARRATIVE_OTEL_API_KEY", key);
        }
        Ok(None) => {
            // No key configured; Codex may fail OTLP auth if the placeholder is used.
            // This is non-fatal here to allow local-only usage.
        }
        Err(err) => {
            // Log but don't block startup; keychain errors shouldn't crash the sidecar.
            eprintln!("[codex_app_server] Failed to retrieve OTLP API key: {err}");
        }
    }

    let child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn Codex App Server sidecar: {e}"))?;

    let _ = child.id();
    Ok(SidecarProcess { child })
}

fn send_sidecar_message(
    runtime: &mut CodexAppServerRuntime,
    message: &serde_json::Value,
) -> Result<(), String> {
    let Some(sidecar) = runtime.sidecar.as_mut() else {
        return Err("Codex App Server sidecar is not running".to_string());
    };

    let Some(stdin) = sidecar.child.stdin.as_mut() else {
        return Err("Codex App Server stdin is unavailable".to_string());
    };

    let encoded = serde_json::to_vec(message)
        .map_err(|e| format!("failed to encode sidecar message: {e}"))?;
    stdin
        .write_all(&encoded)
        .map_err(|e| format!("failed to write sidecar message: {e}"))?;
    stdin
        .write_all(b"\n")
        .map_err(|e| format!("failed to write sidecar message delimiter: {e}"))?;
    stdin
        .flush()
        .map_err(|e| format!("failed to flush sidecar message: {e}"))?;

    Ok(())
}

fn is_valid_jsonrpc_id(value: &serde_json::Value) -> bool {
    matches!(
        value,
        serde_json::Value::String(_) | serde_json::Value::Number(_) | serde_json::Value::Null
    )
}

fn parse_rpc_response_id(value: &serde_json::Value) -> Option<String> {
    jsonrpc_id_to_string(value)
}

fn jsonrpc_id_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(raw) => Some(raw.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
        _ => None,
    }
}

fn parse_approval_request_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => jsonrpc_id_to_string(value),
    }
}

fn send_jsonrpc_result_response(
    runtime: &mut CodexAppServerRuntime,
    id: &serde_json::Value,
    result: serde_json::Value,
) -> Result<(), String> {
    let message = serde_json::json!({
        "id": id,
        "result": result
    });
    send_sidecar_message(runtime, &message)
}

fn send_jsonrpc_error_response(
    runtime: &mut CodexAppServerRuntime,
    id: &serde_json::Value,
    code: i64,
    message: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    });
    send_sidecar_message(runtime, &payload)
}

fn pending_rpc_kind_label(kind: PendingRpcKind) -> &'static str {
    match kind {
        PendingRpcKind::Initialize => METHOD_INITIALIZE,
        PendingRpcKind::AccountRead => METHOD_ACCOUNT_READ,
        PendingRpcKind::AccountLoginStart => METHOD_ACCOUNT_LOGIN_START,
        PendingRpcKind::AccountChatgptTokensRefresh => METHOD_ACCOUNT_CHATGPT_TOKENS_REFRESH,
        PendingRpcKind::ThreadRead => METHOD_THREAD_READ,
    }
}

fn next_rpc_id(runtime: &mut CodexAppServerRuntime) -> i64 {
    let id = runtime.next_rpc_id;
    runtime.next_rpc_id = runtime.next_rpc_id.saturating_add(1);
    id
}

fn cancel_pending_rpcs(runtime: &mut CodexAppServerRuntime, reason: &str) -> usize {
    if runtime.pending_rpcs.is_empty() {
        return 0;
    }
    let cancelled_requests: Vec<(String, PendingRpcKind)> = runtime
        .pending_rpcs
        .iter()
        .map(|(id, request)| (id.clone(), request.kind))
        .collect();
    let cancelled = cancelled_requests.len();
    for (id, kind) in cancelled_requests {
        if matches!(kind, PendingRpcKind::ThreadRead) {
            runtime
                .thread_read_errors
                .insert(id, "thread/read request cancelled".to_string());
        }
    }
    runtime.pending_rpcs.clear();
    increment_labeled_counter(&mut runtime.parse_error_total, reason);
    cancelled
}

/// Cancel a single pending RPC by request ID.
/// Returns true if the RPC was found and cancelled, false otherwise.
fn cancel_pending_rpc_by_id(
    runtime: &mut CodexAppServerRuntime,
    request_id: i64,
    reason: &str,
) -> bool {
    let id = request_id.to_string();
    if let Some(request) = runtime.pending_rpcs.remove(&id) {
        if matches!(request.kind, PendingRpcKind::ThreadRead) {
            runtime
                .thread_read_errors
                .insert(id, "thread/read request cancelled".to_string());
        }
        increment_labeled_counter(&mut runtime.parse_error_total, reason);
        true
    } else {
        false
    }
}

fn remember_approval_decision(
    runtime: &mut CodexAppServerRuntime,
    payload: &LiveSessionEventPayload,
    decision_token: Option<&str>,
    approval_window_id: Option<u64>,
) {
    let LiveSessionEventPayload::ApprovalResult { request_id, .. } = payload else {
        return;
    };

    runtime.approval_decisions.insert(
        request_id.clone(),
        ApprovalDecisionRecord {
            decision_token_hash: decision_token
                .map(hash_opaque_token)
                .unwrap_or_else(|| "none".to_string()),
            approval_window_id: approval_window_id.unwrap_or(runtime.approval_window_id),
        },
    );

    if let Some(existing_idx) = runtime
        .approval_decision_order
        .iter()
        .position(|existing| existing == request_id)
    {
        runtime.approval_decision_order.remove(existing_idx);
    }
    runtime
        .approval_decision_order
        .push_back(request_id.clone());
    while runtime.approval_decision_order.len() > MAX_APPROVAL_DECISIONS {
        if let Some(evicted) = runtime.approval_decision_order.pop_front() {
            runtime.approval_decisions.remove(&evicted);
        }
    }
}

fn bind_approval_request_token(payload: LiveSessionEventPayload) -> LiveSessionEventPayload {
    match payload {
        LiveSessionEventPayload::ApprovalRequest {
            request_id,
            thread_id,
            turn_id,
            command,
            options,
            timeout_ms,
            rpc_request_id,
            decision_token,
        } => LiveSessionEventPayload::ApprovalRequest {
            request_id,
            thread_id,
            turn_id,
            command,
            options,
            timeout_ms,
            rpc_request_id,
            decision_token: Some(decision_token.unwrap_or_else(generate_approval_decision_token)),
        },
        _ => payload,
    }
}

fn apply_pending_rpc_success(
    runtime: &mut CodexAppServerRuntime,
    id: &str,
    request: PendingRpcRequest,
    result: &serde_json::Value,
) {
    match request.kind {
        PendingRpcKind::Initialize => {
            runtime.handshake_state = HandshakeState::InitializeSent;
            runtime.status.last_error = None;
        }
        PendingRpcKind::AccountRead => {
            apply_account_read_result(runtime, result);
        }
        PendingRpcKind::AccountLoginStart => {
            if let Some(url) = result.get("authUrl").and_then(serde_json::Value::as_str) {
                match validate_and_redact_auth_url(url) {
                    Ok(redacted_url) => {
                        runtime.status.last_error =
                            Some(format!("Complete login in browser: {redacted_url}"));
                    }
                    Err(reason) => {
                        runtime.status.last_error =
                            Some(format!("Rejected sidecar auth URL: {reason}"));
                        if runtime.process_state != ProcessState::CrashLoop {
                            runtime.process_state = ProcessState::Degraded;
                        }
                    }
                }
            } else {
                runtime.status.last_error = None;
            }
        }
        PendingRpcKind::AccountChatgptTokensRefresh => {
            runtime.status.auth_mode = "chatgptAuthTokens".to_string();
            runtime.auth_state = AuthState::Authenticated;
            runtime.status.last_error = None;
        }
        PendingRpcKind::ThreadRead => {
            runtime.thread_read_errors.remove(id);
            runtime
                .thread_read_results
                .insert(id.to_string(), result.clone());
            runtime.status.last_error = None;
        }
    }
}

fn apply_pending_rpc_error(
    runtime: &mut CodexAppServerRuntime,
    id: &str,
    request: PendingRpcRequest,
    error: &serde_json::Value,
) {
    let method = pending_rpc_kind_label(request.kind);
    runtime.status.last_error = Some(format!(
        "Sidecar RPC error ({id}, {method}): {}",
        format_redacted_json(error)
    ));
    if runtime.process_state != ProcessState::CrashLoop {
        runtime.process_state = ProcessState::Degraded;
    }
    if matches!(request.kind, PendingRpcKind::Initialize) {
        runtime.handshake_state = HandshakeState::NotStarted;
    }
    if matches!(
        request.kind,
        PendingRpcKind::AccountRead
            | PendingRpcKind::AccountLoginStart
            | PendingRpcKind::AccountChatgptTokensRefresh
    ) {
        runtime.auth_state = AuthState::NeedsLogin;
        runtime.status.stream_healthy = false;
        runtime.stream_session_state = StreamSessionState::Failed;
    }
    if matches!(request.kind, PendingRpcKind::ThreadRead) {
        runtime.thread_read_results.remove(id);
        runtime.thread_read_errors.insert(
            id.to_string(),
            format!(
                "thread/read request failed: {}",
                format_redacted_json(error)
            ),
        );
    }
}

fn send_sidecar_request(
    runtime: &mut CodexAppServerRuntime,
    method: &str,
    params: serde_json::Value,
    kind: PendingRpcKind,
) -> Result<i64, String> {
    if runtime.pending_rpcs.len() >= MAX_PENDING_RPC_REQUESTS {
        increment_labeled_counter(&mut runtime.parse_error_total, "rpc_overload");
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        let message =
            format!("Too many pending sidecar RPC requests (limit: {MAX_PENDING_RPC_REQUESTS})");
        runtime.status.last_error = Some(message.clone());
        sync_status(runtime);
        return Err(message);
    }
    let id = next_rpc_id(runtime);
    let message = serde_json::json!({
        "method": method,
        "id": id,
        "params": params,
    });
    send_sidecar_message(runtime, &message)?;
    let request_id = id.to_string();
    runtime.pending_rpcs.insert(
        request_id,
        PendingRpcRequest {
            kind,
            sent_at_epoch_ms: now_epoch_ms(),
            timeout_ms: RPC_REQUEST_TIMEOUT_MS,
        },
    );
    Ok(id)
}

fn expire_pending_rpcs(runtime: &mut CodexAppServerRuntime, now_ms: i64) -> usize {
    let mut expired: Vec<(String, PendingRpcKind)> = Vec::new();
    for (id, request) in &runtime.pending_rpcs {
        let expires_at = request
            .sent_at_epoch_ms
            .saturating_add(request.timeout_ms as i64);
        if now_ms >= expires_at {
            expired.push((id.clone(), request.kind));
        }
    }

    for (id, _) in &expired {
        let _ = runtime.pending_rpcs.remove(id);
    }

    for (id, kind) in &expired {
        if matches!(kind, PendingRpcKind::ThreadRead) {
            runtime.thread_read_results.remove(id);
            runtime
                .thread_read_errors
                .insert(id.clone(), "thread/read request timed out".to_string());
        }
    }

    if !expired.is_empty() {
        increment_labeled_counter(&mut runtime.parse_error_total, "rpc_timeout");
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error = Some(format!(
            "Timed out waiting for {} sidecar request(s): {}",
            expired.len(),
            expired
                .iter()
                .map(|(id, kind)| format!("{id}:{}", pending_rpc_kind_label(*kind)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if expired
            .iter()
            .any(|(_, kind)| matches!(kind, PendingRpcKind::Initialize))
        {
            runtime.handshake_state = HandshakeState::NotStarted;
        }
        if expired.iter().any(|(_, kind)| {
            matches!(
                kind,
                PendingRpcKind::AccountRead
                    | PendingRpcKind::AccountLoginStart
                    | PendingRpcKind::AccountChatgptTokensRefresh
            )
        }) {
            runtime.auth_state = AuthState::NeedsLogin;
            runtime.status.stream_healthy = false;
            runtime.stream_session_state = StreamSessionState::Failed;
        }
        sync_status(runtime);
    }

    expired.len()
}

fn has_pending_initialize_request(runtime: &CodexAppServerRuntime) -> bool {
    runtime
        .pending_rpcs
        .values()
        .any(|request| matches!(request.kind, PendingRpcKind::Initialize))
}

fn wait_for_initialize_ack(
    state: &Arc<Mutex<CodexAppServerRuntime>>,
    timeout: Duration,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        {
            let runtime = state.lock().map_err(|e| e.to_string())?;
            if runtime.handshake_state == HandshakeState::InitializeSent {
                return Ok(());
            }
            if runtime.handshake_state == HandshakeState::Initialized {
                return Err("Already initialized".to_string());
            }
            if runtime.process_state == ProcessState::Inactive
                || runtime.process_state == ProcessState::CrashLoop
            {
                return Err("Not initialized: app server is not running".to_string());
            }
            if !has_pending_initialize_request(&runtime)
                && runtime.handshake_state != HandshakeState::InitializeSent
            {
                return Err("Not initialized: initialize response not received".to_string());
            }
        }

        if started.elapsed() >= timeout {
            return Err("Not initialized: timed out waiting for initialize response".to_string());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_thread_read_response(
    state: &Arc<Mutex<CodexAppServerRuntime>>,
    request_id: i64,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let request_id = request_id.to_string();
    let started = Instant::now();
    loop {
        {
            let mut runtime = state.lock().map_err(|e| e.to_string())?;
            if let Some(result) = runtime.thread_read_results.remove(&request_id) {
                return Ok(result);
            }
            if let Some(error) = runtime.thread_read_errors.remove(&request_id) {
                return Err(error);
            }
            if runtime.process_state == ProcessState::Inactive
                || runtime.process_state == ProcessState::CrashLoop
            {
                return Err("thread/read aborted: app server is not running".to_string());
            }
            if !runtime.pending_rpcs.contains_key(&request_id) {
                return Err(format!(
                    "thread/read response missing for request id {request_id}"
                ));
            }
        }

        if started.elapsed() >= timeout {
            return Err(format!(
                "thread/read timed out waiting for sidecar response ({request_id})"
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn build_sidecar_live_delta(method: &str, message: &serde_json::Value) -> LiveSessionEventPayload {
    let params = message
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let thread_id = params
        .get("threadId")
        .or_else(|| params.get("thread_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-thread")
        .to_string();
    let turn_id = params
        .get("turnId")
        .or_else(|| params.get("turn_id"))
        .or_else(|| params.get("turn").and_then(|turn| turn.get("id")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-turn")
        .to_string();
    let item_id = params
        .get("itemId")
        .or_else(|| params.get("item_id"))
        .or_else(|| params.get("item").and_then(|item| item.get("id")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-item")
        .to_string();

    LiveSessionEventPayload::SessionDelta {
        thread_id,
        turn_id,
        item_id,
        event_type: method.to_string(),
        source: "app_server_stream".to_string(),
        sequence_id: 0,
        received_at_iso: now_iso(),
        payload: params,
    }
}

fn build_sidecar_approval_request(
    method: &str,
    message: &serde_json::Value,
    rpc_request_id: Option<serde_json::Value>,
) -> LiveSessionEventPayload {
    let params = message
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let request_id = params
        .get("requestId")
        .or_else(|| params.get("request_id"))
        .and_then(parse_approval_request_id)
        .or_else(|| rpc_request_id.as_ref().and_then(parse_approval_request_id))
        .unwrap_or_else(|| format!("approval_{}", now_epoch_ms()));
    let thread_id = params
        .get("threadId")
        .or_else(|| params.get("thread_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-thread")
        .to_string();
    let turn_id = params
        .get("turnId")
        .or_else(|| params.get("turn_id"))
        .or_else(|| params.get("turn").and_then(|turn| turn.get("id")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-turn")
        .to_string();
    let command = params
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(method)
        .to_string();
    let options = params
        .get("options")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .filter(|entries| !entries.is_empty())
        .unwrap_or_else(|| vec!["approve".to_string(), "deny".to_string()]);
    let timeout_ms = params
        .get("timeoutMs")
        .or_else(|| params.get("timeout_ms"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(30_000);

    LiveSessionEventPayload::ApprovalRequest {
        request_id,
        thread_id,
        turn_id,
        command,
        options,
        timeout_ms,
        rpc_request_id,
        decision_token: None,
    }
}

fn is_allowed_sidecar_notification_method(method: &str) -> bool {
    ALLOWED_SIDECAR_NOTIFICATION_METHODS.contains(&method)
}

fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        serde_json::Value::Object(map) => 1 + map.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn reject_unknown_fields(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_fields: &[&str],
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed_fields.iter().any(|allowed| allowed == key) {
            return Err(format!("Unsupported field in sidecar payload: {key}"));
        }
    }
    Ok(())
}

fn require_any_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    candidates: &[&str],
    label: &str,
) -> Result<(), String> {
    let has_value = candidates.iter().any(|field| {
        object
            .get(*field)
            .and_then(serde_json::Value::as_str)
            .is_some()
    });
    if has_value {
        Ok(())
    } else {
        Err(format!("Missing required sidecar field: {label}"))
    }
}

fn validate_sidecar_notification_payload(
    method: &str,
    message: &serde_json::Value,
) -> Result<(), String> {
    if json_depth(message) > MAX_SIDECAR_JSON_DEPTH {
        return Err(format!(
            "Sidecar payload exceeded max depth ({MAX_SIDECAR_JSON_DEPTH})"
        ));
    }

    let params = message
        .get("params")
        .ok_or_else(|| "Missing params for sidecar notification".to_string())?;
    let params_object = params
        .as_object()
        .ok_or_else(|| "Sidecar notification params must be an object".to_string())?;

    match method {
        "account/updated" => {
            reject_unknown_fields(
                params_object,
                &["authMode", "auth_mode", "authenticated", "account"],
            )?;
            if let Some(mode) = params_object
                .get("authMode")
                .or_else(|| params_object.get("auth_mode"))
            {
                if !mode.is_string() && !mode.is_null() {
                    return Err("account/updated.authMode must be string or null".to_string());
                }
            }
        }
        "account/login/completed" => {
            reject_unknown_fields(params_object, &["success", "error", "loginId"])?;
            if params_object
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .is_none()
            {
                return Err("account/login/completed.success must be boolean".to_string());
            }
            if let Some(login_id) = params_object.get("loginId") {
                if !login_id.is_string() {
                    return Err("account/login/completed.loginId must be string".to_string());
                }
            }
            if let Some(error) = params_object.get("error") {
                if !error.is_string() && !error.is_null() {
                    return Err("account/login/completed.error must be string or null".to_string());
                }
            }
        }
        "turn/started" | "turn/completed" => {
            require_any_string_field(params_object, &["threadId", "thread_id"], "threadId")?;
            require_any_string_field(params_object, &["turnId", "turn_id"], "turnId")?;
        }
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            let has_request_id = params_object
                .get("requestId")
                .or_else(|| params_object.get("request_id"))
                .and_then(parse_approval_request_id)
                .is_some()
                || message
                    .get("id")
                    .and_then(parse_approval_request_id)
                    .is_some();
            if !has_request_id {
                return Err(
                    "approval request must include requestId/request_id or envelope id".to_string(),
                );
            }
            require_any_string_field(params_object, &["threadId", "thread_id"], "threadId")?;
            require_any_string_field(params_object, &["turnId", "turn_id"], "turnId")?;
            require_any_string_field(params_object, &["command"], "command")?;
            if let Some(options) = params_object.get("options") {
                let Some(values) = options.as_array() else {
                    return Err("approval request options must be an array".to_string());
                };
                if !values.iter().all(|value| value.is_string()) {
                    return Err(
                        "approval request options must contain only string values".to_string()
                    );
                }
            }
            if let Some(timeout) = params_object.get("timeoutMs") {
                if timeout.as_u64().is_none() {
                    return Err("approval request timeoutMs must be u64".to_string());
                }
            }
        }
        "item/started" | "item/completed" | "item/agentMessage/delta" => {
            require_any_string_field(params_object, &["threadId", "thread_id"], "threadId")?;
            require_any_string_field(params_object, &["turnId", "turn_id"], "turnId")?;
            require_any_string_field(params_object, &["itemId", "item_id"], "itemId")?;
        }
        _ => {}
    }

    Ok(())
}

fn validate_sidecar_rpc_result(
    kind: PendingRpcKind,
    result: &serde_json::Value,
) -> Result<(), String> {
    if json_depth(result) > MAX_SIDECAR_JSON_DEPTH {
        return Err(format!(
            "Sidecar RPC result exceeded max depth ({MAX_SIDECAR_JSON_DEPTH})"
        ));
    }

    match kind {
        PendingRpcKind::Initialize => {
            if !result.is_object() {
                return Err("initialize result must be an object".to_string());
            }
        }
        PendingRpcKind::AccountRead => {
            if !result.is_object() {
                return Err("account/read result must be an object".to_string());
            }
        }
        PendingRpcKind::AccountLoginStart => {
            let object = result
                .as_object()
                .ok_or_else(|| "account/login/start result must be an object".to_string())?;
            if let Some(auth_url) = object.get("authUrl") {
                let Some(auth_url) = auth_url.as_str() else {
                    return Err("account/login/start.authUrl must be a string".to_string());
                };
                validate_and_redact_auth_url(auth_url)
                    .map_err(|reason| format!("account/login/start.authUrl invalid: {reason}"))?;
            }
        }
        PendingRpcKind::AccountChatgptTokensRefresh => {
            if !result.is_object() {
                return Err(
                    "account/chatgptAuthTokens/refresh result must be an object".to_string()
                );
            }
        }
        PendingRpcKind::ThreadRead => {
            if !result.is_object() {
                return Err("thread/read result must be an object".to_string());
            }
        }
    }

    Ok(())
}

fn apply_account_notification(runtime: &mut CodexAppServerRuntime, params: &serde_json::Value) {
    let auth_mode = params
        .get("authMode")
        .or_else(|| params.get("auth_mode"))
        .and_then(serde_json::Value::as_str);

    match auth_mode {
        Some(mode) => apply_account_updated(runtime, mode, true),
        None => {
            runtime.auth_state = AuthState::NeedsLogin;
            runtime.status.auth_mode = "chatgpt".to_string();
            runtime.status.stream_healthy = false;
            runtime.stream_session_state = StreamSessionState::Failed;
            if runtime.process_state != ProcessState::CrashLoop {
                runtime.process_state = ProcessState::Degraded;
            }
            runtime.status.last_error =
                Some("Authentication required via sidecar account update".to_string());
            sync_status(runtime);
        }
    }
}

fn apply_account_read_result(runtime: &mut CodexAppServerRuntime, result: &serde_json::Value) {
    let account = result.get("account");
    match account {
        Some(serde_json::Value::Object(obj)) => {
            let auth_mode = obj
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("chatgpt");
            apply_account_updated(runtime, auth_mode, true);
        }
        _ => {
            runtime.auth_state = AuthState::NeedsLogin;
            runtime.status.auth_mode = "chatgpt".to_string();
            runtime.status.stream_healthy = false;
            runtime.stream_session_state = StreamSessionState::Failed;
            if runtime.process_state != ProcessState::CrashLoop {
                runtime.process_state = ProcessState::Degraded;
            }
            runtime.status.last_error =
                Some("No authenticated Codex account available".to_string());
            sync_status(runtime);
        }
    }
}

fn process_sidecar_message(
    app_handle: &AppHandle,
    state: &Arc<Mutex<CodexAppServerRuntime>>,
    message: serde_json::Value,
) {
    let method = message.get("method").and_then(serde_json::Value::as_str);
    let id = message.get("id").cloned();
    let has_result = message.get("result").is_some();
    let has_error = message.get("error").is_some();

    if method.is_some() && (has_result || has_error) {
        let mut runtime = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error = Some("Rejected mixed sidecar envelope".to_string());
        let parser_error = LiveSessionEventPayload::ParserValidationError {
            kind: "protocol_violation".to_string(),
            raw_preview: "mixed-envelope".to_string(),
            reason: "Envelope contained method and response fields".to_string(),
            occurred_at_iso: now_iso(),
        };
        handle_live_event_internal(&mut runtime, &parser_error);
        emit_live_session_event(app_handle, &parser_error);
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    }

    if let (Some(method), Some(request_id)) = (method, id.as_ref()) {
        let mut runtime = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if !is_valid_jsonrpc_id(request_id) {
            if runtime.process_state != ProcessState::CrashLoop {
                runtime.process_state = ProcessState::Degraded;
            }
            runtime.status.last_error = Some(format!(
                "Rejected sidecar request with invalid id for method: {method}"
            ));
            let parser_error = LiveSessionEventPayload::ParserValidationError {
                kind: "protocol_violation".to_string(),
                raw_preview: method.to_string(),
                reason: "Server request id must be string, number, or null".to_string(),
                occurred_at_iso: now_iso(),
            };
            handle_live_event_internal(&mut runtime, &parser_error);
            emit_live_session_event(app_handle, &parser_error);
            sync_status(&mut runtime);
            emit_status(app_handle, &runtime.status);
            return;
        }

        match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                if runtime.approval_waiters.len() >= MAX_PENDING_APPROVAL_WAITERS {
                    for timed_out in expire_pending_approvals(&mut runtime, now_epoch_ms()) {
                        emit_live_session_event(app_handle, &timed_out);
                    }
                    if runtime.approval_waiters.len() < MAX_PENDING_APPROVAL_WAITERS {
                        sync_status(&mut runtime);
                        emit_status(app_handle, &runtime.status);
                    }
                }

                if runtime.approval_waiters.len() >= MAX_PENDING_APPROVAL_WAITERS {
                    increment_labeled_counter(&mut runtime.parse_error_total, "approval_overload");
                    if runtime.process_state != ProcessState::CrashLoop {
                        runtime.process_state = ProcessState::Degraded;
                    }
                    runtime.status.last_error = Some(format!(
                        "Rejected approval request ({method}) because approval queue is full (limit: {MAX_PENDING_APPROVAL_WAITERS})"
                    ));
                    let _ = send_jsonrpc_error_response(
                        &mut runtime,
                        request_id,
                        JSONRPC_OVERLOADED,
                        "approval queue overloaded; retry later",
                    );
                    sync_status(&mut runtime);
                    emit_status(app_handle, &runtime.status);
                    return;
                }

                if let Err(reason) = validate_sidecar_notification_payload(method, &message) {
                    if runtime.process_state != ProcessState::CrashLoop {
                        runtime.process_state = ProcessState::Degraded;
                    }
                    runtime.status.last_error = Some(format!(
                        "Rejected sidecar request ({method}) due to schema validation error: {reason}"
                    ));
                    let parser_error = LiveSessionEventPayload::ParserValidationError {
                        kind: "schema_mismatch".to_string(),
                        raw_preview: method.to_string(),
                        reason: reason.clone(),
                        occurred_at_iso: now_iso(),
                    };
                    handle_live_event_internal(&mut runtime, &parser_error);
                    emit_live_session_event(app_handle, &parser_error);
                    let _ = send_jsonrpc_error_response(
                        &mut runtime,
                        request_id,
                        JSONRPC_INVALID_PARAMS,
                        "invalid request params",
                    );
                    sync_status(&mut runtime);
                    emit_status(app_handle, &runtime.status);
                    return;
                }

                let event = bind_approval_request_token(build_sidecar_approval_request(
                    method,
                    &message,
                    Some(request_id.clone()),
                ));
                let _ = handle_live_event_internal(&mut runtime, &event);
                emit_live_session_event(app_handle, &event);
            }
            _ => {
                increment_labeled_counter(&mut runtime.parse_error_total, "unknown_server_request");
                let _ = send_jsonrpc_error_response(
                    &mut runtime,
                    request_id,
                    JSONRPC_METHOD_NOT_FOUND,
                    &format!("Unsupported sidecar request method: {method}"),
                );
            }
        }

        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    }

    if let Some(method) = method {
        let mut runtime = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if let Err(reason) = validate_sidecar_notification_payload(method, &message) {
            if runtime.process_state != ProcessState::CrashLoop {
                runtime.process_state = ProcessState::Degraded;
            }
            runtime.status.last_error = Some(format!(
                "Rejected sidecar notification ({method}) due to schema validation error: {reason}"
            ));
            let parser_error = LiveSessionEventPayload::ParserValidationError {
                kind: "schema_mismatch".to_string(),
                raw_preview: method.to_string(),
                reason,
                occurred_at_iso: now_iso(),
            };
            handle_live_event_internal(&mut runtime, &parser_error);
            emit_live_session_event(app_handle, &parser_error);
            sync_status(&mut runtime);
            emit_status(app_handle, &runtime.status);
            return;
        }

        match method {
            "account/updated" => {
                let params = message
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                apply_account_notification(&mut runtime, &params);
            }
            "account/login/completed" => {
                let params = message
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let success = params
                    .get("success")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if success {
                    runtime.auth_state = AuthState::Authenticating;
                    runtime.status.last_error = None;
                } else {
                    runtime.auth_state = AuthState::NeedsLogin;
                    runtime.status.stream_healthy = false;
                    runtime.stream_session_state = StreamSessionState::Failed;
                    if runtime.process_state != ProcessState::CrashLoop {
                        runtime.process_state = ProcessState::Degraded;
                    }
                    runtime.status.last_error = Some(
                        params
                            .get("error")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("Authentication cancelled or failed")
                            .to_string(),
                    );
                }
                sync_status(&mut runtime);
            }
            "item/started"
            | "item/completed"
            | "item/agentMessage/delta"
            | "turn/started"
            | "turn/completed" => {
                let event = build_sidecar_live_delta(method, &message);
                let _ = handle_live_event_internal(&mut runtime, &event);
                emit_live_session_event(app_handle, &event);
            }
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                let event = bind_approval_request_token(build_sidecar_approval_request(
                    method, &message, None,
                ));
                let _ = handle_live_event_internal(&mut runtime, &event);
                emit_live_session_event(app_handle, &event);
            }
            _ => {
                if !is_allowed_sidecar_notification_method(method) {
                    if runtime.process_state != ProcessState::CrashLoop {
                        runtime.process_state = ProcessState::Degraded;
                    }
                    runtime.status.last_error =
                        Some(format!("Unsupported sidecar notification method: {method}"));
                    let parser_error = LiveSessionEventPayload::ParserValidationError {
                        kind: "protocol_violation".to_string(),
                        raw_preview: method.to_string(),
                        reason: format!("Rejected unknown sidecar notification method: {method}"),
                        occurred_at_iso: now_iso(),
                    };
                    handle_live_event_internal(&mut runtime, &parser_error);
                    emit_live_session_event(app_handle, &parser_error);
                    sync_status(&mut runtime);
                }
            }
        }

        emit_status(app_handle, &runtime.status);
        return;
    }

    if id.is_none() || (!has_result && !has_error) {
        let mut runtime = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error = Some("Rejected malformed sidecar envelope".to_string());
        let parser_error = LiveSessionEventPayload::ParserValidationError {
            kind: "protocol_violation".to_string(),
            raw_preview: "invalid-envelope".to_string(),
            reason: "Envelope did not match request/notification/response shapes".to_string(),
            occurred_at_iso: now_iso(),
        };
        handle_live_event_internal(&mut runtime, &parser_error);
        emit_live_session_event(app_handle, &parser_error);
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    }

    let Some(id) = id.as_ref().and_then(parse_rpc_response_id) else {
        let mut runtime = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        increment_labeled_counter(&mut runtime.parse_error_total, "rpc_invalid_id");
        runtime.status.last_error =
            Some("Ignored sidecar response with unsupported id type".to_string());
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    };

    let mut runtime = match state.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let Some(request) = runtime.pending_rpcs.remove(&id) else {
        increment_labeled_counter(&mut runtime.parse_error_total, "rpc_unknown_id");
        runtime.status.last_error = Some(format!(
            "Ignored sidecar response for unknown request id: {id}"
        ));
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    };

    if let Some(err) = message.get("error") {
        apply_pending_rpc_error(&mut runtime, &id, request, err);
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    }

    let result = message
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    if let Err(reason) = validate_sidecar_rpc_result(request.kind, &result) {
        if runtime.process_state != ProcessState::CrashLoop {
            runtime.process_state = ProcessState::Degraded;
        }
        runtime.status.last_error = Some(format!(
            "Rejected sidecar RPC response ({id}) due to schema validation error: {reason}"
        ));
        let parser_error = LiveSessionEventPayload::ParserValidationError {
            kind: "schema_mismatch".to_string(),
            raw_preview: format!("rpc:{id}"),
            reason,
            occurred_at_iso: now_iso(),
        };
        handle_live_event_internal(&mut runtime, &parser_error);
        emit_live_session_event(app_handle, &parser_error);
        sync_status(&mut runtime);
        emit_status(app_handle, &runtime.status);
        return;
    }

    apply_pending_rpc_success(&mut runtime, &id, request, &result);
    sync_status(&mut runtime);
    emit_status(app_handle, &runtime.status);
}

fn validate_sidecar_jsonl_frame(trimmed: &str) -> Result<(), LiveSessionEventPayload> {
    if trimmed.len() <= MAX_SIDECAR_JSONL_BYTES {
        return Ok(());
    }

    Err(LiveSessionEventPayload::ParserValidationError {
        kind: "protocol_violation".to_string(),
        raw_preview: redact_sensitive_stderr_line(trimmed)
            .chars()
            .take(256)
            .collect(),
        reason: format!(
            "Sidecar frame exceeded max size ({} bytes > {} bytes)",
            trimmed.len(),
            MAX_SIDECAR_JSONL_BYTES
        ),
        occurred_at_iso: now_iso(),
    })
}

fn apply_sidecar_stdout_read_error(
    runtime: &mut CodexAppServerRuntime,
    error_message: &str,
) -> LiveSessionEventPayload {
    let event = LiveSessionEventPayload::ParserValidationError {
        kind: "protocol_violation".to_string(),
        raw_preview: "".to_string(),
        reason: format!("Failed to read sidecar stdout line: {error_message}"),
        occurred_at_iso: now_iso(),
    };
    handle_live_event_internal(runtime, &event);
    if runtime.process_state != ProcessState::CrashLoop {
        runtime.process_state = ProcessState::Degraded;
    }
    runtime.status.last_error = Some("Sidecar stdout stream produced invalid UTF-8".to_string());
    sync_status(runtime);
    event
}

fn spawn_sidecar_stdout_reader(
    app_handle: AppHandle,
    state: Arc<Mutex<CodexAppServerRuntime>>,
    cancel: Arc<AtomicBool>,
    stdout: ChildStdout,
) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let line = match line {
                Ok(line) => line,
                Err(err) => {
                    let mut runtime = match state.lock() {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    let event = apply_sidecar_stdout_read_error(&mut runtime, &err.to_string());
                    emit_live_session_event(&app_handle, &event);
                    emit_status(&app_handle, &runtime.status);
                    continue;
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Err(event) = validate_sidecar_jsonl_frame(trimmed) {
                let mut runtime = match state.lock() {
                    Ok(guard) => guard,
                    Err(_) => break,
                };
                handle_live_event_internal(&mut runtime, &event);
                if runtime.process_state != ProcessState::CrashLoop {
                    runtime.process_state = ProcessState::Degraded;
                }
                runtime.status.last_error = Some(
                    "Sidecar frame rejected: payload exceeds maximum allowed size".to_string(),
                );
                sync_status(&mut runtime);
                emit_live_session_event(&app_handle, &event);
                emit_status(&app_handle, &runtime.status);
                continue;
            }
            let parsed = match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(value) => value,
                Err(err) => {
                    let mut runtime = match state.lock() {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    let event = LiveSessionEventPayload::ParserValidationError {
                        kind: "protocol_violation".to_string(),
                        raw_preview: redact_sensitive_stderr_line(trimmed)
                            .chars()
                            .take(256)
                            .collect(),
                        reason: format!("Failed to parse sidecar JSON: {err}"),
                        occurred_at_iso: now_iso(),
                    };
                    handle_live_event_internal(&mut runtime, &event);
                    emit_live_session_event(&app_handle, &event);
                    emit_status(&app_handle, &runtime.status);
                    continue;
                }
            };

            process_sidecar_message(&app_handle, &state, parsed);
        }
    });
}

fn spawn_sidecar_stderr_reader(
    state: Arc<Mutex<CodexAppServerRuntime>>,
    cancel: Arc<AtomicBool>,
    stderr: ChildStderr,
) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }

            let mut runtime = match state.lock() {
                Ok(guard) => guard,
                Err(_) => break,
            };
            remember_sidecar_stderr_line(&mut runtime, &line);
        }
    });
}

fn spawn_monitor_thread(
    app_handle: AppHandle,
    state: Arc<Mutex<CodexAppServerRuntime>>,
    cancel: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !cancel.load(Ordering::SeqCst) {
            let mut should_restart = false;
            let mut restart_path: Option<PathBuf> = None;

            {
                let mut runtime = match state.lock() {
                    Ok(guard) => guard,
                    Err(_) => break,
                };

                if runtime.sidecar.is_none() {
                    break;
                }

                let timed_out_rpcs = expire_pending_rpcs(&mut runtime, now_epoch_ms());
                if timed_out_rpcs > 0 {
                    emit_status(&app_handle, &runtime.status);
                }

                let child_status = runtime
                    .sidecar
                    .as_mut()
                    .and_then(|sidecar| sidecar.child.try_wait().ok().flatten());

                if let Some(exit_status) = child_status {
                    should_restart = handle_sidecar_exit(&mut runtime, exit_status);
                    if should_restart {
                        restart_path = runtime.sidecar_path.clone();
                    }

                    emit_status(&app_handle, &runtime.status);
                }
            }

            if should_restart {
                thread::sleep(Duration::from_millis(RESTART_COOLDOWN_MS));
                if cancel.load(Ordering::SeqCst) {
                    break;
                }

                if let Some(path) = restart_path {
                    if let Err(error) = verify_sidecar_manifest_for_path(&path) {
                        let mut runtime = match state.lock() {
                            Ok(guard) => guard,
                            Err(_) => break,
                        };
                        register_start_failure(&mut runtime, error, "restart_path_untrusted");
                        emit_status(&app_handle, &runtime.status);
                        continue;
                    }
                    match spawn_sidecar_process(&path) {
                        Ok(mut sidecar) => {
                            let Some(stdout) = sidecar.child.stdout.take() else {
                                let mut runtime = match state.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => break,
                                };
                                register_start_failure(
                                    &mut runtime,
                                    "Failed to capture Codex App Server stdout".to_string(),
                                    "stdout_missing",
                                );
                                emit_status(&app_handle, &runtime.status);
                                continue;
                            };
                            let Some(stderr) = sidecar.child.stderr.take() else {
                                let mut runtime = match state.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => break,
                                };
                                register_start_failure(
                                    &mut runtime,
                                    "Failed to capture Codex App Server stderr".to_string(),
                                    "stderr_missing",
                                );
                                emit_status(&app_handle, &runtime.status);
                                continue;
                            };
                            let mut runtime = match state.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            runtime.sidecar = Some(sidecar);
                            apply_restart_reentry_state(&mut runtime);
                            emit_status(&app_handle, &runtime.status);
                            spawn_sidecar_stdout_reader(
                                app_handle.clone(),
                                state.clone(),
                                runtime.monitor_cancel.clone(),
                                stdout,
                            );
                            spawn_sidecar_stderr_reader(
                                state.clone(),
                                runtime.monitor_cancel.clone(),
                                stderr,
                            );
                        }
                        Err(err) => {
                            let mut runtime = match state.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            register_start_failure(&mut runtime, err, "restart_spawn_failed");
                            emit_status(&app_handle, &runtime.status);
                        }
                    }
                }
            }

            thread::sleep(Duration::from_millis(MONITOR_POLL_INTERVAL_MS));
        }
    })
}

fn with_db_pool(app_handle: &AppHandle) -> Option<Arc<SqlitePool>> {
    app_handle
        .try_state::<crate::DbState>()
        .map(|state| state.0.clone())
}

fn thread_read_effect_id(request_id: i64) -> String {
    format!("rpc:thread-read:{request_id}")
}

fn checkpoint_retry_reason_for_thread_read_error(error: &str) -> &'static str {
    let normalized = error.to_lowercase();
    if normalized.contains("timed out") {
        TRUST_PAUSE_REASON_SNAPSHOT_TIMEOUT
    } else if normalized.contains("cancelled") || normalized.contains("context mismatch") {
        TRUST_PAUSE_REASON_THREAD_MISMATCH
    } else {
        TRUST_PAUSE_REASON_HYDRATE_FAILED
    }
}

fn load_recovery_checkpoint_blocking(
    app_handle: &AppHandle,
    thread_id: &str,
) -> Result<Option<RecoveryCheckpoint>, String> {
    let Some(pool) = with_db_pool(app_handle) else {
        return Ok(None);
    };

    tauri::async_runtime::block_on(load_recovery_checkpoint(&pool, thread_id))
}

fn upsert_recovery_checkpoint_blocking(
    app_handle: &AppHandle,
    checkpoint: &RecoveryCheckpoint,
) -> Result<(), String> {
    let Some(pool) = with_db_pool(app_handle) else {
        return Ok(());
    };

    tauri::async_runtime::block_on(upsert_recovery_checkpoint(&pool, checkpoint))
}

fn prepare_thread_snapshot_checkpoint_blocking(
    app_handle: &AppHandle,
    thread_id: &str,
) -> Result<RecoveryCheckpoint, String> {
    let Some(pool) = with_db_pool(app_handle) else {
        // Fallback to non-persistent checkpoint if DB unavailable
        let checkpoint_written_at_iso = now_iso();
        return Ok(new_recovery_checkpoint(thread_id, &checkpoint_written_at_iso));
    };

    let checkpoint_written_at_iso = now_iso();
    tauri::async_runtime::block_on(
        crate::recovery_checkpoint::prepare_recovery_checkpoint_atomic(
            &pool,
            thread_id,
            &checkpoint_written_at_iso,
        ),
    )
}

fn persist_inflight_thread_snapshot_checkpoint_blocking(
    app_handle: &AppHandle,
    thread_id: &str,
    request_id: i64,
) -> Result<RecoveryCheckpoint, String> {
    let existing = load_recovery_checkpoint_blocking(app_handle, thread_id)?
        .unwrap_or_else(|| new_recovery_checkpoint(thread_id, &now_iso()));
    let checkpoint = RecoveryCheckpoint {
        thread_id: Some(thread_id.to_string()),
        inflight_effect_ids: vec![thread_read_effect_id(request_id)],
        checkpoint_written_at_iso: now_iso(),
        ..existing
    };
    upsert_recovery_checkpoint_blocking(app_handle, &checkpoint)?;
    Ok(checkpoint)
}

fn persist_thread_snapshot_success_checkpoint_blocking(
    app_handle: &AppHandle,
    requested_thread_id: &str,
    result: &serde_json::Value,
) -> Result<RecoveryCheckpoint, String> {
    // SECURITY: Validate response thread_id matches requested thread_id
    // to prevent stale responses from overwriting the wrong checkpoint
    let response_thread_id = extract_optional_string(result, &["threadId", "thread_id"]);
    if let Some(ref response_tid) = response_thread_id {
        if response_tid != requested_thread_id {
            return Err(format!(
                "stale thread snapshot response: expected thread_id={requested_thread_id}, got thread_id={response_tid}"
            ));
        }
    }

    let checkpoint =
        checkpoint_from_thread_snapshot_result(requested_thread_id, result, &now_iso(), Vec::new());
    upsert_recovery_checkpoint_blocking(app_handle, &checkpoint)?;
    Ok(checkpoint)
}

fn persist_thread_snapshot_failure_checkpoint_blocking(
    app_handle: &AppHandle,
    thread_id: &str,
    error: &str,
) -> Result<RecoveryCheckpoint, String> {
    let existing = load_recovery_checkpoint_blocking(app_handle, thread_id)?
        .unwrap_or_else(|| new_recovery_checkpoint(thread_id, &now_iso()));
    let reason = checkpoint_retry_reason_for_thread_read_error(error);
    let checkpoint = begin_fresh_retry(&existing, &now_iso(), Some(reason));

    if !crate::recovery_checkpoint::requires_fresh_retry(&existing, Some(reason)) {
        return Ok(existing);
    }

    upsert_recovery_checkpoint_blocking(app_handle, &checkpoint)?;
    Ok(checkpoint)
}

fn parse_i64_env(key: &str, default_value: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_value)
}

#[allow(dead_code)]
fn extract_repo_id(payload: &serde_json::Value) -> Option<i64> {
    payload
        .get("repoId")
        .or_else(|| payload.get("repo_id"))
        .and_then(serde_json::Value::as_i64)
        .filter(|value| *value > 0)
}

/// Classifies schema version for reconnect validation (reserved, P2-039)
#[allow(dead_code)]
fn schema_version_policy(schema_version: &str) -> SchemaVersionPolicy {
    if APP_SERVER_SCHEMA_SUPPORTED.contains(&schema_version) {
        return SchemaVersionPolicy::Supported;
    }
    if APP_SERVER_SCHEMA_DEPRECATED.contains(&schema_version) {
        return SchemaVersionPolicy::Deprecated;
    }
    SchemaVersionPolicy::Rejected
}

/// Validates reconnect payload schema version (reserved, P2-039)
#[allow(dead_code)]
fn validate_reconnect_payload(payload: &LiveSessionEventPayload) -> Result<(), &'static str> {
    let LiveSessionEventPayload::SessionDelta { payload, .. } = payload else {
        return Ok(());
    };

    let schema_version = payload
        .get("schemaVersion")
        .or_else(|| payload.get("schema_version"))
        .and_then(serde_json::Value::as_str);
    if let Some(schema_version) = schema_version {
        if matches!(
            schema_version_policy(schema_version),
            SchemaVersionPolicy::Rejected
        ) {
            return Err(RECONNECT_REASON_SCHEMA_MISMATCH);
        }
    }

    let session_valid = payload
        .get("sessionValid")
        .or_else(|| payload.get("session_valid"))
        .and_then(serde_json::Value::as_bool);
    if matches!(session_valid, Some(false)) {
        return Err(RECONNECT_REASON_SESSION_INVALID);
    }

    let token_valid = payload
        .get("tokenValid")
        .or_else(|| payload.get("token_valid"))
        .and_then(serde_json::Value::as_bool);
    let token_expired = payload
        .get("tokenExpired")
        .or_else(|| payload.get("token_expired"))
        .and_then(serde_json::Value::as_bool);
    if matches!(token_valid, Some(false)) || matches!(token_expired, Some(true)) {
        return Err(RECONNECT_REASON_TOKEN_INVALID);
    }

    Ok(())
}

#[allow(dead_code)]
fn apply_reconnect_validation_failure(runtime: &mut CodexAppServerRuntime, reason: &'static str) {
    runtime.status.stream_healthy = false;
    runtime.stream_session_state = StreamSessionState::Failed;
    if runtime.process_state != ProcessState::CrashLoop {
        runtime.process_state = ProcessState::Degraded;
    }
    runtime.status.last_error = Some(format!("Reconnect validation failed: {reason}"));
    increment_labeled_counter(&mut runtime.restart_total, reason);
    increment_labeled_counter(&mut runtime.parse_error_total, "protocol_violation");
    sync_status(runtime);
}

fn assert_initialized_handshake(runtime: &CodexAppServerRuntime) -> Result<(), String> {
    if runtime.handshake_state != HandshakeState::Initialized {
        return Err(
            "Not initialized: call initialize and wait for initialized handshake".to_string(),
        );
    }
    Ok(())
}

fn assert_thread_snapshot_access(runtime: &CodexAppServerRuntime) -> Result<(), String> {
    assert_initialized_handshake(runtime).map_err(|_| {
        "Handshake incomplete: initialize + initialized required before thread requests".to_string()
    })?;
    if runtime.auth_state != AuthState::Authenticated {
        return Err("Authentication required before requesting thread data".to_string());
    }
    Ok(())
}

fn live_sessions_ttl_hours() -> i64 {
    parse_i64_env(
        "NARRATIVE_LIVE_SESSIONS_TTL_HOURS",
        DEFAULT_LIVE_SESSIONS_TTL_HOURS,
    )
}

fn live_sessions_max_rows() -> i64 {
    parse_i64_env(
        "NARRATIVE_LIVE_SESSIONS_MAX_ROWS",
        DEFAULT_LIVE_SESSIONS_MAX_ROWS,
    )
}

#[allow(dead_code)]
async fn upsert_live_session(
    pool: &SqlitePool,
    payload: &LiveSessionEventPayload,
) -> Result<(), String> {
    let (thread_id, turn_id, item_id, event_type, source, status, payload_json, last_activity_at) =
        match payload {
            LiveSessionEventPayload::SessionDelta {
                thread_id,
                turn_id,
                item_id,
                event_type,
                source,
                received_at_iso,
                ..
            } => {
                let status = if event_type.to_lowercase().contains("completed") {
                    "completed"
                } else {
                    "active"
                };
                (
                    thread_id.clone(),
                    turn_id.clone(),
                    item_id.clone(),
                    event_type.clone(),
                    source.clone(),
                    status.to_string(),
                    serde_json::to_string(payload).map_err(|e| e.to_string())?,
                    received_at_iso.clone(),
                )
            }
            _ => return Ok(()),
        };

    sqlx::query(
        r#"
        INSERT INTO live_sessions (
          thread_id, turn_id, item_id, event_type, source, status, payload, last_activity_at, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, (strftime('%Y-%m-%dT%H:%M:%fZ','now')), (strftime('%Y-%m-%dT%H:%M:%fZ','now')))
        ON CONFLICT(thread_id, turn_id, item_id, event_type)
        DO UPDATE SET
          source = excluded.source,
          status = excluded.status,
          payload = excluded.payload,
          last_activity_at = excluded.last_activity_at,
          updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        "#,
    )
    .bind(thread_id)
    .bind(turn_id)
    .bind(item_id)
    .bind(event_type)
    .bind(source)
    .bind(status)
    .bind(payload_json)
    .bind(last_activity_at)
    .execute(pool)
    .await
    .map_err(|e| format!("failed to upsert live_sessions row: {e}"))?;

    Ok(())
}

async fn cleanup_live_sessions_with_policy(
    pool: &SqlitePool,
    ttl_hours: i64,
    max_rows: i64,
) -> Result<LiveSessionsCleanupResult, String> {
    let ttl_seconds = ttl_hours.saturating_mul(3600);

    let ttl_result = sqlx::query(
        "DELETE FROM live_sessions WHERE unixepoch(last_activity_at) < (unixepoch('now') - ?)",
    )
    .bind(ttl_seconds)
    .execute(pool)
    .await
    .map_err(|e| format!("failed to cleanup live_sessions by ttl: {e}"))?;

    let cap_result = sqlx::query(
        r#"
        DELETE FROM live_sessions
        WHERE id IN (
          SELECT id
          FROM live_sessions
          ORDER BY datetime(last_activity_at) DESC, id DESC
          LIMIT -1 OFFSET ?
        )
        "#,
    )
    .bind(max_rows)
    .execute(pool)
    .await
    .map_err(|e| format!("failed to cleanup live_sessions by row cap: {e}"))?;

    Ok(LiveSessionsCleanupResult {
        removed_rows: ttl_result.rows_affected() + cap_result.rows_affected(),
        ttl_hours,
        max_rows,
    })
}

fn cleanup_live_sessions_blocking(
    app_handle: &AppHandle,
) -> Result<Option<LiveSessionsCleanupResult>, String> {
    let Some(pool) = with_db_pool(app_handle) else {
        return Ok(None);
    };

    let ttl_hours = live_sessions_ttl_hours();
    let max_rows = live_sessions_max_rows();

    tauri::async_runtime::block_on(cleanup_live_sessions_with_policy(
        &pool, ttl_hours, max_rows,
    ))
    .map(Some)
}

#[allow(dead_code)]
fn persist_live_event_blocking(
    app_handle: &AppHandle,
    payload: &LiveSessionEventPayload,
) -> Result<(), String> {
    let Some(pool) = with_db_pool(app_handle) else {
        return Ok(());
    };

    tauri::async_runtime::block_on(async {
        upsert_live_session(&pool, payload).await?;
        if let LiveSessionEventPayload::SessionDelta {
            thread_id,
            turn_id,
            item_id,
            event_type,
            source,
            payload,
            received_at_iso,
            ..
        } = payload
        {
            if event_type.to_lowercase().contains("completed") {
                let Some(repo_id) = extract_repo_id(payload) else {
                    return Err(format!(
                        "missing repoId for completed session persistence (thread_id={}, turn_id={}, item_id={})",
                        thread_id, turn_id, item_id
                    ));
                };

                crate::import::commands::store_codex_app_server_completed_session(
                    &pool,
                    repo_id,
                    thread_id,
                    turn_id,
                    item_id,
                    event_type,
                    source,
                    payload,
                    received_at_iso,
                )
                .await
                .map_err(|err| format!("canonical session persistence failed: {err}"))?;

                let _ = cleanup_live_sessions_with_policy(
                    &pool,
                    live_sessions_ttl_hours(),
                    live_sessions_max_rows(),
                )
                .await?;
            }
        }
        Ok::<(), String>(())
    })
}

#[allow(dead_code)]
fn parse_event_from_legacy_input(event: CodexStreamEventInput) -> LiveSessionEventPayload {
    LiveSessionEventPayload::SessionDelta {
        thread_id: event.thread_id,
        turn_id: event.turn_id,
        item_id: event.item_id,
        event_type: event.event_type,
        source: normalize_source(&event.source),
        sequence_id: 0,
        received_at_iso: now_iso(),
        payload: event.payload.unwrap_or(serde_json::Value::Null),
    }
}

#[allow(dead_code)]
fn parser_validation_error(
    kind: &str,
    raw: &serde_json::Value,
    reason: &str,
) -> LiveSessionEventPayload {
    let mut preview = raw.to_string();
    if preview.len() > 512 {
        preview.truncate(512);
    }
    LiveSessionEventPayload::ParserValidationError {
        kind: kind.to_string(),
        raw_preview: preview,
        reason: reason.to_string(),
        occurred_at_iso: now_iso(),
    }
}

#[allow(dead_code)]
fn parse_live_payload(
    raw: serde_json::Value,
) -> Result<LiveSessionEventPayload, LiveSessionEventPayload> {
    match serde_json::from_value::<LiveSessionEventPayload>(raw.clone()) {
        Ok(parsed) => Ok(parsed),
        Err(_) => {
            let provider = raw
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("codex")
                .to_string();
            let thread_id = raw
                .get("threadId")
                .or_else(|| raw.get("thread_id"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let turn_id = raw
                .get("turnId")
                .or_else(|| raw.get("turn_id"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let item_id = raw
                .get("itemId")
                .or_else(|| raw.get("item_id"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            let event_type = raw
                .get("eventType")
                .or_else(|| raw.get("event_type"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);

            if let (Some(thread_id), Some(turn_id), Some(item_id), Some(event_type)) =
                (thread_id, turn_id, item_id, event_type)
            {
                let source = raw
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("app_server_stream");
                let payload = raw
                    .get("payload")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                return Ok(parse_event_from_legacy_input(CodexStreamEventInput {
                    provider,
                    thread_id,
                    turn_id,
                    item_id,
                    event_type,
                    source: source.to_string(),
                    payload: Some(payload),
                }));
            }

            Err(parser_validation_error(
                "missing_fields",
                &raw,
                "Unable to parse sidecar event payload",
            ))
        }
    }
}

fn apply_session_delta(
    runtime: &mut CodexAppServerRuntime,
    input: &CodexStreamEventInput,
) -> CodexStreamIngestResult {
    let key = event_identity_key(input);
    let incoming_source = normalize_source(&input.source);
    let event_type = input.event_type.trim().to_lowercase();

    let mut decision = "accepted".to_string();
    let mut replaced_source: Option<String> = None;
    let chosen_source;

    if let Some(existing_source) = runtime.dedupe_sources.get(&key).cloned() {
        if existing_source == incoming_source {
            decision = "duplicate".to_string();
            runtime.stream_events_duplicates += 1;
            chosen_source = existing_source;
        } else {
            let incoming_priority = source_priority(&event_type, &incoming_source);
            let existing_priority = source_priority(&event_type, &existing_source);
            if incoming_priority > existing_priority {
                remember_dedupe_source(runtime, &key, &incoming_source);
                decision = "replaced".to_string();
                runtime.stream_events_replaced += 1;
                replaced_source = Some(existing_source.clone());
                chosen_source = incoming_source;
            } else {
                decision = "dropped".to_string();
                runtime.stream_events_dropped += 1;
                chosen_source = existing_source;
            }
        }
    } else {
        remember_dedupe_source(runtime, &key, &incoming_source);
        runtime.stream_events_accepted += 1;
        chosen_source = incoming_source;
    }

    let log_entry = CodexStreamDedupeDecision {
        at_iso: now_iso(),
        key: key.clone(),
        decision: decision.clone(),
        incoming_source: normalize_source(&input.source),
        chosen_source: chosen_source.clone(),
        replaced_source: replaced_source.clone(),
    };
    runtime.recent_dedupe.push_front(log_entry);
    while runtime.recent_dedupe.len() > MAX_DEDUPE_LOG {
        runtime.recent_dedupe.pop_back();
    }

    if runtime.process_state == ProcessState::Running
        && runtime.handshake_state == HandshakeState::Initialized
        && runtime.auth_state == AuthState::Authenticated
        && matches!(decision.as_str(), "accepted" | "replaced")
    {
        runtime.status.stream_healthy = true;
        runtime.stream_session_state = if event_type.contains("completed") {
            StreamSessionState::Completed
        } else {
            StreamSessionState::Alive
        };
        sync_status(runtime);
    }

    CodexStreamIngestResult {
        key,
        decision,
        chosen_source,
        replaced_source,
    }
}

fn expire_pending_approvals(
    runtime: &mut CodexAppServerRuntime,
    now_ms: i64,
) -> Vec<LiveSessionEventPayload> {
    let mut expired_request_ids: Vec<String> = Vec::new();
    for (request_id, pending) in &runtime.approval_waiters {
        let expires_at_ms = pending
            .created_at_epoch_ms
            .saturating_add(pending.timeout_ms as i64);
        if now_ms >= expires_at_ms {
            expired_request_ids.push(request_id.clone());
        }
    }

    let mut results = Vec::new();
    for request_id in expired_request_ids {
        if let Some(pending) = runtime.approval_waiters.remove(&request_id) {
            let payload = LiveSessionEventPayload::ApprovalResult {
                request_id,
                thread_id: pending.thread_id,
                approved: false,
                decided_at_iso: now_iso(),
                decided_by: Some("system".to_string()),
                reason: Some("Timed out waiting for approval response".to_string()),
            };
            remember_approval_decision(
                runtime,
                &payload,
                Some(pending.decision_token.as_str()),
                Some(pending.approval_window_id),
            );
            increment_labeled_counter(&mut runtime.approval_result_total, "timeout");
            results.push(payload);
        }
    }

    results
}

fn handle_live_event_internal(
    runtime: &mut CodexAppServerRuntime,
    payload: &LiveSessionEventPayload,
) -> Option<CodexStreamIngestResult> {
    runtime.event_sequence_id = runtime.event_sequence_id.saturating_add(1);

    match payload {
        LiveSessionEventPayload::SessionDelta {
            thread_id,
            turn_id,
            item_id,
            event_type,
            source,
            payload,
            ..
        } => {
            let input = CodexStreamEventInput {
                provider: "codex".to_string(),
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                event_type: event_type.clone(),
                source: source.clone(),
                payload: Some(payload.clone()),
            };
            runtime.last_stream_event_at_epoch_ms = Some(now_epoch_ms());
            Some(apply_session_delta(runtime, &input))
        }
        LiveSessionEventPayload::ApprovalRequest {
            request_id,
            thread_id,
            timeout_ms,
            rpc_request_id,
            decision_token,
            ..
        } => {
            let Some(decision_token) = decision_token.clone() else {
                return None;
            };
            if runtime.approval_waiters.len() >= MAX_PENDING_APPROVAL_WAITERS {
                if let Some((oldest_request_id, _)) = runtime
                    .approval_waiters
                    .iter()
                    .min_by_key(|(_, pending)| pending.created_at_epoch_ms)
                    .map(|(id, pending)| (id.clone(), pending.created_at_epoch_ms))
                {
                    runtime.approval_waiters.remove(&oldest_request_id);
                    increment_labeled_counter(
                        &mut runtime.parse_error_total,
                        "approval_waiter_overflow",
                    );
                    if runtime.process_state != ProcessState::CrashLoop {
                        runtime.process_state = ProcessState::Degraded;
                    }
                    runtime.status.last_error = Some(format!(
                        "Approval waiter overflow: evicted oldest request {oldest_request_id}"
                    ));
                }
            }
            runtime.approval_waiters.insert(
                request_id.clone(),
                PendingApproval {
                    thread_id: thread_id.clone(),
                    created_at_epoch_ms: now_epoch_ms(),
                    timeout_ms: *timeout_ms,
                    decision_token,
                    approval_window_id: runtime.approval_window_id,
                    rpc_request_id: rpc_request_id.clone(),
                },
            );
            None
        }
        LiveSessionEventPayload::ApprovalResult {
            request_id,
            approved,
            ..
        } => {
            let pending = runtime.approval_waiters.remove(request_id);
            remember_approval_decision(
                runtime,
                payload,
                pending
                    .as_ref()
                    .map(|record| record.decision_token.as_str()),
                pending.as_ref().map(|record| record.approval_window_id),
            );
            increment_labeled_counter(
                &mut runtime.approval_result_total,
                if *approved { "approved" } else { "rejected" },
            );
            None
        }
        LiveSessionEventPayload::ParserValidationError { kind, .. } => {
            increment_labeled_counter(&mut runtime.parse_error_total, kind);
            None
        }
    }
}

fn validate_approval_submission_context(
    runtime: &mut CodexAppServerRuntime,
    request_id: &str,
    thread_id: &str,
    decision_token: &str,
) -> Result<PendingApproval, String> {
    if decision_token.trim().is_empty() {
        increment_labeled_counter(&mut runtime.approval_result_total, "invalid_token");
        return Err(format!(
            "approval submission rejected for {request_id}: missing decision token"
        ));
    }

    let Some(pending) = runtime.approval_waiters.get(request_id).cloned() else {
        if let Some(record) = runtime.approval_decisions.get(request_id) {
            increment_labeled_counter(&mut runtime.approval_result_total, "replay_rejected");
            return Err(format!(
                "approval replay rejected for request_id={request_id} (window={}, token_hash={})",
                record.approval_window_id, record.decision_token_hash
            ));
        }
        return Err(format!("approval request not found: {request_id}"));
    };

    if pending.approval_window_id != runtime.approval_window_id {
        runtime.approval_waiters.remove(request_id);
        increment_labeled_counter(&mut runtime.approval_result_total, "stale_context");
        return Err(format!(
            "approval submission rejected for {request_id}: stale approval window context"
        ));
    }

    if pending.thread_id != thread_id {
        increment_labeled_counter(&mut runtime.approval_result_total, "context_mismatch");
        return Err(format!(
            "approval submission rejected for {request_id}: thread context mismatch"
        ));
    }

    if pending.decision_token != decision_token {
        increment_labeled_counter(&mut runtime.approval_result_total, "invalid_token");
        return Err(format!(
            "approval submission rejected for {request_id}: invalid decision token"
        ));
    }

    Ok(pending)
}

#[allow(dead_code)]
fn command_not_exposed_error(command: &str) -> String {
    format!("command-not-exposed: {command} is internal-only; use sidecar event bridge")
}

fn terminate_child_with_timeout(child: &mut Child, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    child
                        .kill()
                        .map_err(|e| format!("failed to kill sidecar process: {e}"))?;
                    let _ = child.wait();
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(format!("failed while waiting for sidecar exit: {err}")),
        }
    }
}

fn schedule_cleanup_task(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = cleanup_live_sessions_blocking(&app_handle) {
            eprintln!("Narrative: live_sessions cleanup failed: {err}");
        }
    });
}

#[command(rename_all = "camelCase")]
pub fn get_codex_app_server_status(
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAppServerStatus, String> {
    let runtime = state.inner.lock().map_err(|e| e.to_string())?;
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn start_codex_app_server(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAppServerStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;

    if runtime.sidecar.is_some() {
        return Ok(runtime.status.clone());
    }

    runtime.process_state = ProcessState::Starting;
    runtime.stream_session_state = StreamSessionState::Expected;
    runtime.handshake_state = HandshakeState::NotStarted;
    runtime.auth_state = AuthState::NeedsLogin;
    runtime.status.stream_healthy = false;
    runtime.status.last_error = None;
    let _ = cancel_pending_rpcs(&mut runtime, "rpc_cancelled_start");
    runtime.next_rpc_id = 1;
    runtime.sidecar_stderr_ring.clear();
    runtime.sidecar_stderr_dropped = 0;
    runtime.last_stream_event_at_epoch_ms = None;
    runtime.approval_waiters.clear();
    runtime.approval_window_id = generate_approval_window_id();
    runtime.thread_read_results.clear();
    runtime.thread_read_errors.clear();
    sync_status(&mut runtime);

    if runtime.status.stream_kill_switch {
        runtime.stream_session_state = StreamSessionState::Disabled;
        set_process_state(
            &mut runtime,
            ProcessState::Degraded,
            Some("Stream enrichment kill-switch enabled".to_string()),
        );
        emit_status(&app_handle, &runtime.status);
        return Ok(runtime.status.clone());
    }

    let sidecar_path = match detect_sidecar_path(&app_handle) {
        Ok(path) => path,
        Err(error) => {
            let cause = if error.contains("trust policy")
                || error.contains("checksum")
                || error.contains("signature")
                || error.contains("override is blocked")
            {
                "spawn_path_untrusted"
            } else {
                "spawn_path_missing"
            };
            register_start_failure(&mut runtime, error, cause);
            emit_status(&app_handle, &runtime.status);
            return Ok(runtime.status.clone());
        }
    };

    match spawn_sidecar_process(&sidecar_path) {
        Ok(mut sidecar) => {
            let Some(stdout) = sidecar.child.stdout.take() else {
                register_start_failure(
                    &mut runtime,
                    "Failed to capture Codex App Server stdout".to_string(),
                    "stdout_missing",
                );
                emit_status(&app_handle, &runtime.status);
                return Ok(runtime.status.clone());
            };
            let Some(stderr) = sidecar.child.stderr.take() else {
                register_start_failure(
                    &mut runtime,
                    "Failed to capture Codex App Server stderr".to_string(),
                    "stderr_missing",
                );
                emit_status(&app_handle, &runtime.status);
                return Ok(runtime.status.clone());
            };
            runtime.sidecar_path = Some(sidecar_path);
            runtime.sidecar = Some(sidecar);
            runtime.process_state = ProcessState::Running;
            runtime.status.stream_healthy = false;
            runtime.status.last_error = Some(format!(
                "Sidecar started; waiting for initialize/initialized/authenticated handshake (protocol target: {APP_SERVER_PROTOCOL_TARGET})"
            ));
            sync_status(&mut runtime);

            let cancel = Arc::new(AtomicBool::new(false));
            runtime.monitor_cancel = cancel.clone();
            runtime.monitor_handle = Some(spawn_monitor_thread(
                app_handle.clone(),
                state.inner.clone(),
                cancel,
            ));
            spawn_sidecar_stdout_reader(
                app_handle.clone(),
                state.inner.clone(),
                runtime.monitor_cancel.clone(),
                stdout,
            );
            spawn_sidecar_stderr_reader(
                state.inner.clone(),
                runtime.monitor_cancel.clone(),
                stderr,
            );
            emit_status(&app_handle, &runtime.status);
            drop(runtime);
            schedule_cleanup_task(app_handle);
        }
        Err(err) => {
            register_start_failure(&mut runtime, err, "spawn_failed");
            emit_status(&app_handle, &runtime.status);
        }
    }

    let runtime = state.inner.lock().map_err(|e| e.to_string())?;
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn stop_codex_app_server(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAppServerStatus, String> {
    let (mut sidecar, monitor_handle, cancel) = {
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        runtime.process_state = ProcessState::Stopping;
        runtime.status.stream_healthy = false;
        runtime.stream_session_state = StreamSessionState::Failed;
        sync_status(&mut runtime);
        emit_status(&app_handle, &runtime.status);

        (
            runtime.sidecar.take(),
            runtime.monitor_handle.take(),
            runtime.monitor_cancel.clone(),
        )
    };

    cancel.store(true, Ordering::SeqCst);

    if let Some(ref mut process) = sidecar {
        let _ = terminate_child_with_timeout(
            &mut process.child,
            Duration::from_millis(SHUTDOWN_GRACE_MS),
        );
    }

    if let Some(handle) = monitor_handle {
        let _ = handle.join();
    }

    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    runtime.sidecar = None;
    runtime.stream_session_state = StreamSessionState::Disabled;
    runtime.handshake_state = HandshakeState::NotStarted;
    runtime.auth_state = AuthState::NeedsLogin;
    runtime.status.stream_healthy = false;
    set_process_state(&mut runtime, ProcessState::Inactive, None);
    let _ = cancel_pending_rpcs(&mut runtime, "rpc_cancelled_stop");
    runtime.next_rpc_id = 1;
    runtime.sidecar_stderr_ring.clear();
    runtime.sidecar_stderr_dropped = 0;
    runtime.last_stream_event_at_epoch_ms = None;
    runtime.approval_waiters.clear();
    runtime.approval_window_id = generate_approval_window_id();
    runtime.thread_read_results.clear();
    runtime.thread_read_errors.clear();
    emit_status(&app_handle, &runtime.status);
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_initialize(
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAppServerStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    if runtime.process_state == ProcessState::Inactive
        || runtime.process_state == ProcessState::CrashLoop
    {
        return Err("App Server is not running; cannot send initialize handshake".to_string());
    }
    if runtime.handshake_state == HandshakeState::Initialized {
        return Err("Already initialized".to_string());
    }
    if has_pending_initialize_request(&runtime) {
        return Err("initialize request already in-flight".to_string());
    }
    let initialize_request = serde_json::json!({
        "clientInfo": {
            "name": "trace-narrative",
            "title": "Firefly Narrative",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    send_sidecar_request(
        &mut runtime,
        METHOD_INITIALIZE,
        initialize_request,
        PendingRpcKind::Initialize,
    )?;
    runtime.status.last_error = Some("Initialize request sent; waiting for response".to_string());
    sync_status(&mut runtime);
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_initialized(
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAppServerStatus, String> {
    {
        let runtime = state.inner.lock().map_err(|e| e.to_string())?;
        if runtime.handshake_state == HandshakeState::Initialized {
            return Err("Already initialized".to_string());
        }
        if runtime.handshake_state == HandshakeState::NotStarted
            && !has_pending_initialize_request(&runtime)
        {
            return Err("Not initialized: initialize must be called first".to_string());
        }
    }

    wait_for_initialize_ack(&state.inner, Duration::from_millis(RPC_REQUEST_TIMEOUT_MS))?;

    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    if runtime.handshake_state != HandshakeState::InitializeSent {
        return Err("Not initialized: initialize response not ready".to_string());
    }

    let initialized_notification = serde_json::json!({
        "method": METHOD_INITIALIZED,
        "params": {}
    });
    send_sidecar_message(&mut runtime, &initialized_notification)?;
    let account_read_request = serde_json::json!({ "refreshToken": false });
    send_sidecar_request(
        &mut runtime,
        METHOD_ACCOUNT_READ,
        account_read_request,
        PendingRpcKind::AccountRead,
    )?;
    runtime.handshake_state = HandshakeState::Initialized;
    runtime.stream_session_state = StreamSessionState::Expected;
    sync_status(&mut runtime);
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_account_read(
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAccountStatus, String> {
    let runtime = state.inner.lock().map_err(|e| e.to_string())?;
    let mode = runtime.status.auth_mode.clone();
    let interactive_login_required = matches!(mode.as_str(), "chatgpt" | "chatgptAuthTokens");
    Ok(CodexAccountStatus {
        auth_state: runtime.status.auth_state.clone(),
        auth_mode: mode,
        interactive_login_required,
        supported_modes: supported_auth_modes(),
    })
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_account_login_start(
    state: State<'_, CodexAppServerState>,
    auth_mode: Option<String>,
) -> Result<CodexAccountStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    assert_initialized_handshake(&runtime)?;
    let requested_mode = normalize_auth_mode(auth_mode.as_deref().unwrap_or("chatgpt"));
    let Some(login_type) = auth_mode_to_login_start_type(&requested_mode) else {
        return Err(format!(
            "Unsupported auth mode: {requested_mode}. Expected one of {}",
            SUPPORTED_AUTH_MODES.join(", ")
        ));
    };
    let login_start_request = serde_json::json!({ "type": login_type });
    send_sidecar_request(
        &mut runtime,
        METHOD_ACCOUNT_LOGIN_START,
        login_start_request,
        PendingRpcKind::AccountLoginStart,
    )?;
    runtime.status.auth_mode = requested_mode.clone();
    runtime.auth_state = AuthState::Authenticating;
    sync_status(&mut runtime);
    Ok(CodexAccountStatus {
        auth_state: runtime.status.auth_state.clone(),
        auth_mode: requested_mode.clone(),
        interactive_login_required: requested_mode != "apikey",
        supported_modes: supported_auth_modes(),
    })
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_account_chatgpt_auth_tokens_refresh(
    state: State<'_, CodexAppServerState>,
    access_token: String,
    refresh_token: Option<String>,
) -> Result<CodexAccountStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    assert_initialized_handshake(&runtime)?;

    let trimmed_access_token = access_token.trim();
    if trimmed_access_token.is_empty() {
        return Err("accessToken is required for chatgptAuthTokens refresh".to_string());
    }

    let refresh_request = serde_json::json!({
        "accessToken": trimmed_access_token,
        "refreshToken": refresh_token.map(|token| token.trim().to_string()),
    });
    send_sidecar_request(
        &mut runtime,
        METHOD_ACCOUNT_CHATGPT_TOKENS_REFRESH,
        refresh_request,
        PendingRpcKind::AccountChatgptTokensRefresh,
    )?;
    runtime.auth_state = AuthState::Authenticating;
    runtime.status.auth_mode = "chatgptAuthTokens".to_string();
    runtime.status.last_error = Some("Refreshing chatgptAuthTokens session".to_string());
    sync_status(&mut runtime);

    Ok(CodexAccountStatus {
        auth_state: runtime.status.auth_state.clone(),
        auth_mode: runtime.status.auth_mode.clone(),
        interactive_login_required: true,
        supported_modes: supported_auth_modes(),
    })
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_account_logout(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
) -> Result<CodexAccountStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    runtime.auth_state = AuthState::LoggedOut;
    runtime.handshake_state = HandshakeState::NotStarted;
    runtime.status.stream_healthy = false;
    runtime.status.last_error = Some("Logged out".to_string());
    runtime.stream_session_state = StreamSessionState::Failed;

    // Reset reliability metrics on logout to ensure fresh state on next login
    runtime.stream_events_accepted = 0;
    runtime.stream_events_duplicates = 0;
    runtime.stream_events_dropped = 0;
    runtime.stream_events_replaced = 0;
    runtime.last_stream_event_at_epoch_ms = None;

    // Clear pending RPCs and approvals
    let _ = cancel_pending_rpcs(&mut runtime, "logged_out");
    runtime.approval_waiters.clear();
    runtime.approval_decisions.clear();
    runtime.approval_decision_order.clear();

    // Capture mode before emitting status
    let mode = runtime.status.auth_mode.clone();
    sync_status(&mut runtime);
    drop(runtime);

    // Emit reliability status refresh after logout
    let _ = get_capture_reliability_status(app_handle, state);

    Ok(CodexAccountStatus {
        auth_state: AuthState::LoggedOut.as_status().to_string(),
        auth_mode: mode.clone(),
        interactive_login_required: mode != "apikey",
        supported_modes: supported_auth_modes(),
    })
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_set_stream_kill_switch(
    state: State<'_, CodexAppServerState>,
    enabled: bool,
) -> Result<CodexAppServerStatus, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    runtime.status.stream_kill_switch = enabled;
    if enabled {
        runtime.process_state = ProcessState::Degraded;
        runtime.stream_session_state = StreamSessionState::Disabled;
        runtime.status.stream_healthy = false;
        runtime.status.last_error = Some("Stream enrichment disabled by kill-switch".to_string());
    }
    sync_status(&mut runtime);
    Ok(runtime.status.clone())
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_request_thread_snapshot(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    thread_id: String,
) -> Result<serde_json::Value, String> {
    let request_thread_id = thread_id.trim().to_string();
    if request_thread_id.is_empty() {
        return Err("threadId is required".to_string());
    }

    // Validate access BEFORE preparing checkpoint to avoid writing state for denied requests
    let request_id = {
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        assert_thread_snapshot_access(&runtime)?;
        send_sidecar_request(
            &mut runtime,
            METHOD_THREAD_READ,
            serde_json::json!({ "threadId": request_thread_id }),
            PendingRpcKind::ThreadRead,
        )?
    };

    // Only prepare checkpoint after access is validated
    if let Err(err) = prepare_thread_snapshot_checkpoint_blocking(&app_handle, &request_thread_id) {
        // Cancel the queued RPC to prevent orphaned pending requests on DB write failure
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        cancel_pending_rpc_by_id(&mut runtime, request_id, "checkpoint_prepare_failed");
        return Err(format!("failed to prepare trust recovery checkpoint: {err}"));
    }
    if let Err(err) = persist_inflight_thread_snapshot_checkpoint_blocking(
        &app_handle,
        &request_thread_id,
        request_id,
    ) {
        // Cancel the queued RPC to prevent orphaned pending requests on DB write failure
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        cancel_pending_rpc_by_id(&mut runtime, request_id, "checkpoint_persist_failed");
        return Err(format!("failed to persist inflight trust recovery checkpoint: {err}"));
    }

    let response = wait_for_thread_read_response(
        &state.inner,
        request_id,
        Duration::from_millis(RPC_REQUEST_TIMEOUT_MS),
    );

    match &response {
        Ok(result) => {
            persist_thread_snapshot_success_checkpoint_blocking(
                &app_handle,
                &request_thread_id,
                result,
            )
            .map_err(|err| format!("failed to persist trust recovery checkpoint: {err}"))?;
        }
        Err(error) => {
            persist_thread_snapshot_failure_checkpoint_blocking(
                &app_handle,
                &request_thread_id,
                error,
            )
            .map_err(|err| {
                format!("failed to persist fresh-retry trust recovery checkpoint: {err}")
            })?;
        }
    }

    response
}

/// Load the recovery checkpoint for a thread at startup/restart to determine
/// trust state before hydrating. Call this after handshake completes.
///
/// This is the startup recovery path - it reads durable checkpoint state
/// to influence trust promotion decisions before any live hydrate request.
#[command(rename_all = "camelCase")]
pub fn codex_app_server_load_thread_recovery_checkpoint(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    thread_id: String,
) -> Result<CodexThreadRecoveryCheckpointStatus, String> {
    let request_thread_id = thread_id.trim().to_string();
    if request_thread_id.is_empty() {
        return Err("threadId is required".to_string());
    }

    // Verify handshake is complete before allowing recovery checkpoint access
    {
        let runtime = state.inner.lock().map_err(|e| e.to_string())?;
        if runtime.handshake_state != HandshakeState::Initialized {
            return Err(
                "Handshake not complete; cannot load recovery checkpoint before initialization"
                    .to_string(),
            );
        }
    }

    // Load existing checkpoint from durable storage
    let existing = load_recovery_checkpoint_blocking(&app_handle, &request_thread_id)?;

    match existing {
        None => {
            // No checkpoint exists - fresh start, no prior state to recover
            Ok(CodexThreadRecoveryCheckpointStatus {
                thread_id: request_thread_id,
                checkpoint_exists: false,
                requires_fresh_retry: false,
                trust_state_recommendation: "none".to_string(),
                checkpoint: None,
                fresh_retry_reason: None,
            })
        }
        Some(checkpoint) => {
            // Check if checkpoint is compatible or requires fresh retry
            let needs_fresh_retry =
                requires_fresh_retry(&checkpoint, checkpoint.trust_pause_reason.as_deref());

            if needs_fresh_retry {
                // Checkpoint exists but is incompatible/corrupted - recommend trust_paused
                // The frontend should offer recovery affordances (retry-hydrate, checkpoint-reset)
                let reason = determine_fresh_retry_reason(&checkpoint);
                Ok(CodexThreadRecoveryCheckpointStatus {
                    thread_id: request_thread_id,
                    checkpoint_exists: true,
                    requires_fresh_retry: true,
                    trust_state_recommendation: "trust_paused".to_string(),
                    checkpoint: Some(checkpoint),
                    fresh_retry_reason: Some(reason),
                })
            } else {
                // Checkpoint exists and is compatible - can resume from this state
                // The frontend can proceed with trust promotion after hydrate
                Ok(CodexThreadRecoveryCheckpointStatus {
                    thread_id: request_thread_id,
                    checkpoint_exists: true,
                    requires_fresh_retry: false,
                    trust_state_recommendation: "replaying".to_string(),
                    checkpoint: Some(checkpoint),
                    fresh_retry_reason: None,
                })
            }
        }
    }
}

/// Determine the reason why a checkpoint requires fresh retry.
/// Used to provide actionable feedback to the user.
fn determine_fresh_retry_reason(checkpoint: &RecoveryCheckpoint) -> String {
    if checkpoint.schema_version != crate::recovery_checkpoint::RECOVERY_CHECKPOINT_SCHEMA_VERSION
    {
        return format!(
            "checkpoint schema version {} is incompatible (expected {})",
            checkpoint.schema_version,
            crate::recovery_checkpoint::RECOVERY_CHECKPOINT_SCHEMA_VERSION
        );
    }

    if checkpoint
        .thread_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return "checkpoint has missing or empty thread_id".to_string();
    }

    if checkpoint.replay_cursor.is_none() && checkpoint.last_applied_event_seq.is_some() {
        return "checkpoint has event sequence but no replay cursor".to_string();
    }

    // Default reason for other incompatible states
    "checkpoint state is incompatible or corrupted".to_string()
}

#[command(rename_all = "camelCase")]
#[allow(dead_code)]
pub fn codex_app_server_receive_live_event(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    payload: serde_json::Value,
) -> Result<Option<CodexStreamIngestResult>, String> {
    let parsed = match parse_live_payload(payload.clone()) {
        Ok(parsed) => parsed,
        Err(parser_error) => {
            let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
            handle_live_event_internal(&mut runtime, &parser_error);
            emit_live_session_event(&app_handle, &parser_error);
            emit_status(&app_handle, &runtime.status);
            return Err("protocol-violation: failed to parse sidecar payload".to_string());
        }
    };

    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    let parsed = bind_approval_request_token(parsed);

    if let Err(reason) = validate_reconnect_payload(&parsed) {
        apply_reconnect_validation_failure(&mut runtime, reason);
        let parser_error = LiveSessionEventPayload::ParserValidationError {
            kind: "protocol_violation".to_string(),
            raw_preview: "reconnect validation failed".to_string(),
            reason: format!("Reconnect validation failed: {reason}"),
            occurred_at_iso: now_iso(),
        };
        handle_live_event_internal(&mut runtime, &parser_error);
        emit_live_session_event(&app_handle, &parser_error);
        emit_status(&app_handle, &runtime.status);
        return Err(format!("reconnect-validation-failed:{reason}"));
    }

    for timed_out in expire_pending_approvals(&mut runtime, now_epoch_ms()) {
        emit_live_session_event(&app_handle, &timed_out);
    }

    let result = handle_live_event_internal(&mut runtime, &parsed);

    emit_live_session_event(&app_handle, &parsed);

    if let LiveSessionEventPayload::SessionDelta { .. } = &parsed {
        if let Err(err) = persist_live_event_blocking(&app_handle, &parsed) {
            let reason = format!("Dropped completion persistence due to error: {err}");
            eprintln!("Narrative: {reason}");
            let parser_error = LiveSessionEventPayload::ParserValidationError {
                kind: "protocol_violation".to_string(),
                raw_preview: "session persistence failed".to_string(),
                reason,
                occurred_at_iso: now_iso(),
            };
            handle_live_event_internal(&mut runtime, &parser_error);
            emit_live_session_event(&app_handle, &parser_error);
        }
    }

    emit_status(&app_handle, &runtime.status);
    Ok(result)
}

#[command(rename_all = "camelCase")]
pub fn codex_app_server_submit_approval(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    request_id: String,
    thread_id: String,
    decision_token: String,
    approved: bool,
    reason: Option<String>,
) -> Result<LiveSessionEventPayload, String> {
    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    for timed_out in expire_pending_approvals(&mut runtime, now_epoch_ms()) {
        emit_live_session_event(&app_handle, &timed_out);
    }

    let pending = match validate_approval_submission_context(
        &mut runtime,
        &request_id,
        &thread_id,
        &decision_token,
    ) {
        Ok(pending) => pending,
        Err(reason) => {
            let user_error = if reason.contains("missing decision token") {
                "approval decision token is required".to_string()
            } else if reason.contains("stale approval window context") {
                "approval request is stale; refresh required".to_string()
            } else if reason.contains("thread context mismatch") {
                "approval submission context mismatch".to_string()
            } else if reason.contains("invalid decision token") {
                "approval decision token invalid".to_string()
            } else if reason.contains("approval replay rejected") {
                format!("approval request already decided: {request_id}")
            } else {
                reason.clone()
            };

            let redacted_reason = redact_sensitive_stderr_line(&reason);
            let runtime_error = redacted_reason.clone();
            let audit = LiveSessionEventPayload::ParserValidationError {
                kind: "protocol_violation".to_string(),
                raw_preview: METHOD_APPROVAL_SUBMIT.to_string(),
                reason: redacted_reason,
                occurred_at_iso: now_iso(),
            };
            handle_live_event_internal(&mut runtime, &audit);
            emit_live_session_event(&app_handle, &audit);
            runtime.status.last_error = Some(runtime_error);
            return Err(user_error);
        }
    };

    let response_request_id = request_id.clone();
    let response_thread_id = pending.thread_id.clone();
    let response_reason = reason.clone();
    let payload = LiveSessionEventPayload::ApprovalResult {
        request_id,
        thread_id: pending.thread_id,
        approved,
        decided_at_iso: now_iso(),
        decided_by: Some("user".to_string()),
        reason,
    };

    if let Some(rpc_request_id) = pending.rpc_request_id.as_ref() {
        let response_result = serde_json::json!({
            "requestId": response_request_id,
            "threadId": response_thread_id,
            "approved": approved,
            "decision": if approved { "approved" } else { "denied" },
            "reason": response_reason
        });
        if let Err(error) =
            send_jsonrpc_result_response(&mut runtime, rpc_request_id, response_result)
        {
            increment_labeled_counter(
                &mut runtime.parse_error_total,
                "approval_response_write_failed",
            );
            runtime.status.last_error = Some(format!(
                "Failed to write approval response to sidecar: {error}"
            ));
            return Err("failed to submit approval response to sidecar".to_string());
        }
    }

    handle_live_event_internal(&mut runtime, &payload);
    emit_live_session_event(&app_handle, &payload);
    Ok(payload)
}

#[command(rename_all = "camelCase")]
pub fn get_codex_stream_dedupe_log(
    state: State<'_, CodexAppServerState>,
    limit: Option<usize>,
) -> Result<Vec<CodexStreamDedupeDecision>, String> {
    let runtime = state.inner.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    Ok(runtime.recent_dedupe.iter().take(limit).cloned().collect())
}

#[command(rename_all = "camelCase")]
pub fn get_capture_reliability_status(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
) -> Result<CaptureReliabilityStatus, String> {
    let config = crate::ingest_config::load_config().unwrap_or_default();

    let has_base_watch_paths = !config.watch_paths.claude.is_empty()
        || !config.watch_paths.cursor.is_empty()
        || ((config.codex.mode == "logs" || config.codex.mode == "both")
            && !config.watch_paths.codex_logs.is_empty());
    let otel_baseline_healthy =
        config.auto_ingest_enabled && (has_base_watch_paths || config.codex.receiver_enabled);

    let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
    let app_server = runtime.status.clone();

    let stream_expected = config.codex.stream_enrichment_enabled
        && !config.codex.stream_kill_switch
        && !app_server.stream_kill_switch;
    let stream_healthy = matches!(runtime.process_state, ProcessState::Running)
        && runtime.handshake_state == HandshakeState::Initialized
        && runtime.auth_state == AuthState::Authenticated
        && app_server.stream_healthy
        && runtime.stream_session_state != StreamSessionState::Failed;

    let mode = if otel_baseline_healthy && stream_healthy {
        "HYBRID_ACTIVE"
    } else if otel_baseline_healthy && stream_expected {
        "DEGRADED_STREAMING"
    } else if otel_baseline_healthy {
        "OTEL_ONLY"
    } else {
        "FAILURE"
    }
    .to_string();

    let mut reasons = Vec::new();
    if !otel_baseline_healthy {
        reasons.push("OTEL baseline capture is unavailable".to_string());
    }
    if stream_expected && !stream_healthy {
        reasons.push("Codex streaming unavailable; falling back to OTEL baseline".to_string());
    }
    if let Some(err) = &app_server.last_error {
        reasons.push(err.clone());
    }

    emit_mode_transition(
        &app_handle,
        &mut runtime,
        &mode,
        reasons
            .first()
            .map(String::as_str)
            .unwrap_or("capture mode evaluated"),
    );

    let now_ms = now_epoch_ms();
    let time_since_last_stream_event_ms = runtime
        .last_stream_event_at_epoch_ms
        .map(|last| now_ms.saturating_sub(last));

    Ok(CaptureReliabilityStatus {
        mode,
        otel_baseline_healthy,
        stream_expected,
        stream_healthy,
        reasons,
        metrics: CaptureReliabilityMetrics {
            stream_events_accepted: runtime.stream_events_accepted,
            stream_events_duplicates: runtime.stream_events_duplicates,
            stream_events_dropped: runtime.stream_events_dropped,
            stream_events_replaced: runtime.stream_events_replaced,
            parser_validation_errors_total: counter_total(&runtime.parse_error_total),
            rpc_timeouts_total: runtime
                .parse_error_total
                .get("rpc_timeout")
                .copied()
                .unwrap_or(0),
            approval_timeouts_total: runtime
                .approval_result_total
                .get("timeout")
                .copied()
                .unwrap_or(0),
            restart_events_total: counter_total(&runtime.restart_total),
            pending_rpcs: runtime.pending_rpcs.len(),
            pending_approvals: runtime.approval_waiters.len(),
            sidecar_stderr_buffered: runtime.sidecar_stderr_ring.len(),
            sidecar_stderr_dropped: runtime.sidecar_stderr_dropped,
            time_since_last_stream_event_ms,
        },
        transitions: runtime.transitions.iter().cloned().collect(),
        app_server,
    })
}

/// TODO(2026-02-24): migration-safe deprecation path — keep command shape for one release,
/// but reject renderer mutation attempts. Remove in next release after bridge rollout verification.
#[command(rename_all = "camelCase")]
#[allow(dead_code)]
pub fn codex_app_server_set_stream_health(
    _healthy: bool,
    _reason: Option<String>,
) -> Result<CodexAppServerStatus, String> {
    Err(command_not_exposed_error(
        "codex_app_server_set_stream_health",
    ))
}

/// TODO(2026-02-24): migration-safe deprecation path — keep command shape for one release,
/// but reject renderer mutation attempts. Remove in next release after bridge rollout verification.
#[command(rename_all = "camelCase")]
#[allow(dead_code)]
pub fn ingest_codex_stream_event(
    _event: CodexStreamEventInput,
) -> Result<CodexStreamIngestResult, String> {
    Err(command_not_exposed_error("ingest_codex_stream_event"))
}

/// Reset the recovery checkpoint for a thread to a fresh retry state.
///
/// This clears the replay cursor and event sequence, forcing the next hydration
/// to start fresh rather than attempting to resume from a potentially corrupted state.
/// Use this when the app server is in `trust_paused` state due to hydration failures.
///
/// # Arguments
/// * `thread_id` - The thread ID whose checkpoint should be reset
///
/// # Returns
/// `true` if a checkpoint was found and reset, `false` if no checkpoint existed
#[command(rename_all = "camelCase")]
pub fn codex_app_server_retry_hydrate(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    thread_id: String,
) -> Result<bool, String> {
    let request_thread_id = thread_id.trim().to_string();
    if request_thread_id.is_empty() {
        return Err("threadId is required".to_string());
    }

    // Verify handshake is complete before allowing recovery operations
    {
        let runtime = state.inner.lock().map_err(|e| e.to_string())?;
        if runtime.handshake_state != HandshakeState::Initialized {
            return Err(
                "Handshake not complete; cannot retry hydrate before initialization".to_string(),
            );
        }
    }

    let pool = app_handle
        .try_state::<crate::DbState>()
        .map(|s| s.0.clone())
        .ok_or("Database pool not available")?;

    // Load existing checkpoint
    let existing = tauri::async_runtime::block_on(async {
        load_recovery_checkpoint(&pool, &request_thread_id).await
    })?;

    let Some(checkpoint) = existing else {
        return Ok(false);
    };

    // Create fresh retry checkpoint (clears replay state)
    let fresh = begin_fresh_retry(&checkpoint, &now_iso(), checkpoint.trust_pause_reason.as_deref());

    // Persist the reset checkpoint
    tauri::async_runtime::block_on(async { upsert_recovery_checkpoint(&pool, &fresh).await })?;

    // Emit status update to refresh UI
    {
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        emit_status(&app_handle, &runtime.status);
    }

    Ok(true)
}

/// Clear the recovery checkpoint for a thread entirely.
///
/// This removes all recovery state for the thread, effectively treating it as
/// a new thread on the next hydration attempt. Use this when the checkpoint
/// state is stale or no longer relevant.
///
/// # Arguments
/// * `thread_id` - The thread ID whose checkpoint should be deleted
///
/// # Returns
/// `true` if a checkpoint was deleted, `false` if no checkpoint existed
#[command(rename_all = "camelCase")]
pub fn codex_app_server_clear_stale_state(
    app_handle: AppHandle,
    state: State<'_, CodexAppServerState>,
    thread_id: String,
) -> Result<bool, String> {
    let request_thread_id = thread_id.trim().to_string();
    if request_thread_id.is_empty() {
        return Err("threadId is required".to_string());
    }

    // Verify handshake is complete before allowing recovery operations
    {
        let runtime = state.inner.lock().map_err(|e| e.to_string())?;
        if runtime.handshake_state != HandshakeState::Initialized {
            return Err(
                "Handshake not complete; cannot clear stale state before initialization"
                    .to_string(),
            );
        }
    }

    let pool = app_handle
        .try_state::<crate::DbState>()
        .map(|s| s.0.clone())
        .ok_or("Database pool not available")?;

    // Delete the checkpoint
    let deleted = tauri::async_runtime::block_on(async {
        crate::recovery_checkpoint::delete_recovery_checkpoint(&pool, &request_thread_id).await
    })?;

    // Emit status update to refresh UI
    if deleted {
        let mut runtime = state.inner.lock().map_err(|e| e.to_string())?;
        emit_status(&app_handle, &runtime.status);
    }

    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_account_updated, apply_pending_rpc_error, apply_pending_rpc_success,
        apply_reconnect_validation_failure, apply_restart_reentry_state,
        apply_sidecar_stdout_read_error, assert_initialized_handshake,
        assert_thread_snapshot_access, auth_mode_to_login_start_type, bind_approval_request_token,
        build_sidecar_approval_request, cancel_pending_rpcs, cleanup_live_sessions_with_policy,
        command_not_exposed_error, event_identity_key, expire_pending_approvals,
        expire_pending_rpcs, format_redacted_json, handle_live_event_internal, handle_sidecar_exit,
        harden_sidecar_command, has_pending_initialize_request,
        is_allowed_sidecar_notification_method, next_rpc_id, normalize_auth_mode, now_epoch_ms,
        now_iso, parse_event_from_legacy_input, parse_live_payload, parse_rpc_response_id,
        parser_validation_error, redact_sensitive_stderr_line, register_start_failure,
        remember_approval_decision, remember_dedupe_source, remember_sidecar_stderr_line,
        schema_version_policy, send_sidecar_request, source_priority, terminate_child_with_timeout,
        validate_and_redact_auth_url, validate_approval_submission_context,
        validate_reconnect_payload, validate_sidecar_jsonl_frame,
        validate_sidecar_notification_payload, validate_sidecar_rpc_result, version_at_least,
        wait_for_initialize_ack, wait_for_thread_read_response, AuthState, CodexAppServerRuntime,
        CodexAppServerStatus, CodexStreamEventInput, HandshakeState, LiveSessionEventPayload,
        PendingApproval, PendingRpcKind, PendingRpcRequest, SchemaVersionPolicy,
        ALLOWED_SIDECAR_NOTIFICATION_METHODS, APPROVAL_TOKEN_BYTES, APP_SERVER_PROTOCOL_TARGET,
        BLOCKED_SIDECAR_ENV_OVERRIDES, MAX_DEDUPE_SOURCES, MAX_PENDING_APPROVAL_WAITERS,
        MAX_PENDING_RPC_REQUESTS, MAX_SIDECAR_JSONL_BYTES, MAX_SIDECAR_STDERR_LINES,
        RECONNECT_REASON_SCHEMA_MISMATCH, RECONNECT_REASON_SESSION_INVALID,
        RECONNECT_REASON_TOKEN_INVALID, REQUIRED_APP_SERVER_METHODS, RESTART_BUDGET,
    };
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn identity_key_is_stable() {
        let input = CodexStreamEventInput {
            provider: "codex".to_string(),
            thread_id: "th_1".to_string(),
            turn_id: "tu_1".to_string(),
            item_id: "it_1".to_string(),
            event_type: "item/completed".to_string(),
            source: "otel".to_string(),
            payload: None,
        };

        assert_eq!(
            event_identity_key(&input),
            "v1|codex|th_1|tu_1|it_1|item/completed"
        );
    }

    #[test]
    fn precedence_prefers_stream_for_delta_and_otel_for_completed() {
        assert!(
            source_priority("item/agentMessage/delta", "app_server_stream")
                > source_priority("item/agentMessage/delta", "otel")
        );
        assert!(
            source_priority("item/completed", "otel")
                > source_priority("item/completed", "app_server_stream")
        );
    }

    #[test]
    fn restart_budget_enters_crash_loop_at_threshold() {
        let mut runtime = CodexAppServerRuntime::default();
        for i in 0..RESTART_BUDGET {
            register_start_failure(&mut runtime, format!("failure-{i}"), "test");
        }
        assert_eq!(runtime.status.state, "crash_loop");
        assert_eq!(runtime.status.restart_attempts_in_window, RESTART_BUDGET);
    }

    #[test]
    fn start_failure_sets_degraded_state_before_crash_loop_threshold() {
        let mut runtime = CodexAppServerRuntime::default();
        register_start_failure(&mut runtime, "spawn failed".to_string(), "spawn_failed");

        assert_eq!(runtime.status.state, "degraded");
        assert_eq!(runtime.process_state, super::ProcessState::Degraded);
        assert_eq!(runtime.restart_total.get("spawn_failed").copied(), Some(1));
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("spawn failed"));
    }

    #[test]
    fn sidecar_exit_handler_requests_restart_when_budget_available() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            1.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::Initialize,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 10_000,
            },
        );

        let status = Command::new("sh")
            .arg("-c")
            .arg("exit 7")
            .status()
            .expect("status");
        let should_restart = handle_sidecar_exit(&mut runtime, status);

        assert!(should_restart);
        assert!(runtime.pending_rpcs.is_empty());
        assert_eq!(runtime.next_rpc_id, 1);
        assert_eq!(runtime.status.state, "degraded");
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("cancelled 1 pending RPC"));
    }

    #[test]
    fn sidecar_exit_handler_blocks_restart_on_crash_loop_or_kill_switch() {
        let status = Command::new("sh")
            .arg("-c")
            .arg("exit 8")
            .status()
            .expect("status");

        let mut kill_switch_runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        kill_switch_runtime.status.stream_kill_switch = true;
        let should_restart = handle_sidecar_exit(&mut kill_switch_runtime, status);
        assert!(!should_restart);

        let mut crash_loop_runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::CrashLoop,
            ..CodexAppServerRuntime::default()
        };
        let should_restart = handle_sidecar_exit(&mut crash_loop_runtime, status);
        assert!(!should_restart);
    }

    #[test]
    fn restart_reentry_state_resets_handshake_auth_and_pending_buffers() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Degraded,
            handshake_state: HandshakeState::Initialized,
            auth_state: AuthState::Authenticated,
            stream_session_state: super::StreamSessionState::Alive,
            ..CodexAppServerRuntime::default()
        };
        runtime.status.stream_healthy = true;
        runtime.approval_waiters.insert(
            "req_1".to_string(),
            PendingApproval {
                thread_id: "th_1".to_string(),
                created_at_epoch_ms: 1,
                timeout_ms: 30_000,
                decision_token: "token_1".to_string(),
                approval_window_id: runtime.approval_window_id,
                rpc_request_id: None,
            },
        );
        runtime
            .thread_read_results
            .insert("100".to_string(), serde_json::json!({ "threadId": "th_1" }));
        runtime
            .thread_read_errors
            .insert("101".to_string(), "thread/read failed".to_string());

        apply_restart_reentry_state(&mut runtime);

        assert_eq!(runtime.process_state, super::ProcessState::Running);
        assert_eq!(runtime.handshake_state, HandshakeState::NotStarted);
        assert_eq!(runtime.auth_state, AuthState::NeedsLogin);
        assert_eq!(
            runtime.stream_session_state,
            super::StreamSessionState::Expected
        );
        assert!(!runtime.status.stream_healthy);
        assert!(runtime.approval_waiters.is_empty());
        assert!(runtime.thread_read_results.is_empty());
        assert!(runtime.thread_read_errors.is_empty());
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("re-entry sequence"));
    }

    #[test]
    fn chatgpt_account_updated_sets_authenticated_when_verified() {
        let mut runtime = CodexAppServerRuntime {
            auth_state: AuthState::NeedsLogin,
            handshake_state: HandshakeState::Initialized,
            ..CodexAppServerRuntime::default()
        };
        runtime.status = CodexAppServerStatus {
            state: "running".to_string(),
            stream_healthy: true,
            ..CodexAppServerStatus::default()
        };

        apply_account_updated(&mut runtime, "chatgpt", true);

        assert_eq!(runtime.status.auth_state, "authenticated");
        assert!(runtime.status.last_error.is_none());
    }

    #[test]
    fn account_updated_supports_documented_modes_and_rejects_unknown_mode() {
        let mut runtime = CodexAppServerRuntime::default();
        apply_account_updated(&mut runtime, "apikey", true);
        assert_eq!(runtime.status.auth_state, "authenticated");
        assert_eq!(runtime.status.auth_mode, "apikey");

        apply_account_updated(&mut runtime, "chatgptAuthTokens", true);
        assert_eq!(runtime.status.auth_state, "authenticated");
        assert_eq!(runtime.status.auth_mode, "chatgptAuthTokens");

        apply_account_updated(&mut runtime, "custom_mode", true);
        assert_eq!(runtime.status.auth_state, "needs_login");
        assert_eq!(runtime.status.state, "degraded");
        assert_eq!(runtime.status.auth_mode, "custom_mode");
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("Unsupported auth mode"));
    }

    #[test]
    fn auth_mode_normalization_and_login_type_mapping_follow_contract() {
        assert_eq!(normalize_auth_mode("api_key"), "apikey");
        assert_eq!(
            normalize_auth_mode("chatgptauthtokens"),
            "chatgptAuthTokens"
        );
        assert_eq!(normalize_auth_mode("none"), "none");

        assert_eq!(auth_mode_to_login_start_type("apikey"), Some("apiKey"));
        assert_eq!(
            auth_mode_to_login_start_type("chatgptAuthTokens"),
            Some("chatgptAuthTokens")
        );
        assert_eq!(auth_mode_to_login_start_type("unsupported"), None);
    }

    #[test]
    fn sidecar_version_floor_comparison_handles_prerelease_suffixes() {
        assert!(version_at_least("0.97.0", "0.97.0"));
        assert!(version_at_least("0.97.1-alpha.1", "0.97.0"));
        assert!(!version_at_least("0.96.9", "0.97.0"));
        assert!(!version_at_least("invalid", "0.97.0"));
    }

    #[test]
    fn redaction_helpers_scrub_sensitive_fields_and_lines() {
        let redacted_json = format_redacted_json(&serde_json::json!({
            "accessToken": "secret-token",
            "nested": { "refresh_token": "refresh-secret", "ok": "value" }
        }));
        assert!(redacted_json.contains("[REDACTED]"));
        assert!(!redacted_json.contains("secret-token"));
        assert!(!redacted_json.contains("refresh-secret"));

        let redacted_line =
            redact_sensitive_stderr_line("authorization: Bearer secret-token-value");
        assert!(redacted_line.contains("REDACTED"));
        assert!(!redacted_line.contains("secret-token-value"));
    }

    #[test]
    fn dedupe_source_cache_is_bounded() {
        let mut runtime = CodexAppServerRuntime::default();
        for idx in 0..(MAX_DEDUPE_SOURCES + 32) {
            let key = format!("k-{idx}");
            remember_dedupe_source(&mut runtime, &key, "otel");
        }

        assert_eq!(runtime.dedupe_sources.len(), MAX_DEDUPE_SOURCES);
        assert_eq!(runtime.dedupe_key_order.len(), MAX_DEDUPE_SOURCES);
        assert!(!runtime.dedupe_sources.contains_key("k-0"));
        assert!(runtime
            .dedupe_sources
            .contains_key(&format!("k-{}", MAX_DEDUPE_SOURCES + 31)));
    }

    #[test]
    fn sidecar_stderr_ring_is_bounded_and_tracks_drops() {
        let mut runtime = CodexAppServerRuntime::default();
        for idx in 0..(MAX_SIDECAR_STDERR_LINES + 7) {
            remember_sidecar_stderr_line(&mut runtime, &format!("stderr-{idx}"));
        }

        assert_eq!(runtime.sidecar_stderr_ring.len(), MAX_SIDECAR_STDERR_LINES);
        assert_eq!(runtime.sidecar_stderr_dropped, 7);
        assert_eq!(
            runtime.sidecar_stderr_ring.front().map(String::as_str),
            Some("stderr-7")
        );
    }

    #[test]
    fn harden_sidecar_command_sets_trusted_cwd_and_strips_loader_overrides() {
        let mut command = Command::new("sh");
        let binary_path = Path::new("/tmp/codex-app-server");

        harden_sidecar_command(&mut command, binary_path);

        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp")));
        for env_key in BLOCKED_SIDECAR_ENV_OVERRIDES {
            let removed = command
                .get_envs()
                .find_map(|(key, value)| (key == std::ffi::OsStr::new(env_key)).then_some(value));
            assert_eq!(removed, Some(None));
        }
    }

    #[test]
    fn parser_payload_fallback_supports_legacy_shape() {
        let raw = serde_json::json!({
            "provider": "codex",
            "threadId": "th_1",
            "turnId": "tu_1",
            "itemId": "it_1",
            "eventType": "item/completed",
            "source": "otel",
            "payload": {"done": true}
        });

        let parsed = parse_live_payload(raw).expect("legacy payload should parse");
        assert!(matches!(
            parsed,
            LiveSessionEventPayload::SessionDelta { .. }
        ));
    }

    #[test]
    fn parser_validation_error_is_produced_for_invalid_payload() {
        let raw = serde_json::json!({ "unexpected": "shape" });
        let err = parse_live_payload(raw).expect_err("invalid payload should return parser error");
        assert!(matches!(
            err,
            LiveSessionEventPayload::ParserValidationError {
                kind,
                reason,
                ..
            } if kind == "missing_fields" && !reason.is_empty()
        ));
    }

    #[test]
    fn oversized_sidecar_frames_are_rejected() {
        let oversized = "x".repeat(MAX_SIDECAR_JSONL_BYTES + 1);
        let result = validate_sidecar_jsonl_frame(&oversized);
        assert!(matches!(
            result,
            Err(LiveSessionEventPayload::ParserValidationError {
                kind,
                reason,
                ..
            }) if kind == "protocol_violation" && reason.contains("exceeded max size")
        ));
    }

    #[test]
    fn stdout_read_errors_degrade_runtime_without_panicking() {
        let mut runtime = CodexAppServerRuntime::default();
        let event =
            apply_sidecar_stdout_read_error(&mut runtime, "stream did not contain valid UTF-8");
        assert!(matches!(
            event,
            LiveSessionEventPayload::ParserValidationError { kind, .. } if kind == "protocol_violation"
        ));
        assert_eq!(runtime.process_state, super::ProcessState::Degraded);
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("invalid UTF-8"));
    }

    #[test]
    fn internal_approval_round_trip_tracks_pending_requests() {
        let mut runtime = CodexAppServerRuntime::default();
        let request = LiveSessionEventPayload::ApprovalRequest {
            request_id: "req_1".to_string(),
            thread_id: "th_1".to_string(),
            turn_id: "tu_1".to_string(),
            command: "rm -rf /".to_string(),
            options: vec!["approve".to_string(), "deny".to_string()],
            timeout_ms: 5_000,
            rpc_request_id: None,
            decision_token: Some("token_req_1".to_string()),
        };

        handle_live_event_internal(&mut runtime, &request);
        assert_eq!(runtime.approval_waiters.len(), 1);

        let result = LiveSessionEventPayload::ApprovalResult {
            request_id: "req_1".to_string(),
            thread_id: "th_1".to_string(),
            approved: false,
            decided_at_iso: "2026-02-24T00:00:00.000Z".to_string(),
            decided_by: Some("user".to_string()),
            reason: Some("Denied".to_string()),
        };
        handle_live_event_internal(&mut runtime, &result);

        assert_eq!(
            runtime.approval_result_total.get("rejected").copied(),
            Some(1)
        );
        assert!(runtime.approval_decisions.contains_key("req_1"));
    }

    #[test]
    fn approval_timeout_produces_timeout_result() {
        let mut runtime = CodexAppServerRuntime::default();
        let request = LiveSessionEventPayload::ApprovalRequest {
            request_id: "req_timeout".to_string(),
            thread_id: "thread_timeout".to_string(),
            turn_id: "turn_timeout".to_string(),
            command: "write_file".to_string(),
            options: vec!["allow".to_string(), "deny".to_string()],
            timeout_ms: 1,
            rpc_request_id: None,
            decision_token: Some("token_timeout".to_string()),
        };
        handle_live_event_internal(&mut runtime, &request);

        let expired = expire_pending_approvals(&mut runtime, i64::MAX);
        assert_eq!(expired.len(), 1);
        assert!(matches!(
            &expired[0],
            LiveSessionEventPayload::ApprovalResult {
                approved: false,
                reason: Some(reason),
                ..
            } if reason.contains("Timed out")
        ));
        assert_eq!(runtime.approval_waiters.len(), 0);
        assert_eq!(
            runtime.approval_result_total.get("timeout").copied(),
            Some(1)
        );
        assert!(runtime.approval_decisions.contains_key("req_timeout"));
    }

    #[test]
    fn approval_waiters_are_bounded_with_overflow_telemetry() {
        let mut runtime = CodexAppServerRuntime::default();
        runtime.approval_waiters.insert(
            "oldest".to_string(),
            PendingApproval {
                thread_id: "th_old".to_string(),
                created_at_epoch_ms: 1,
                timeout_ms: 30_000,
                decision_token: "tok_old".to_string(),
                approval_window_id: runtime.approval_window_id,
                rpc_request_id: None,
            },
        );
        for idx in 0..(MAX_PENDING_APPROVAL_WAITERS - 1) {
            runtime.approval_waiters.insert(
                format!("req_{idx}"),
                PendingApproval {
                    thread_id: "th".to_string(),
                    created_at_epoch_ms: (idx + 2) as i64,
                    timeout_ms: 30_000,
                    decision_token: format!("tok_{idx}"),
                    approval_window_id: runtime.approval_window_id,
                    rpc_request_id: None,
                },
            );
        }
        let request = LiveSessionEventPayload::ApprovalRequest {
            request_id: "incoming".to_string(),
            thread_id: "th_new".to_string(),
            turn_id: "tu_new".to_string(),
            command: "cmd".to_string(),
            options: vec!["approve".to_string()],
            timeout_ms: 30_000,
            rpc_request_id: Some(serde_json::json!(99)),
            decision_token: Some("tok_new".to_string()),
        };
        handle_live_event_internal(&mut runtime, &request);
        assert_eq!(runtime.approval_waiters.len(), MAX_PENDING_APPROVAL_WAITERS);
        assert!(!runtime.approval_waiters.contains_key("oldest"));
        assert!(runtime.approval_waiters.contains_key("incoming"));
        assert_eq!(
            runtime
                .parse_error_total
                .get("approval_waiter_overflow")
                .copied(),
            Some(1)
        );
    }

    #[test]
    fn thread_snapshot_access_requires_initialized_and_authenticated() {
        let runtime = CodexAppServerRuntime::default();
        let err = assert_thread_snapshot_access(&runtime).expect_err("handshake gate should fail");
        assert!(err.contains("Handshake incomplete"));

        let runtime = CodexAppServerRuntime {
            handshake_state: HandshakeState::Initialized,
            auth_state: AuthState::NeedsLogin,
            ..CodexAppServerRuntime::default()
        };
        let err = assert_thread_snapshot_access(&runtime).expect_err("auth gate should fail");
        assert!(err.contains("Authentication required"));

        let runtime = CodexAppServerRuntime {
            handshake_state: HandshakeState::Initialized,
            auth_state: AuthState::Authenticated,
            ..CodexAppServerRuntime::default()
        };
        assert!(assert_thread_snapshot_access(&runtime).is_ok());
    }

    #[test]
    fn reconnect_validation_rejects_invalid_schema_session_and_token() {
        let make_payload = |payload: serde_json::Value| LiveSessionEventPayload::SessionDelta {
            thread_id: "th".to_string(),
            turn_id: "tu".to_string(),
            item_id: "it".to_string(),
            event_type: "item/delta".to_string(),
            source: "app_server_stream".to_string(),
            sequence_id: 1,
            received_at_iso: "2026-02-24T00:00:00.000Z".to_string(),
            payload,
        };

        let invalid_schema = make_payload(serde_json::json!({ "schemaVersion": "v2" }));
        assert_eq!(
            validate_reconnect_payload(&invalid_schema).unwrap_err(),
            RECONNECT_REASON_SCHEMA_MISMATCH
        );

        let invalid_session = make_payload(serde_json::json!({ "sessionValid": false }));
        assert_eq!(
            validate_reconnect_payload(&invalid_session).unwrap_err(),
            RECONNECT_REASON_SESSION_INVALID
        );

        let invalid_token = make_payload(serde_json::json!({ "tokenExpired": true }));
        assert_eq!(
            validate_reconnect_payload(&invalid_token).unwrap_err(),
            RECONNECT_REASON_TOKEN_INVALID
        );
    }

    #[test]
    fn schema_version_policy_classifies_supported_and_rejected_versions() {
        assert_eq!(schema_version_policy("v1"), SchemaVersionPolicy::Supported);
        assert_eq!(schema_version_policy("v2"), SchemaVersionPolicy::Rejected);
    }

    #[test]
    fn protocol_target_constant_is_defined() {
        assert_eq!(APP_SERVER_PROTOCOL_TARGET, "v2");
    }

    #[test]
    fn next_rpc_id_starts_at_one_and_increments() {
        let mut runtime = CodexAppServerRuntime::default();
        assert_eq!(next_rpc_id(&mut runtime), 1);
        assert_eq!(next_rpc_id(&mut runtime), 2);
        assert_eq!(next_rpc_id(&mut runtime), 3);
    }

    #[test]
    fn initialize_tracking_helpers_detect_pending_and_wait_for_ack() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            1.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::Initialize,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 10_000,
            },
        );
        assert!(has_pending_initialize_request(&runtime));

        let state = std::sync::Arc::new(std::sync::Mutex::new(runtime));
        let state_clone = state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(mut runtime) = state_clone.lock() {
                runtime.handshake_state = HandshakeState::InitializeSent;
            }
        });

        let wait_result = wait_for_initialize_ack(&state, Duration::from_millis(500));
        assert!(wait_result.is_ok());
    }

    #[test]
    fn thread_read_waiter_returns_result_and_error() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            11.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 10_000,
            },
        );
        runtime
            .thread_read_results
            .insert("11".to_string(), serde_json::json!({ "threadId": "th_1" }));
        let state = std::sync::Arc::new(std::sync::Mutex::new(runtime));
        let value = wait_for_thread_read_response(&state, 11, Duration::from_millis(100))
            .expect("thread/read result should be returned");
        assert_eq!(
            value.get("threadId").and_then(serde_json::Value::as_str),
            Some("th_1")
        );

        let mut runtime_err = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime_err.pending_rpcs.insert(
            12.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 10_000,
            },
        );
        runtime_err
            .thread_read_errors
            .insert("12".to_string(), "thread/read request failed".to_string());
        let state_err = std::sync::Arc::new(std::sync::Mutex::new(runtime_err));
        let err = wait_for_thread_read_response(&state_err, 12, Duration::from_millis(100))
            .expect_err("thread/read error should be surfaced");
        assert!(err.contains("thread/read request failed"));
    }

    #[test]
    fn thread_read_timeout_records_thread_read_error_and_degraded_runtime() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            77.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: 0,
                timeout_ms: 1,
            },
        );

        let expired = expire_pending_rpcs(&mut runtime, i64::MAX);
        assert_eq!(expired, 1);
        assert_eq!(
            runtime.thread_read_errors.get("77").map(String::as_str),
            Some("thread/read request timed out")
        );
        assert_eq!(runtime.status.state, "degraded");
        assert_eq!(
            runtime.parse_error_total.get("rpc_timeout").copied(),
            Some(1)
        );

        let state = std::sync::Arc::new(std::sync::Mutex::new(runtime));
        let err = wait_for_thread_read_response(&state, 77, Duration::from_millis(5))
            .expect_err("timed out thread/read should stay non-authoritative");
        assert!(err.contains("timed out"));
    }

    #[test]
    fn cancelled_thread_read_waiter_returns_cancelled_error_after_thread_switch() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            88.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
        );

        let cancelled = cancel_pending_rpcs(&mut runtime, "rpc_cancelled_thread_switch");
        assert_eq!(cancelled, 1);
        assert!(runtime.pending_rpcs.is_empty());
        assert_eq!(
            runtime.thread_read_errors.get("88").map(String::as_str),
            Some("thread/read request cancelled")
        );

        let state = std::sync::Arc::new(std::sync::Mutex::new(runtime));
        let err = wait_for_thread_read_response(&state, 88, Duration::from_millis(5))
            .expect_err("stale thread/read worker should be cancelled");
        assert!(err.contains("cancelled"));
    }

    #[test]
    fn fresh_retry_request_isolated_from_stale_thread_read_timeout() {
        let mut runtime = CodexAppServerRuntime {
            process_state: super::ProcessState::Running,
            ..CodexAppServerRuntime::default()
        };
        runtime.pending_rpcs.insert(
            91.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: 0,
                timeout_ms: 1,
            },
        );
        let expired = expire_pending_rpcs(&mut runtime, i64::MAX);
        assert_eq!(expired, 1);
        assert!(runtime.thread_read_errors.contains_key("91"));

        runtime.pending_rpcs.insert(
            92.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 10_000,
            },
        );
        runtime.thread_read_results.insert(
            92.to_string(),
            serde_json::json!({ "threadId": "th_retry" }),
        );

        let state = std::sync::Arc::new(std::sync::Mutex::new(runtime));
        let value = wait_for_thread_read_response(&state, 92, Duration::from_millis(100))
            .expect("fresh retry should not inherit stale timeout state");
        assert_eq!(
            value.get("threadId").and_then(serde_json::Value::as_str),
            Some("th_retry")
        );
    }

    #[test]
    fn initialized_handshake_guard_requires_initialized_state() {
        let runtime = CodexAppServerRuntime::default();
        let err = assert_initialized_handshake(&runtime).expect_err("handshake should be required");
        assert!(err.contains("Not initialized"));

        let runtime = CodexAppServerRuntime {
            handshake_state: HandshakeState::Initialized,
            ..CodexAppServerRuntime::default()
        };
        assert!(assert_initialized_handshake(&runtime).is_ok());
    }

    #[test]
    fn cancel_pending_rpcs_clears_inflight_requests() {
        let mut runtime = CodexAppServerRuntime::default();
        runtime.pending_rpcs.insert(
            1.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::Initialize,
                sent_at_epoch_ms: 1,
                timeout_ms: 1,
            },
        );
        runtime.pending_rpcs.insert(
            2.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::AccountRead,
                sent_at_epoch_ms: 1,
                timeout_ms: 1,
            },
        );

        let cancelled = cancel_pending_rpcs(&mut runtime, "rpc_cancelled_test");
        assert_eq!(cancelled, 2);
        assert!(runtime.pending_rpcs.is_empty());
        assert_eq!(
            runtime.parse_error_total.get("rpc_cancelled_test").copied(),
            Some(1)
        );
    }

    #[test]
    fn expire_pending_rpcs_marks_runtime_degraded_with_request_labels() {
        let mut runtime = CodexAppServerRuntime::default();
        runtime.handshake_state = HandshakeState::InitializeSent;
        runtime.pending_rpcs.insert(
            42.to_string(),
            PendingRpcRequest {
                kind: PendingRpcKind::Initialize,
                sent_at_epoch_ms: 0,
                timeout_ms: 1,
            },
        );

        let expired = expire_pending_rpcs(&mut runtime, i64::MAX);
        assert_eq!(expired, 1);
        assert!(runtime.pending_rpcs.is_empty());
        assert_eq!(runtime.handshake_state, HandshakeState::NotStarted);
        assert_eq!(runtime.status.state, "degraded");
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("42:initialize"));
    }

    #[test]
    fn pending_rpc_handlers_update_runtime_by_request_kind() {
        let mut runtime = CodexAppServerRuntime::default();

        apply_pending_rpc_success(
            &mut runtime,
            "1",
            PendingRpcRequest {
                kind: PendingRpcKind::Initialize,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
            &serde_json::json!({}),
        );
        assert_eq!(runtime.handshake_state, HandshakeState::InitializeSent);

        apply_pending_rpc_success(
            &mut runtime,
            "2",
            PendingRpcRequest {
                kind: PendingRpcKind::AccountRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
            &serde_json::json!({ "account": { "type": "chatgpt" } }),
        );
        assert_eq!(runtime.auth_state, AuthState::Authenticated);

        runtime.auth_state = AuthState::Authenticating;
        runtime.process_state = super::ProcessState::Running;
        apply_pending_rpc_error(
            &mut runtime,
            "99",
            PendingRpcRequest {
                kind: PendingRpcKind::AccountLoginStart,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
            &serde_json::json!({
                "code": -32000,
                "message": "boom",
                "accessToken": "super-secret-token"
            }),
        );
        assert_eq!(runtime.auth_state, AuthState::NeedsLogin);
        assert_eq!(runtime.process_state, super::ProcessState::Degraded);
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("account/login/start"));
        assert!(!runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("super-secret-token"));

        apply_pending_rpc_success(
            &mut runtime,
            "3",
            PendingRpcRequest {
                kind: PendingRpcKind::AccountChatgptTokensRefresh,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
            &serde_json::json!({}),
        );
        assert_eq!(runtime.auth_state, AuthState::Authenticated);
        assert_eq!(runtime.status.auth_mode, "chatgptAuthTokens");

        apply_pending_rpc_success(
            &mut runtime,
            "4",
            PendingRpcRequest {
                kind: PendingRpcKind::ThreadRead,
                sent_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 1_000,
            },
            &serde_json::json!({ "threadId": "thread_1", "items": [] }),
        );
        assert!(runtime.thread_read_results.contains_key("4"));
    }

    #[test]
    fn sidecar_notification_allowlist_rejects_unknown_methods() {
        assert!(is_allowed_sidecar_notification_method("account/updated"));
        assert!(is_allowed_sidecar_notification_method("turn/completed"));
        assert!(!is_allowed_sidecar_notification_method("thread/read"));
        assert_eq!(
            ALLOWED_SIDECAR_NOTIFICATION_METHODS.len(),
            9,
            "notification allowlist changed; update schema drift contract"
        );
        assert!(
            REQUIRED_APP_SERVER_METHODS.contains(&"thread/read")
                && REQUIRED_APP_SERVER_METHODS.contains(&"approval/submit"),
            "required app-server method contract changed unexpectedly"
        );
    }

    #[test]
    fn notification_schema_validation_rejects_unknown_fields_and_missing_required_fields() {
        let unknown_field_message = serde_json::json!({
            "method": "account/updated",
            "params": { "authMode": "chatgpt", "unexpected": true }
        });
        let err = validate_sidecar_notification_payload("account/updated", &unknown_field_message)
            .expect_err("unknown field should fail");
        assert!(err.contains("Unsupported field"));

        let missing_required_message = serde_json::json!({
            "method": "account/login/completed",
            "params": { "error": "cancelled" }
        });
        let err = validate_sidecar_notification_payload(
            "account/login/completed",
            &missing_required_message,
        )
        .expect_err("missing success should fail");
        assert!(err.contains("success must be boolean"));

        let empty_request_id_message = serde_json::json!({
            "method": "item/commandExecution/requestApproval",
            "params": {
                "requestId": "",
                "threadId": "th_1",
                "turnId": "tu_1",
                "command": "echo hi"
            }
        });
        let err = validate_sidecar_notification_payload(
            "item/commandExecution/requestApproval",
            &empty_request_id_message,
        )
        .expect_err("empty request id should fail without envelope id");
        assert!(err.contains("approval request must include requestId/request_id or envelope id"));
    }

    #[test]
    fn rpc_result_schema_validation_rejects_invalid_shapes() {
        let init_err =
            validate_sidecar_rpc_result(PendingRpcKind::Initialize, &serde_json::json!(42))
                .expect_err("initialize result must be object");
        assert!(init_err.contains("initialize result must be an object"));

        let login_err = validate_sidecar_rpc_result(
            PendingRpcKind::AccountLoginStart,
            &serde_json::json!({ "authUrl": 123 }),
        )
        .expect_err("authUrl should be string");
        assert!(login_err.contains("authUrl must be a string"));

        let refresh_err = validate_sidecar_rpc_result(
            PendingRpcKind::AccountChatgptTokensRefresh,
            &serde_json::json!(null),
        )
        .expect_err("refresh result should be object");
        assert!(refresh_err.contains("refresh result must be an object"));

        let auth_url_err = validate_sidecar_rpc_result(
            PendingRpcKind::AccountLoginStart,
            &serde_json::json!({ "authUrl": "http://evil.example/login" }),
        )
        .expect_err("auth url should be rejected");
        assert!(auth_url_err.contains("authUrl invalid"));

        let thread_read_err =
            validate_sidecar_rpc_result(PendingRpcKind::ThreadRead, &serde_json::json!(null))
                .expect_err("thread/read result should be object");
        assert!(thread_read_err.contains("thread/read result must be an object"));
    }

    #[test]
    fn auth_url_validation_enforces_https_allowlist_and_redaction() {
        let sanitized =
            validate_and_redact_auth_url("https://chat.openai.com/auth?token=secret123#frag")
                .expect("allowlisted url should pass");
        assert_eq!(sanitized, "https://chat.openai.com/auth");

        let invalid_scheme =
            validate_and_redact_auth_url("http://chat.openai.com/auth").expect_err("http rejected");
        assert!(invalid_scheme.contains("https"));

        let invalid_host = validate_and_redact_auth_url("https://evil.example/login")
            .expect_err("host should be rejected");
        assert!(invalid_host.contains("allowlisted"));
    }

    #[test]
    fn bind_approval_request_token_generates_missing_token() {
        let payload = LiveSessionEventPayload::ApprovalRequest {
            request_id: "req_bind".to_string(),
            thread_id: "th_bind".to_string(),
            turn_id: "tu_bind".to_string(),
            command: "write_file".to_string(),
            options: vec!["approve".to_string(), "deny".to_string()],
            timeout_ms: 10_000,
            rpc_request_id: None,
            decision_token: None,
        };

        let bound = bind_approval_request_token(payload);
        let LiveSessionEventPayload::ApprovalRequest { decision_token, .. } = bound else {
            panic!("expected approval request payload");
        };
        assert!(decision_token.as_deref().unwrap_or_default().len() >= APPROVAL_TOKEN_BYTES);
    }

    #[test]
    fn build_sidecar_approval_request_uses_envelope_id_when_request_id_missing() {
        let message = serde_json::json!({
            "method": "item/commandExecution/requestApproval",
            "params": {
                "threadId": "th_1",
                "turnId": "tu_1",
                "command": "echo hi"
            }
        });
        let payload = build_sidecar_approval_request(
            "item/commandExecution/requestApproval",
            &message,
            Some(serde_json::json!(42)),
        );
        let LiveSessionEventPayload::ApprovalRequest {
            request_id,
            rpc_request_id,
            ..
        } = payload
        else {
            panic!("expected approval request");
        };
        assert_eq!(request_id, "42");
        assert_eq!(rpc_request_id, Some(serde_json::json!(42)));
    }

    #[test]
    fn build_sidecar_approval_request_falls_back_to_envelope_id_when_request_id_is_empty() {
        let message = serde_json::json!({
            "method": "item/fileChange/requestApproval",
            "id": 42,
            "params": {
                "requestId": "",
                "threadId": "th_1",
                "turnId": "tu_1",
                "command": "echo hi"
            }
        });
        let payload = build_sidecar_approval_request(
            "item/fileChange/requestApproval",
            &message,
            Some(serde_json::json!(42)),
        );
        let LiveSessionEventPayload::ApprovalRequest {
            request_id,
            rpc_request_id,
            ..
        } = payload
        else {
            panic!("expected approval request");
        };
        assert_eq!(request_id, "42");
        assert_eq!(rpc_request_id, Some(serde_json::json!(42)));
    }

    #[test]
    fn parse_rpc_response_id_supports_integer_string_and_null_ids() {
        assert_eq!(
            parse_rpc_response_id(&serde_json::json!(7)),
            Some("7".to_string())
        );
        assert_eq!(
            parse_rpc_response_id(&serde_json::json!("8")),
            Some("8".to_string())
        );
        assert_eq!(
            parse_rpc_response_id(&serde_json::json!("not-a-number")),
            Some("not-a-number".to_string())
        );
        assert_eq!(
            parse_rpc_response_id(&serde_json::json!(null)),
            Some("null".to_string())
        );
    }

    #[test]
    fn approval_submission_context_rejects_replay_and_context_mismatch() {
        let mut runtime = CodexAppServerRuntime::default();
        runtime.approval_window_id = 777;
        runtime.approval_waiters.insert(
            "req_ctx".to_string(),
            PendingApproval {
                thread_id: "thread_ctx".to_string(),
                created_at_epoch_ms: now_epoch_ms(),
                timeout_ms: 5_000,
                decision_token: "token_ctx".to_string(),
                approval_window_id: 777,
                rpc_request_id: None,
            },
        );

        let mismatch = validate_approval_submission_context(
            &mut runtime,
            "req_ctx",
            "wrong_thread",
            "token_ctx",
        )
        .expect_err("thread mismatch should fail");
        assert!(mismatch.contains("thread context mismatch"));
        assert_eq!(
            runtime
                .approval_result_total
                .get("context_mismatch")
                .copied(),
            Some(1)
        );

        let invalid_token = validate_approval_submission_context(
            &mut runtime,
            "req_ctx",
            "thread_ctx",
            "wrong_token",
        )
        .expect_err("invalid token should fail");
        assert!(invalid_token.contains("invalid decision token"));
        assert_eq!(
            runtime.approval_result_total.get("invalid_token").copied(),
            Some(1)
        );

        let valid = validate_approval_submission_context(
            &mut runtime,
            "req_ctx",
            "thread_ctx",
            "token_ctx",
        )
        .expect("valid context should pass");
        assert_eq!(valid.thread_id, "thread_ctx");

        let replay_payload = LiveSessionEventPayload::ApprovalResult {
            request_id: "req_replay".to_string(),
            thread_id: "thread_replay".to_string(),
            approved: false,
            decided_at_iso: now_iso(),
            decided_by: Some("user".to_string()),
            reason: Some("Denied".to_string()),
        };
        remember_approval_decision(
            &mut runtime,
            &replay_payload,
            Some("token_replay"),
            Some(777),
        );

        let replay = validate_approval_submission_context(
            &mut runtime,
            "req_replay",
            "thread_replay",
            "token_replay",
        )
        .expect_err("replay should be rejected");
        assert!(replay.contains("replay rejected"));
        assert_eq!(
            runtime
                .approval_result_total
                .get("replay_rejected")
                .copied(),
            Some(1)
        );
    }

    #[test]
    fn send_sidecar_request_enforces_pending_queue_limit() {
        let mut runtime = CodexAppServerRuntime::default();
        for id in 1..=(MAX_PENDING_RPC_REQUESTS as i64) {
            runtime.pending_rpcs.insert(
                id.to_string(),
                PendingRpcRequest {
                    kind: PendingRpcKind::AccountRead,
                    sent_at_epoch_ms: now_epoch_ms(),
                    timeout_ms: 10_000,
                },
            );
        }

        let result = send_sidecar_request(
            &mut runtime,
            "account/read",
            serde_json::json!({ "refreshToken": false }),
            PendingRpcKind::AccountRead,
        );
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap_or_default()
            .contains("Too many pending sidecar RPC requests"));
        assert_eq!(runtime.process_state, super::ProcessState::Degraded);
        assert_eq!(
            runtime.parse_error_total.get("rpc_overload").copied(),
            Some(1)
        );
    }

    #[test]
    fn reconnect_validation_failure_updates_runtime_with_normalized_reason() {
        let mut runtime = CodexAppServerRuntime::default();
        apply_reconnect_validation_failure(&mut runtime, RECONNECT_REASON_TOKEN_INVALID);
        assert_eq!(runtime.status.state, "degraded");
        assert_eq!(
            runtime.stream_session_state,
            super::StreamSessionState::Failed
        );
        assert!(runtime
            .status
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains(RECONNECT_REASON_TOKEN_INVALID));
        assert_eq!(
            runtime
                .restart_total
                .get(RECONNECT_REASON_TOKEN_INVALID)
                .copied(),
            Some(1)
        );
    }

    #[test]
    #[cfg(unix)]
    fn terminate_child_with_timeout_kills_process_when_needed() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .expect("spawn child");

        terminate_child_with_timeout(&mut child, Duration::from_millis(10))
            .expect("terminate child");
        let status = child.try_wait().expect("wait child status");
        assert!(status.is_some());
    }

    #[test]
    fn live_sessions_cleanup_applies_ttl_and_row_cap() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("memory sqlite");

            sqlx::query(include_str!("../migrations/015_live_sessions.sql"))
                .execute(&pool)
                .await
                .expect("migration applies");

            for idx in 0..8 {
                sqlx::query(
                    r#"
                    INSERT INTO live_sessions (
                      thread_id, turn_id, item_id, event_type, source, status, payload, last_activity_at
                    ) VALUES (?, ?, ?, 'item/completed', 'otel', 'completed', '{}', datetime('now', ?))
                    "#,
                )
                .bind(format!("th_{idx}"))
                .bind("tu")
                .bind("it")
                .bind(format!("-{} hours", idx + 1))
                .execute(&pool)
                .await
                .expect("insert row");
            }

            let cleanup = cleanup_live_sessions_with_policy(&pool, 2, 3)
                .await
                .expect("cleanup works");

            assert!(cleanup.removed_rows >= 5);

            let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM live_sessions")
                .fetch_one(&pool)
                .await
                .expect("count rows");
            assert!(remaining <= 3);
        });
    }

    #[test]
    fn deprecated_mutation_commands_return_explicit_error() {
        let stream_health_err = command_not_exposed_error("codex_app_server_set_stream_health");
        let ingest_err = command_not_exposed_error("ingest_codex_stream_event");

        assert!(stream_health_err.contains("command-not-exposed"));
        assert!(ingest_err.contains("command-not-exposed"));
    }

    #[test]
    fn parser_error_payload_truncates_raw_preview() {
        let raw = serde_json::json!({ "payload": "x".repeat(2_048) });
        let payload = parser_validation_error("protocol_violation", &raw, "boom");
        match payload {
            LiveSessionEventPayload::ParserValidationError { raw_preview, .. } => {
                assert!(raw_preview.len() <= 512);
            }
            _ => panic!("expected parser error"),
        }
    }

    #[test]
    fn legacy_input_conversion_sets_expected_source() {
        let payload = parse_event_from_legacy_input(CodexStreamEventInput {
            provider: "codex".to_string(),
            thread_id: "th_1".to_string(),
            turn_id: "tu_1".to_string(),
            item_id: "it_1".to_string(),
            event_type: "item/delta".to_string(),
            source: "stream".to_string(),
            payload: None,
        });

        match payload {
            LiveSessionEventPayload::SessionDelta { source, .. } => {
                assert_eq!(source, "app_server_stream");
            }
            _ => panic!("expected session delta"),
        }
    }

    #[test]
    fn thread_recovery_checkpoint_status_reports_fresh_retry_reasons() {
        use super::{determine_fresh_retry_reason, CodexThreadRecoveryCheckpointStatus};
        use crate::recovery_checkpoint::{
            RecoveryCheckpoint, RECOVERY_CHECKPOINT_SCHEMA_VERSION,
        };

        // Schema version mismatch
        let bad_schema = RecoveryCheckpoint {
            thread_id: Some("thread_1".to_string()),
            last_applied_event_seq: Some(10),
            replay_cursor: Some("cursor:10".to_string()),
            inflight_effect_ids: vec![],
            checkpoint_written_at_iso: "2026-03-07T04:00:00.000Z".to_string(),
            schema_version: RECOVERY_CHECKPOINT_SCHEMA_VERSION + 99,
            trust_pause_reason: None,
        };
        let reason = determine_fresh_retry_reason(&bad_schema);
        assert!(reason.contains("schema version"));
        assert!(reason.contains("incompatible"));

        // Empty thread_id
        let no_thread = RecoveryCheckpoint {
            thread_id: Some("".to_string()),
            last_applied_event_seq: Some(10),
            replay_cursor: Some("cursor:10".to_string()),
            inflight_effect_ids: vec![],
            checkpoint_written_at_iso: "2026-03-07T04:00:00.000Z".to_string(),
            schema_version: RECOVERY_CHECKPOINT_SCHEMA_VERSION,
            trust_pause_reason: None,
        };
        let reason = determine_fresh_retry_reason(&no_thread);
        assert!(reason.contains("missing or empty thread_id"));

        // Missing replay cursor with event sequence
        let no_cursor = RecoveryCheckpoint {
            thread_id: Some("thread_2".to_string()),
            last_applied_event_seq: Some(10),
            replay_cursor: None,
            inflight_effect_ids: vec![],
            checkpoint_written_at_iso: "2026-03-07T04:00:00.000Z".to_string(),
            schema_version: RECOVERY_CHECKPOINT_SCHEMA_VERSION,
            trust_pause_reason: None,
        };
        let reason = determine_fresh_retry_reason(&no_cursor);
        assert!(reason.contains("no replay cursor"));
    }

    #[test]
    fn thread_recovery_checkpoint_status_serializes_for_frontend() {
        use super::CodexThreadRecoveryCheckpointStatus;
        use crate::recovery_checkpoint::{
            RecoveryCheckpoint, RECOVERY_CHECKPOINT_SCHEMA_VERSION,
        };

        // No checkpoint case
        let no_checkpoint = CodexThreadRecoveryCheckpointStatus {
            thread_id: "thread_new".to_string(),
            checkpoint_exists: false,
            requires_fresh_retry: false,
            trust_state_recommendation: "none".to_string(),
            checkpoint: None,
            fresh_retry_reason: None,
        };
        let json = serde_json::to_string(&no_checkpoint).expect("serialize");
        assert!(json.contains("\"threadId\":\"thread_new\""));
        assert!(json.contains("\"checkpointExists\":false"));
        assert!(json.contains("\"requiresFreshRetry\":false"));

        // Checkpoint exists, requires fresh retry
        let checkpoint = RecoveryCheckpoint {
            thread_id: Some("thread_corrupted".to_string()),
            last_applied_event_seq: Some(5),
            replay_cursor: None, // Missing cursor triggers fresh retry
            inflight_effect_ids: vec![],
            checkpoint_written_at_iso: "2026-03-07T04:00:00.000Z".to_string(),
            schema_version: RECOVERY_CHECKPOINT_SCHEMA_VERSION,
            trust_pause_reason: None,
        };
        let needs_retry = CodexThreadRecoveryCheckpointStatus {
            thread_id: "thread_corrupted".to_string(),
            checkpoint_exists: true,
            requires_fresh_retry: true,
            trust_state_recommendation: "trust_paused".to_string(),
            checkpoint: Some(checkpoint),
            fresh_retry_reason: Some("checkpoint has event sequence but no replay cursor".to_string()),
        };
        let json = serde_json::to_string(&needs_retry).expect("serialize");
        assert!(json.contains("\"trustStateRecommendation\":\"trust_paused\""));
        assert!(json.contains("\"freshRetryReason\""));
    }
}
