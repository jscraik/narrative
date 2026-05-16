export type NarrativeTelemetrySchemaVersion = "v1";

export type NarrativeTelemetryEventName =
	| "what_ready"
	| "first_win_completed"
	| "narrative_viewed"
	| "layer_switched"
	| "branch_fast_path_loaded"
	| "branch_primary_cta_used"
	| "branch_advanced_analysis_opened"
	| "branch_low_confidence_fallback_used"
	| "branch_evidence_open_time_ms"
	| "branch_story_to_diff_time_ms"
	| "branch_verification_mode_selected"
	| "branch_scope_reset_occurred"
	| "branch_advanced_control_used"
	| "audience_switched"
	| "evidence_opened"
	| "fallback_used"
	| "feedback_submitted"
	| "rollout_scored"
	| "kill_switch_triggered"
	| "ui.quality.render_decision";

// ============================================================================
// Ask-Why Telemetry Events
// ============================================================================

import type {
	AskWhyCitationType,
	AskWhyConfidenceBand,
	AskWhyFallbackReasonCode,
} from "../types";

export type AskWhyTelemetryEventName =
	| "ask_why_submitted"
	| "ask_why_answer_viewed"
	| "ask_why_evidence_opened"
	| "ask_why_fallback_used"
	| "ask_why_error";

// Re-export types for convenience
export type {
	AskWhyCitationType,
	AskWhyConfidenceBand,
	AskWhyFallbackReasonCode,
};

export type AskWhyTelemetryPayload = {
	request_key?: string;
	queryId: string;
	attemptId?: string;
	branchId?: string;
	branchScope?: string;
	questionHash?: string;
	confidence?: AskWhyConfidenceBand;
	citationCount?: number;
	citationType?: AskWhyCitationType;
	citationId?: string;
	fallbackUsed?: boolean;
	reasonCode?: AskWhyFallbackReasonCode;
	errorType?: string;
	eventOutcome?: NarrativeEventOutcome;
	funnelStep?: NarrativeFunnelStep;
	funnelSessionId?: string;
	flowLatencyMs?: number;
};

export type DashboardTelemetryEventName =
	| "dashboard_retry_budget_exhausted"
	| "dashboard_state_transition"
	| "dashboard_action"
	| "dashboard_source_authority"
	| "dashboard_command_authority_denied"
	| "open_repo"
	| "import_session"
	| "permission_denied"
	| "apply_filter"
	| "clear_filter"
	| "view_activity";

// Combined telemetry event names for dispatch
export type NarrativeTelemetryEventNameAll =
	| NarrativeTelemetryEventName
	| AskWhyTelemetryEventName
	| DashboardTelemetryEventName;

type DashboardTelemetryPayload = Record<string, unknown>;

// Canonical envelope fields shared across all telemetry
export interface CanonicalTelemetryEnvelope {
	session_id?: string;
	request_key_hash?: string;
	attempt?: number;
	mode?: string;
	window_id?: string;
	ts_iso8601: string;
}

// Telemetry event structure
export type NarrativeTelemetryEventDetail = CanonicalTelemetryEnvelope & {
	schemaVersion: NarrativeTelemetrySchemaVersion;
	event: NarrativeTelemetryEventNameAll;
	payload:
		| NarrativeTelemetryPayload
		| AskWhyTelemetryPayload
		| DashboardTelemetryPayload;
};

declare global {
	interface WindowEventMap {
		"narrative:telemetry": CustomEvent<NarrativeTelemetryEventDetail>;
	}
}

// ============================================================================
// Narrative Telemetry Types
// ============================================================================

type NarrativeEvidenceSource =
	| "demo"
	| "git"
	| "recall_lane"
	| "header_cta"
	| "evidence_link"
	| "raw_diff";

export type NarrativeHeaderKind = "hidden" | "shell" | "full";
export type NarrativeRepoStatus = "idle" | "loading" | "ready" | "error";
export type NarrativeTransitionType = "initial" | "state_change";

export type HeaderQualityReasonCode =
	| "mode_unsupported"
	| "repo_idle"
	| "model_missing"
	| "feature_disabled"
	| "loading"
	| "error"
	| "ready"
	| "unknown";

