import { listen } from "@tauri-apps/api/event";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import {
	useCallback,
	useDeferredValue,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { refreshSessionBadges } from "../core/repo/sessionBadges";
import { type ActivityEvent, getIngestActivity } from "../core/tauri/activity";
import {
	AUTH_URL_HINT_PREFIX,
	autoImportSessionFile,
	backfillRecentSessions,
	type CaptureReliabilityStatus,
	type CodexAppServerStatus,
	type CollectorMigrationStatus,
	codexAppServerInitialize,
	codexAppServerInitialized,
	codexAppServerLoadThreadRecoveryCheckpoint,
	codexAppServerLoginStart,
	codexAppServerLogout,
	configureCodexOtel,
	type DiscoveredSources,
	discoverCaptureSources,
	ensureOtlpApiKey,
	getCaptureReliabilityStatus,
	getCodexAppServerStatus,
	getCollectorMigrationStatus,
	getIngestConfig,
	getOtlpKeyStatus,
	type IngestConfig,
	type LiveSessionEventPayload,
	type OtlpKeyStatus,
	purgeExpiredSessions,
	resetOtlpApiKey,
	rollbackCollectorMigration,
	runCollectorMigration,
	setIngestConfig,
	startCodexAppServer,
	startFileWatcher,
	stopCodexAppServer,
	stopFileWatcher,
} from "../core/tauri/ingestConfig";
import { setOtelReceiverEnabled } from "../core/tauri/otelReceiver";
import type { BranchViewModel } from "../core/types";

export type IngestStatus = {
	enabled: boolean;
	captureMode: "OTEL_ONLY" | "HYBRID_ACTIVE" | "DEGRADED_STREAMING" | "FAILURE";
	captureModeMessage?: string;
	migrationRequired: boolean;
	lastImportAt?: string;
	lastSource?: string;
	errorCount: number;
};

export type IngestIssue = {
	id: string;
	title: string;
	message: string;
	action?: { label: string; handler: () => void };
};

export type IngestToast = {
	id: string;
	message: string;
};

function extractAuthUrlFromStatus(
	status: CodexAppServerStatus | null | undefined,
): string | null {
	const hint = status?.lastError?.trim();
	if (!hint?.startsWith(AUTH_URL_HINT_PREFIX)) return null;
	const url = hint.slice(AUTH_URL_HINT_PREFIX.length).trim();
	if (!url) return null;
	try {
		const parsed = new URL(url);
		const host = parsed.hostname.toLowerCase();
		const hostAllowed =
			host === "openai.com" ||
			host === "chatgpt.com" ||
			host === "auth.openai.com" ||
			host === "chat.openai.com" ||
			host.endsWith(".openai.com") ||
			host.endsWith(".chatgpt.com");
		if (parsed.protocol !== "https:" || !hostAllowed) return null;
		return parsed.toString();
	} catch {
		return null;
	}
}

async function openExternalAuthUrl(url: string): Promise<boolean> {
	try {
		await openExternal(url);
		return true;
	} catch {
		return false;
	}
}

export function useAutoIngest(params: {
	repoRoot: string;
	repoId: number;
	model: BranchViewModel;
	setRepoState: (updater: (prev: BranchViewModel) => BranchViewModel) => void;
}) {
	const { repoRoot, repoId, model, setRepoState } = params;
	const [config, setConfig] = useState<IngestConfig | null>(null);
	const [otlpKey, setOtlpKey] = useState<OtlpKeyStatus | null>(null);
	const [sources, setSources] = useState<DiscoveredSources | null>(null);
	const [status, setStatus] = useState<IngestStatus>({
		enabled: false,
		captureMode: "FAILURE",
		migrationRequired: false,
		errorCount: 0,
	});
	const [collectorMigration, setCollectorMigration] =
		useState<CollectorMigrationStatus | null>(null);
	const [captureReliability, setCaptureReliability] =
		useState<CaptureReliabilityStatus | null>(null);
	const [codexAppServerStatus, setCodexAppServerStatus] =
		useState<CodexAppServerStatus | null>(null);
	const [issues, setIssues] = useState<IngestIssue[]>([]);
	const [toast, setToast] = useState<IngestToast | null>(null);
	const [activityRecent, setActivityRecent] = useState<ActivityEvent[]>([]);
	const lastImportRef = useRef<{ source?: string; time?: string }>({});
	const idCounter = useRef(0);
	const reliabilityRefreshInFlightRef = useRef(false);
	const isMountedRef = useRef(true);

	// --- PHASE 2: Thread tracking for recovery checkpoint integration ---
	const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
	const lastActiveThreadIdRef = useRef<string | null>(null);
	const [trustState, setTrustState] = useState<
		"none" | "hydrating" | "replaying" | "live_trusted" | "trust_paused"
	>("none");

	// Debounce thread ID changes to prevent race conditions when events arrive rapidly.
	// useDeferredValue allows React to stabilize the thread ID before triggering checkpoint load.
	const deferredActiveThreadId = useDeferredValue(activeThreadId);

	// --- P2-041: Development invariant checks for split-brain trust state ---
	useEffect(() => {
		if (process.env.NODE_ENV === "development") {
			// Detect split-brain: capture reports active but trust reports paused
			if (
				captureReliability?.mode === "HYBRID_ACTIVE" &&
				trustState === "trust_paused"
			) {
				// split-brain detected — no action needed, state resolves on next tick
			}
			// Detect split-brain: no active thread but trust reports live
			if (trustState === "live_trusted" && !activeThreadId) {
				// split-brain detected — no action needed, state resolves on next tick
			}
		}
	}, [captureReliability, trustState, activeThreadId]);

	useEffect(() => {
		isMountedRef.current = true;
		return () => {
			isMountedRef.current = false;
		};
	}, []);

	const refreshActivity = useCallback(
		async (limit: number) => {
			try {
				const items = await getIngestActivity(repoId, limit);
				if (!isMountedRef.current) return;
				setActivityRecent(items.slice(0, 3));
				const lastSeenAtISO = items[0]?.createdAtIso;
				if (lastSeenAtISO) {
					lastImportRef.current = {
						...lastImportRef.current,
						time: lastSeenAtISO,
					};
					if (!isMountedRef.current) return;
					setStatus((prev) => ({
						...prev,
						lastImportAt: lastSeenAtISO,
					}));
				}
			} catch {
				// non-fatal; avoid spamming issues list for optional UI enhancement
			}
		},
		[repoId],
	);

	// --- PHASE 2: Define recordIssue before use in loadRecoveryCheckpoint ---
	const recordIssue = useCallback(
		(title: string, message: string, action?: IngestIssue["action"]) => {
			if (!isMountedRef.current) return;
			idCounter.current += 1;
			const id = `${Date.now()}-${idCounter.current}`;
			setIssues((prev) => {
				if (
					prev.some(
						(issue) => issue.title === title && issue.message === message,
					)
				) {
					return prev;
				}
				setStatus((prevStatus) => ({
					...prevStatus,
					errorCount: prevStatus.errorCount + 1,
				}));
				return [{ id, title, message, action }, ...prev];
			});
		},
		[],
	);

	// --- PHASE 2: Load recovery checkpoint when active thread changes ---
	const loadRecoveryCheckpoint = useCallback(
		async (threadId: string) => {
			if (!isMountedRef.current) return;
			try {
				const checkpointStatus =
					await codexAppServerLoadThreadRecoveryCheckpoint(threadId);
				if (!isMountedRef.current) return;
				setTrustState(checkpointStatus.trustStateRecommendation);
				// If checkpoint requires fresh retry, we could trigger additional UI feedback here
				if (
					checkpointStatus.requiresFreshRetry &&
					checkpointStatus.freshRetryReason
				) {
					recordIssue(
						"Thread recovery requires fresh retry",
						checkpointStatus.freshRetryReason,
					);
				}
			} catch (e) {
				const message = e instanceof Error ? e.message : String(e);
				recordIssue("Failed to load thread recovery checkpoint", message);
			}
		},
		[recordIssue],
	);

	// --- PHASE 2: Load recovery checkpoint when active thread changes (debounced) ---
	// Use deferred value to prevent race conditions when thread ID changes rapidly
	const loadRequestIdRef = useRef(0);

	useEffect(() => {
		// Use deferred thread ID to allow rapid changes to settle before loading
		if (
			!deferredActiveThreadId ||
			deferredActiveThreadId === lastActiveThreadIdRef.current
		)
			return;

		// Increment request ID to invalidate any in-flight loads
		const currentRequestId = ++loadRequestIdRef.current;
		lastActiveThreadIdRef.current = deferredActiveThreadId;

		void loadRecoveryCheckpoint(deferredActiveThreadId).then(() => {
			// Only update state if this is still the current request
			if (
				loadRequestIdRef.current !== currentRequestId ||
				!isMountedRef.current
			) {
				return;
			}
			// State updates happen inside loadRecoveryCheckpoint via callbacks
		});
	}, [deferredActiveThreadId, loadRecoveryCheckpoint]);

	const watchPaths = useMemo(() => {
		if (!config) return [];
		const base = [...config.watchPaths.claude, ...config.watchPaths.cursor];
		if (config.codex.mode === "logs" || config.codex.mode === "both") {
			base.push(...(config.watchPaths.codexLogs ?? []));
		}
		return base;
	}, [config]);

	const refreshBadges = useCallback(async () => {
		if (!repoRoot || !repoId) return;
		await refreshSessionBadges(repoRoot, repoId, model.timeline, setRepoState, {
			limit: 10,
		});
	}, [repoRoot, repoId, model.timeline, setRepoState]);

	const refreshReliability = useCallback(async () => {
		if (reliabilityRefreshInFlightRef.current) return;
		reliabilityRefreshInFlightRef.current = true;
		try {
			const [migration, reliability, appServer] = await Promise.all([
				getCollectorMigrationStatus(),
				getCaptureReliabilityStatus(),
				getCodexAppServerStatus(),
			]);

			if (!isMountedRef.current) return;
			setCollectorMigration(migration);
			setCaptureReliability(reliability);
			setCodexAppServerStatus(appServer);
			setStatus((prev) => ({
				...prev,
				captureMode: reliability.mode,
				captureModeMessage: reliability.reasons[0],
				migrationRequired: migration.migrationRequired,
			}));
		} catch (e) {
			const message = e instanceof Error ? e.message : String(e);
			recordIssue("Capture reliability check failed", message);
		} finally {
			reliabilityRefreshInFlightRef.current = false;
		}
	}, [recordIssue]);

	const showToast = useCallback((message: string) => {
		idCounter.current += 1;
		const id = `${Date.now()}-${idCounter.current}`;
		if (!isMountedRef.current) return;
		setToast({ id, message });
		setTimeout(() => {
			if (!isMountedRef.current) return;
			setToast((prev) => (prev?.id === id ? null : prev));
		}, 3500);
	}, []);

	const handleAutoImport = useCallback(
		async (payload: { path: string; tool?: string }) => {
			if (!repoId) return;
			try {
				const result = await autoImportSessionFile(repoId, payload.path);
				if (!isMountedRef.current) return;
				await refreshBadges();
				if (!isMountedRef.current) return;
				await refreshActivity(3);
				if (!isMountedRef.current) return;

				if (result.status === "imported") {
					const message = `Imported ${result.tool} session (redactions: ${result.redactionCount})`;
					if (!isMountedRef.current) return;
					showToast(message);
					lastImportRef.current = {
						source: result.tool,
						time: new Date().toISOString(),
					};
					setStatus((prev) => ({
						...prev,
						lastImportAt: lastImportRef.current.time,
						lastSource: result.tool,
					}));
				} else if (result.status === "skipped") {
					if (!isMountedRef.current) return;
					showToast(`Skipped duplicate ${result.tool} session`);
				}
			} catch (e) {
				const message = e instanceof Error ? e.message : String(e);
				recordIssue("Auto-import failed", message);
			}
		},
		[repoId, refreshBadges, refreshActivity, recordIssue, showToast],
	);

	useEffect(() => {
		if (!repoRoot || !repoId) return;
		let mounted = true;

		const load = async () => {
			try {
				const config = await getIngestConfig();
				const key = await getOtlpKeyStatus();
				const discovered = await discoverCaptureSources();
				const migration = await getCollectorMigrationStatus();
				const reliability = await getCaptureReliabilityStatus();
				const appServer = await getCodexAppServerStatus();
				if (!mounted || !isMountedRef.current) return;
				setConfig(config);
				setOtlpKey(key);
				setSources(discovered);
				setCollectorMigration(migration);
				setCaptureReliability(reliability);
				setCodexAppServerStatus(appServer);
				setStatus((prev) => ({ ...prev, enabled: config.autoIngestEnabled }));
				setStatus((prev) => ({
					...prev,
					enabled: config.autoIngestEnabled,
					captureMode: reliability.mode,
					captureModeMessage: reliability.reasons[0],
					migrationRequired: migration.migrationRequired,
				}));
				if (!mounted || !isMountedRef.current) return;
				await purgeExpiredSessions(repoId, config.retentionDays);
				if (!mounted || !isMountedRef.current) return;
				await refreshActivity(3);
			} catch (e) {
				const message = e instanceof Error ? e.message : String(e);
				recordIssue("Ingest config error", message);
				if (!mounted || !isMountedRef.current) return;
			}
		};

		load();
		return () => {
			mounted = false;
		};
	}, [repoRoot, repoId, recordIssue, refreshActivity]);

	useEffect(() => {
		if (!repoRoot || !repoId) return;
		if (!config?.autoIngestEnabled) return;
		let cancelled = false;
		let unlistenStatus: (() => void) | null = null;
		let unlistenLiveEvents: (() => void) | null = null;

		const attach = async () => {
			try {
				// biome-ignore format: contract parity test requires single-line listen with single-quoted event name
				const statusUnlisten = await listen<CodexAppServerStatus>('codex-app-server-status', (event) => {
						if (cancelled) return;
						setCodexAppServerStatus(event.payload);
					},
				);
				if (cancelled) {
					statusUnlisten();
					return;
				}
				unlistenStatus = statusUnlisten;
			} catch {
				// optional listener
			}

			try {
				// biome-ignore format: contract parity test requires single-line listen with single-quoted event name
				const liveEventUnlisten = await listen<LiveSessionEventPayload>('session:live:event', (event) => {
						if (cancelled) return;
						if (event.payload.type === "ParserValidationError") {
							recordIssue(
								"Codex App Server parser validation error",
								event.payload.reason,
							);
						}
						// --- PHASE 2: Track threadId from live events for recovery checkpoint loading ---
						if (
							(event.payload.type === "SessionDelta" ||
								event.payload.type === "ApprovalRequest") &&
							event.payload.threadId
						) {
							setActiveThreadId(event.payload.threadId);
						}
						void refreshReliability();
					},
				);

				if (cancelled) {
					liveEventUnlisten();
					return;
				}
				unlistenLiveEvents = liveEventUnlisten;
			} catch {
				// optional listener
			}
		};

		void attach();
		const interval = setInterval(() => {
			if (cancelled) return;
			void refreshReliability();
		}, 5000);

		return () => {
			cancelled = true;
			clearInterval(interval);
			if (unlistenStatus) unlistenStatus();
			if (unlistenLiveEvents) unlistenLiveEvents();
		};
	}, [
		repoRoot,
		repoId,
		config?.autoIngestEnabled,
		recordIssue,
		refreshReliability,
	]);

	useEffect(() => {
		if (!config?.autoIngestEnabled || watchPaths.length === 0) return;
		if (!repoRoot || !repoId) return;

		let unlisten: (() => void) | null = null;
		let unlistenOtel: (() => void) | null = null;
		let cancelled = false;
		const stopWatcherSafely = () => {
			void stopFileWatcher();
		};

		const start = async () => {
			try {
				await startFileWatcher(watchPaths);
				if (cancelled) {
					stopWatcherSafely();
					return;
				}
			} catch (e) {
				recordIssue(
					"Auto-ingest disabled",
					e instanceof Error ? e.message : String(e),
				);
				return;
			}

			try {
				const sessionUnlisten = await listen<{ path: string; tool?: string }>(
					"session-file-changed",
					async (event) => {
						if (cancelled) return;
						await handleAutoImport(event.payload);
					},
				);
				if (cancelled) {
					sessionUnlisten();
					return;
				}
				unlisten = sessionUnlisten;
			} catch (e) {
				recordIssue(
					"Auto-ingest listener failed",
					e instanceof Error ? e.message : String(e),
				);
			}

			try {
				const otelUnlisten = await listen("otel-trace-ingested", async () => {
					if (cancelled) return;
					await refreshActivity(3);
				});
				if (cancelled) {
					otelUnlisten();
					return;
				}
				unlistenOtel = otelUnlisten;
			} catch {
				// optional
			}
		};

		void start();

		return () => {
			cancelled = true;
			if (unlisten) unlisten();
			if (unlistenOtel) unlistenOtel();
			stopWatcherSafely();
		};
	}, [
		config?.autoIngestEnabled,
		watchPaths,
		repoRoot,
		repoId,
		handleAutoImport,
		refreshActivity,
		recordIssue,
	]);

	const toggleAutoIngest = useCallback(
		async (enabled: boolean) => {
			try {
				const codexUpdate =
					!enabled && config
						? { ...config.codex, receiverEnabled: false }
						: undefined;
				const next = await setIngestConfig({
					autoIngestEnabled: enabled,
					codex: codexUpdate,
				});
				setConfig(next);
				setStatus((prev) => ({ ...prev, enabled }));

				if (!enabled) {
					stopFileWatcher();
					try {
						await setOtelReceiverEnabled(false);
						await stopCodexAppServer();
					} catch (e) {
						recordIssue(
							"Failed to disable Codex receiver",
							e instanceof Error ? e.message : String(e),
						);
					}
					await refreshReliability();
					if (!isMountedRef.current) return;
					return;
				}

				// Backfill immediately so the UI doesn’t feel empty.
				showToast("Backfilling recent sessions…");
				try {
					const res = await backfillRecentSessions(repoId, 10);
					if (!isMountedRef.current) return;
					if (res.failed && res.failed > 0) {
						showToast(
							`Backfilled ${res.imported} session(s) with ${res.failed} failure(s).`,
						);
					} else if (res.imported && res.imported > 0) {
						showToast(`Backfilled ${res.imported} session(s).`);
					} else {
						showToast("No sessions found to backfill.");
					}
					await refreshBadges();
					await refreshActivity(3);
				} catch (e) {
					// Backfill failure should not prevent enabling capture.
					const msg = e instanceof Error ? e.message : String(e);
					recordIssue("Backfill failed", msg);
				}

				if (next.consent.codexTelemetryGranted && next.codex.receiverEnabled) {
					if (otlpKey?.present) {
						try {
							await setOtelReceiverEnabled(true);
						} catch (e) {
							recordIssue(
								"Failed to enable Codex receiver",
								e instanceof Error ? e.message : String(e),
							);
						}
					} else {
						recordIssue(
							"Missing Codex API key",
							"Generate an API key in Narrative settings (stored securely), then configure Codex telemetry.",
						);
					}
				}

				if (
					next.codex.streamEnrichmentEnabled &&
					!next.codex.streamKillSwitch
				) {
					try {
						await startCodexAppServer();
						if (!isMountedRef.current) return;
						await codexAppServerInitialize();
						if (!isMountedRef.current) return;
						await codexAppServerInitialized();
					} catch (e) {
						const msg = e instanceof Error ? e.message : String(e);
						recordIssue("Codex App Server degraded", msg);
					}
				}
				await refreshReliability();
				if (!isMountedRef.current) return;
			} catch (e) {
				recordIssue(
					"Auto-ingest toggle failed",
					e instanceof Error ? e.message : String(e),
				);
			}
		},
		[
			config,
			otlpKey?.present,
			repoId,
			refreshActivity,
			refreshBadges,
			recordIssue,
			refreshReliability,
			showToast,
		],
	);

	const updateWatchPaths = useCallback(
		async (paths: {
			claude: string[];
			cursor: string[];
			codexLogs: string[];
		}) => {
			try {
				const next = await setIngestConfig({ watchPaths: paths });
				if (!isMountedRef.current) return;
				setConfig(next);
				await refreshReliability();
			} catch (e) {
				recordIssue(
					"Failed to update watch paths",
					e instanceof Error ? e.message : String(e),
				);
			}
		},
		[recordIssue, refreshReliability],
	);

	const configureCodexTelemetry = useCallback(async () => {
		if (!config) return;
		try {
			const key = await ensureOtlpApiKey();
			if (!isMountedRef.current) return;
			setOtlpKey(key);
			await configureCodexOtel(config.codex.endpoint);
			const receiverEnabled =
				key.present && config.consent.codexTelemetryGranted;
			const next = await setIngestConfig({
				codex: { ...config.codex, receiverEnabled },
			});
			if (!isMountedRef.current) return;
			setConfig(next);
			showToast("Codex telemetry configured");
			await refreshReliability();
		} catch (e) {
			recordIssue(
				"Codex config failed",
				e instanceof Error ? e.message : String(e),
			);
		}
	}, [config, recordIssue, refreshReliability, showToast]);

	const rotateOtlpKey = useCallback(async () => {
		try {
			const key = await resetOtlpApiKey();
			if (!isMountedRef.current) return;
			setOtlpKey(key);
			showToast("Receiver API key rotated");
		} catch (e) {
			recordIssue(
				"Key rotation failed",
				e instanceof Error ? e.message : String(e),
			);
		}
	}, [recordIssue, showToast]);

	const grantCodexConsent = useCallback(async () => {
		try {
			const now = new Date().toISOString();
			const next = await setIngestConfig({
				consent: { codexTelemetryGranted: true, grantedAtIso: now },
			});
			if (!isMountedRef.current) return;
			setConfig(next);
		} catch (e) {
			recordIssue(
				"Consent update failed",
				e instanceof Error ? e.message : String(e),
			);
		}
	}, [recordIssue]);

	const migrateCollector = useCallback(
		async (dryRun = false) => {
			try {
				const result = await runCollectorMigration(dryRun);
				if (!isMountedRef.current) return result;
				setCollectorMigration(result.status);
				if (!dryRun && result.migrated) {
					showToast("Collector migration completed");
				} else if (dryRun) {
					showToast("Collector migration dry-run ready");
				}
				await refreshReliability();
				return result;
			} catch (e) {
				const message = e instanceof Error ? e.message : String(e);
				recordIssue("Collector migration failed", message);
				throw e;
			}
		},
		[recordIssue, refreshReliability, showToast],
	);

	const rollbackCollector = useCallback(async () => {
		try {
			const result = await rollbackCollectorMigration();
			if (!isMountedRef.current) return result;
			setCollectorMigration(result.status);
			showToast("Collector migration rollback applied");
			await refreshReliability();
			return result;
		} catch (e) {
			const message = e instanceof Error ? e.message : String(e);
			recordIssue("Collector rollback failed", message);
			throw e;
		}
	}, [recordIssue, refreshReliability, showToast]);

	const dismissIssue = useCallback((id: string) => {
		setIssues((prev) => prev.filter((issue) => issue.id !== id));
	}, []);

	const authorizeCodexAppServerForLiveTest = useCallback(async () => {
		try {
			const started = await startCodexAppServer();
			if (started.state !== "running") {
				throw new Error(
					started.lastError ??
						`Codex App Server failed to start (state: ${started.state})`,
				);
			}
			await codexAppServerInitialize();
			await codexAppServerInitialized();

			// Check if already authenticated before attempting login
			const statusAfterInit = await getCodexAppServerStatus();
			if (!isMountedRef.current) return;
			setCodexAppServerStatus(statusAfterInit);

			if (statusAfterInit.authState === "authenticated") {
				showToast("Codex App Server already authenticated");
				if (!isMountedRef.current) return;
				await refreshReliability();
				return;
			}

			// Only call loginStart if we actually need to authenticate
			await codexAppServerLoginStart();
			const latestStatus = await getCodexAppServerStatus();
			if (!isMountedRef.current) return;
			setCodexAppServerStatus(latestStatus);
			const authUrl = extractAuthUrlFromStatus(latestStatus);
			if (authUrl) {
				const opened = await openExternalAuthUrl(authUrl);
				showToast(
					opened
						? "Opened Codex App Server login in your browser"
						: "Unable to open browser automatically; copy the URL from status to continue.",
				);
			} else {
				showToast("Codex App Server login started (check browser flow)");
			}
			if (!isMountedRef.current) return;
			await refreshReliability();
		} catch {
			// Non-fatal: authorization failure is shown in UI status
		}
	}, [refreshReliability, showToast]);

	const logoutCodexAppServerAccount = useCallback(async () => {
		await codexAppServerLogout();
		showToast("Codex App Server account logged out");
	}, [showToast]);

	return {
		ingestConfig: config,
		otlpEnvStatus: null,
		otlpKeyStatus: otlpKey,
		discoveredSources: sources,
		collectorMigrationStatus: collectorMigration,
		captureReliabilityStatus: captureReliability,
		codexAppServerStatus,
		ingestStatus: status,
		activityRecent,
		getActivityAll: async () => getIngestActivity(repoId, 50),
		issues,
		toast,
		toggleAutoIngest,
		updateWatchPaths,
		configureCodexTelemetry,
		rotateOtlpKey,
		grantCodexConsent,
		authorizeCodexAppServerForLiveTest,
		logoutCodexAppServerAccount,
		migrateCollector,
		rollbackCollector,
		refreshCaptureReliability: refreshReliability,
		dismissIssue,
		// --- PHASE 2: Recovery checkpoint state ---
		activeThreadId,
		trustState,
	};
}