export type NarrativeTelemetryPayload = {
	request_key?: string;
	schemaVersion?: NarrativeTelemetrySchemaVersion;
	attemptId?: string;
	branch?: string;
	branchScopeKey?: string;
	oldBranchScopeKey?: string;
	newBranchScopeKey?: string;
	viewInstanceId?: string;
	source?: NarrativeEvidenceSource;
	detailLevel?: "summary" | "evidence" | "diff";
	audience?: "executive" | "manager" | "engineer";
	evidenceKind?: "commit" | "session" | "file" | "diff";
	verificationMode?: "evidence-first" | "session-first" | "diff-first";
	workspaceState?:
		| "loading"
		| "error"
		| "empty"
		| "kill_switch"
		| "degraded"
		| "ready-low-confidence"
		| "ready";
	trustPosture?:
		| "none"
		| "hydrating"
		| "live_trusted"
		| "replaying"
		| "trust_paused";
	fixtureClass?:
		| "high-confidence"
		| "low-confidence"
		| "degraded"
		| "kill-switch"
		| "default";
	hasAdvancedAnalysis?: boolean;
	cta?: "inspect-evidence" | "open-raw-diff";
	control?: string;
	initialOpen?: boolean;
	target?: "evidence" | "raw-diff";
	elapsedMs?: number;
	clearedStateKeys?: string[];
	staleAsyncDropped?: boolean;
	confidence?: number;
	rolloutStatus?: "healthy" | "watch" | "rollback";
	score?: number;
	reason?: string;
	reasonCode?: HeaderQualityReasonCode;
	headerKind?: NarrativeHeaderKind;
	repoStatus?: NarrativeRepoStatus;
	transition?: NarrativeTransitionType;
	durationMs?: number;
	budgetMs?: number;
	overBudget?: boolean;
	feedbackType?:
		| "highlight_key"
		| "highlight_wrong"
		| "branch_missing_decision";
	feedbackTargetKind?: "highlight" | "branch";
	feedbackActorRole?: "developer" | "reviewer";
	recallLaneItemId?: string;
	recallLaneConfidenceBand?: "low" | "medium" | "high";
	itemId?: string;
	branchScope?: string;
	eventOutcome?: NarrativeEventOutcome;
	funnelStep?: NarrativeFunnelStep;
	funnelSessionId?: string;
	flowLatencyMs?: number;
};

export type NarrativeEventOutcome =
	| "attempt"
	| "success"
	| "fallback"
	| "failed"
	| "stale_ignored";
export type NarrativeFunnelStep =
	| "what_ready"
	| "why_requested"
	| "why_ready"
	| "evidence_requested"
	| "evidence_ready";

export type NarrativeRenderDecisionInput = {
	branch?: string;
	source: "demo" | "git";
	headerKind: NarrativeHeaderKind;
	repoStatus: NarrativeRepoStatus;
	transition: NarrativeTransitionType;
	reasonCode: HeaderQualityReasonCode;
	durationMs: number;
	budgetMs: number;
};

// ============================================================================
// Utility Functions
// ============================================================================

function sanitizeMs(value: number): number {
	if (!Number.isFinite(value) || value < 0) return 0;
	return Math.round(value * 1000) / 1000;
}

const TELEMETRY_DEDUPE_WINDOW_MS = 750;
const TERMINAL_OUTCOMES: ReadonlySet<NarrativeEventOutcome> = new Set([
	"success",
	"fallback",
	"failed",
	"stale_ignored",
]);
const VALID_EVENT_OUTCOMES: ReadonlySet<NarrativeEventOutcome> = new Set([
	"attempt",
	...TERMINAL_OUTCOMES,
]);
const VALID_FUNNEL_STEPS: ReadonlySet<NarrativeFunnelStep> = new Set([
	"what_ready",
	"why_requested",
	"why_ready",
	"evidence_requested",
	"evidence_ready",
]);
const FIRST_WIN_FLOW_EVENTS: ReadonlySet<NarrativeTelemetryEventNameAll> =
	new Set([
		"what_ready",
		"first_win_completed",
		"evidence_opened",
		"fallback_used",
		"ask_why_submitted",
		"ask_why_answer_viewed",
		"ask_why_evidence_opened",
		"ask_why_fallback_used",
		"ask_why_error",
	]);
const recentTelemetrySignatures = new Map<string, number>();

type NarrativeTelemetryRuntimeConfig = {
	consentGranted: boolean;
};

const runtimeConfig: NarrativeTelemetryRuntimeConfig = {
	consentGranted: true,
};

function sanitizeText(value: string): string {
	let sanitized = "";
	for (const char of value) {
		const code = char.charCodeAt(0);
		if (
			(code >= 32 && code !== 127) ||
			char === "\n" ||
			char === "\r" ||
			char === "\t"
		) {
			sanitized += char;
		}
	}
	return sanitized.trim().slice(0, 240);
}

function isAbsolutePath(value: string): boolean {
	return value.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(value);
}

function sanitizePayloadValue(value: unknown): unknown {
	if (typeof value === "string") return sanitizeText(value);
	if (Array.isArray(value))
		return value.map((entry) => sanitizePayloadValue(entry));
	if (value && typeof value === "object") {
		const next: Record<string, unknown> = {};
		for (const [key, nested] of Object.entries(value)) {
			next[key] = sanitizePayloadValue(nested);
		}
		return next;
	}
	return value;
}

function buildTelemetrySignature(
	event: NarrativeTelemetryEventNameAll,
	payload: NarrativeTelemetryPayload | AskWhyTelemetryPayload,
): string | null {
	const eventOutcome = payload.eventOutcome;
	if (!eventOutcome || !TERMINAL_OUTCOMES.has(eventOutcome)) return null;
	const attemptId = payload.attemptId ?? "";
	const isAskWhyPayload = "queryId" in payload;
	const queryId = isAskWhyPayload ? payload.queryId : "";
	const itemId = isAskWhyPayload
		? (payload.citationId ?? "")
		: (payload.itemId ?? "");
	const branchScope = payload.branchScope ?? "";
	const funnelStep = payload.funnelStep ?? "";
	return `${event}|${eventOutcome}|${attemptId}|${branchScope}|${queryId}|${itemId}|${funnelStep}`;
}

function shouldDropDuplicateTerminalEvent(
	signature: string,
	nowMs: number,
): boolean {
	for (const [
		cachedSignature,
		cachedAtMs,
	] of recentTelemetrySignatures.entries()) {
		if (nowMs - cachedAtMs > TELEMETRY_DEDUPE_WINDOW_MS) {
			recentTelemetrySignatures.delete(cachedSignature);
		}
	}

	const previousAt = recentTelemetrySignatures.get(signature);
	recentTelemetrySignatures.set(signature, nowMs);
	if (previousAt === undefined) return false;
	return nowMs - previousAt <= TELEMETRY_DEDUPE_WINDOW_MS;
}

function validatePayload(
	event: NarrativeTelemetryEventNameAll,
	payload: NarrativeTelemetryPayload | AskWhyTelemetryPayload,
): payload is NarrativeTelemetryPayload | AskWhyTelemetryPayload {
	if (payload.eventOutcome && !VALID_EVENT_OUTCOMES.has(payload.eventOutcome)) {
		return false;
	}
	if (payload.funnelStep && !VALID_FUNNEL_STEPS.has(payload.funnelStep)) {
		return false;
	}
	if (payload.branchScope && isAbsolutePath(payload.branchScope)) {
		return false;
	}
	if (FIRST_WIN_FLOW_EVENTS.has(event)) {
		if (!payload.attemptId || payload.attemptId.trim().length === 0) {
			return false;
		}
	}
	if (event === "first_win_completed") {
		if (payload.funnelStep !== "evidence_ready") return false;
		if (!payload.eventOutcome || !TERMINAL_OUTCOMES.has(payload.eventOutcome))
			return false;
	}
	return true;
}

let currentSessionId: string | undefined;
let activeWindowId: string | undefined;
let appMode: string | undefined;

export function setTelemetryContext(context: {
	sessionId?: string;
	windowId?: string;
	mode?: string;
}) {
	if (context.sessionId) currentSessionId = context.sessionId;
	if (context.windowId) activeWindowId = context.windowId;
	if (context.mode) appMode = context.mode;
}

export function dispatchNarrativeTelemetry(
	event: NarrativeTelemetryEventNameAll,
	payload:
		| NarrativeTelemetryPayload
		| AskWhyTelemetryPayload
		| DashboardTelemetryPayload,
	envelopeOverrides?: Partial<CanonicalTelemetryEnvelope>,
) {
	if (typeof window === "undefined") return;
	if (!runtimeConfig.consentGranted) return;

	const sanitizedPayload = sanitizePayloadValue(payload);
	const normalizedPayload = toRecordPayload(sanitizedPayload);
	const validatablePayload = !isDashboardEvent(event)
		? asValidatablePayload(normalizedPayload)
		: null;

	// NOTE: For now, payload validation works for known types. Dashboard events skip validation or are validated elsewhere.
	if (validatablePayload && !validatePayload(event, validatablePayload)) return;

	const signature = validatablePayload
		? buildTelemetrySignature(event, validatablePayload)
		: null;
	if (signature && shouldDropDuplicateTerminalEvent(signature, Date.now())) {
		return;
	}

	// Canonical envelope required by Phase 4 spec
	const ts_iso8601 = new Date().toISOString();

	// Ensure request_key is hashed if present in payload overrides
	let request_key_hash = envelopeOverrides?.request_key_hash;
	const requestKey = validatablePayload?.request_key;
	if (!request_key_hash && typeof requestKey === "string") {
		request_key_hash = hashString(requestKey);
	}

	window.dispatchEvent(
		new CustomEvent("narrative:telemetry", {
			detail: {
				schemaVersion: "v1" as NarrativeTelemetrySchemaVersion,
				event,
				payload: normalizedPayload,
				session_id: envelopeOverrides?.session_id || currentSessionId,
				request_key_hash,
				attempt:
					envelopeOverrides?.attempt ||
					(typeof normalizedPayload.attemptId === "string" ? 1 : undefined),
				mode: envelopeOverrides?.mode || appMode || "default",
				window_id:
					envelopeOverrides?.window_id || activeWindowId || crypto.randomUUID(),
				ts_iso8601,
			} as NarrativeTelemetryEventDetail,
		}),
	);
}

function asValidatablePayload(
	payload: unknown,
): NarrativeTelemetryPayload | AskWhyTelemetryPayload | null {
	return typeof payload === "object" &&
		payload !== null &&
		("eventOutcome" in payload ||
			"funnelStep" in payload ||
			"branchScope" in payload ||
			"attemptId" in payload)
		? (payload as NarrativeTelemetryPayload | AskWhyTelemetryPayload)
		: null;
}

function isDashboardEvent(
	event: NarrativeTelemetryEventNameAll,
): event is DashboardTelemetryEventName {
	return (
		event === "dashboard_retry_budget_exhausted" ||
		event === "dashboard_state_transition" ||
		event === "dashboard_action" ||
		event === "dashboard_source_authority" ||
		event === "dashboard_command_authority_denied" ||
		event === "open_repo" ||
		event === "import_session" ||
		event === "permission_denied" ||
		event === "apply_filter" ||
		event === "clear_filter" ||
		event === "view_activity"
	);
}

function toRecordPayload(
	payload: unknown,
):
	| NarrativeTelemetryPayload
	| AskWhyTelemetryPayload
	| DashboardTelemetryPayload {
	if (
		typeof payload === "object" &&
		payload !== null &&
		!Array.isArray(payload)
	) {
		return payload as
			| NarrativeTelemetryPayload
			| AskWhyTelemetryPayload
			| DashboardTelemetryPayload;
	}
	return {};
}

function hashString(value: string): string {
	let hash = 2166136261;
	for (let index = 0; index < value.length; index += 1) {
		hash ^= value.charCodeAt(index);
		hash = Math.imul(hash, 16777619);
	}
	return (hash >>> 0).toString(36);
}

export function createTelemetryBranchScope(
	repoId: number | null,
	branchName: string | undefined,
): string {
	const normalizedBranch = sanitizeText(branchName ?? "unknown");
	const repoScope = `r${repoId ?? 0}`;
	return `${repoScope}:b${hashString(normalizedBranch)}`;
}

export function setNarrativeTelemetryRuntimeConfig(
	config: Partial<NarrativeTelemetryRuntimeConfig>,
): void {
	if (typeof config.consentGranted === "boolean") {
		runtimeConfig.consentGranted = config.consentGranted;
	}
}

// ============================================================================
// Narrative Telemetry Functions
// ============================================================================

export function trackNarrativeEvent(
	event: NarrativeTelemetryEventName,
	payload: NarrativeTelemetryPayload = {},
) {
	dispatchNarrativeTelemetry(event, payload);
}

export function trackQualityRenderDecision(
	input: NarrativeRenderDecisionInput,
) {
	const durationMs = sanitizeMs(input.durationMs);
	const budgetMs = sanitizeMs(input.budgetMs);

	trackNarrativeEvent("ui.quality.render_decision", {
		schemaVersion: "v1",
		branch: input.branch,
		source: input.source,
		headerKind: input.headerKind,
		repoStatus: input.repoStatus,
		transition: input.transition,
		reasonCode: input.reasonCode,
		durationMs,
		budgetMs,
		overBudget: durationMs > budgetMs,
	});
}

// ============================================================================
// Ask-Why Telemetry Tracking Functions
// ============================================================================

function trackAskWhyEvent(
	event: AskWhyTelemetryEventName,
	payload: AskWhyTelemetryPayload,
) {
	dispatchNarrativeTelemetry(event, payload);
}

export type TrackAskWhySubmittedInput = {
	queryId: string;
	attemptId: string;
	branchId: string;
	questionHash: string;
	branchScope?: string;
	funnelSessionId?: string;
};

export function trackAskWhySubmitted(input: TrackAskWhySubmittedInput) {
	trackAskWhyEvent("ask_why_submitted", {
		queryId: input.queryId,
		attemptId: input.attemptId,
		branchId: input.branchId,
		branchScope: input.branchScope,
		questionHash: input.questionHash,
		funnelSessionId: input.funnelSessionId,
		funnelStep: "why_requested",
		eventOutcome: "attempt",
	});
}

export type TrackAskWhyAnswerViewedInput = {
	queryId: string;
	attemptId: string;
	confidence: AskWhyConfidenceBand;
	citationCount: number;
	fallbackUsed: boolean;
	branchScope?: string;
	funnelSessionId?: string;
	flowLatencyMs?: number;
};

export function trackAskWhyAnswerViewed(input: TrackAskWhyAnswerViewedInput) {
	trackAskWhyEvent("ask_why_answer_viewed", {
		queryId: input.queryId,
		attemptId: input.attemptId,
		branchScope: input.branchScope,
		confidence: input.confidence,
		citationCount: input.citationCount,
		fallbackUsed: input.fallbackUsed,
		funnelSessionId: input.funnelSessionId,
		funnelStep: "why_ready",
		eventOutcome: input.fallbackUsed ? "fallback" : "success",
		flowLatencyMs:
			typeof input.flowLatencyMs === "number"
				? sanitizeMs(input.flowLatencyMs)
				: undefined,
	});
}

export type TrackAskWhyEvidenceOpenedInput = {
	queryId: string;
	attemptId: string;
	citationType: AskWhyCitationType;
	citationId: string;
	branchScope?: string;
	funnelSessionId?: string;
};

export function trackAskWhyEvidenceOpened(
	input: TrackAskWhyEvidenceOpenedInput,
) {
	trackAskWhyEvent("ask_why_evidence_opened", {
		queryId: input.queryId,
		attemptId: input.attemptId,
		branchScope: input.branchScope,
		citationType: input.citationType,
		citationId: input.citationId,
		funnelSessionId: input.funnelSessionId,
		funnelStep: "evidence_ready",
		eventOutcome: "success",
	});
}

export type TrackAskWhyFallbackUsedInput = {
	queryId: string;
	attemptId: string;
	reasonCode: AskWhyFallbackReasonCode;
	branchScope?: string;
	funnelSessionId?: string;
};

export function trackAskWhyFallbackUsed(input: TrackAskWhyFallbackUsedInput) {
	trackAskWhyEvent("ask_why_fallback_used", {
		queryId: input.queryId,
		attemptId: input.attemptId,
		branchScope: input.branchScope,
		reasonCode: input.reasonCode,
		fallbackUsed: true,
		funnelSessionId: input.funnelSessionId,
		funnelStep: "evidence_requested",
		eventOutcome: "fallback",
	});
}

export type TrackAskWhyErrorInput = {
	queryId: string;
	attemptId: string;
	errorType: string;
	branchScope?: string;
	funnelSessionId?: string;
	eventOutcome?: NarrativeEventOutcome;
};

export function trackAskWhyError(input: TrackAskWhyErrorInput) {
	trackAskWhyEvent("ask_why_error", {
		queryId: input.queryId,
		attemptId: input.attemptId,
		branchScope: input.branchScope,
		errorType: input.errorType,
		funnelSessionId: input.funnelSessionId,
		funnelStep: "why_ready",
		eventOutcome: input.eventOutcome ?? "failed",
	});
}

// ============================================================================
// Dashboard Telemetry Tracking Functions
// ============================================================================

export type TrackDashboardEventInput = {
	event: DashboardTelemetryEventName;
	payload?: Record<string, unknown>;
	envelopeOverrides?: Partial<CanonicalTelemetryEnvelope>;
};

export function trackDashboardEvent(input: TrackDashboardEventInput) {
	// If we have a repo_id, we hash it if it's not a known numeric ID format, but here the spec says:
	// "Enforce hashing/enum requirements for repo_id, request_key, scope, capability, and window."
	const finalPayload = { ...input.payload };
	if (finalPayload.repo_id !== undefined) {
		if (typeof finalPayload.repo_id === "number") {
			finalPayload.repo_id = `r${finalPayload.repo_id}`;
		} else {
			finalPayload.repo_id = hashString(String(finalPayload.repo_id));
		}
	}

	dispatchNarrativeTelemetry(
		input.event,
		finalPayload,
		input.envelopeOverrides,
	);
}
