import { open as openExternal } from "@tauri-apps/plugin-shell";
import type { ReactNode } from "react";
import {
	AUTH_URL_HINT_PREFIX,
	type CaptureReliabilityStatus,
	type CollectorMigrationStatus,
} from "../../../core/tauri/ingestConfig";
import { HelpPopover } from "../HelpPopover";
import { Toggle } from "../Toggle";

export type DetectionStatus =
	| "idle"
	| "searching"
	| "found"
	| "not-found"
	| "error";

export type WatchPaths = {
	claude: string[];
	cursor: string[];
	codexLogs: string[];
};

function captureModeClass(captureMode: string) {
	if (captureMode === "HYBRID_ACTIVE")
		return "bg-accent-green-bg text-accent-green border-accent-green-light";
	if (captureMode === "OTEL_ONLY")
		return "bg-accent-blue-bg text-accent-blue border-accent-blue-light";
	if (captureMode === "DEGRADED_STREAMING")
		return "bg-accent-amber-bg text-accent-amber border-accent-amber-light";
	if (captureMode === "FAILURE")
		return "bg-accent-red-bg text-accent-red border-accent-red-light";
	return "bg-bg-tertiary text-text-secondary border-border-subtle";
}

export function PanelHeader() {
	return (
		<div className="flex items-center justify-between">
			<div>
				<div className="section-header">Auto-Ingest Setup</div>
				<div className="section-subheader mt-0.5">connect once</div>
			</div>
			<HelpPopover
				content="Auto-ingest monitors your local AI interaction logs (Claude, Cursor, etc.) and automatically links them to your git commits."
				label="About auto-ingest"
			/>
		</div>
	);
}

export function CaptureModeCard(props: {
	captureMode: string;
	captureReliability: CaptureReliabilityStatus | null | undefined;
	authBusy: boolean;
	onRefreshReliability?: () => void;
	onAuthorize?: () => Promise<void>;
	onLogout?: () => Promise<void>;
}) {
	const {
		captureMode,
		captureReliability,
		authBusy,
		onRefreshReliability,
		onAuthorize,
		onLogout,
	} = props;
	const appServerStatus = captureReliability?.appServer;
	const authUrl = (() => {
		const hint = appServerStatus?.lastError?.trim();
		if (!hint?.startsWith(AUTH_URL_HINT_PREFIX)) return null;
		const value = hint.slice(AUTH_URL_HINT_PREFIX.length).trim();
		if (!value) return null;
		try {
			const parsed = new URL(value);
			if (parsed.protocol !== "https:") return null;
			return parsed.toString();
		} catch {
			return null;
		}
	})();
	const isAuthorized = appServerStatus?.authState === "authenticated";
	const isAuthFlowInProgress = appServerStatus?.authState === "authenticating";
	const hasAuthUrl = Boolean(authUrl);
	const primaryAuthLabel = isAuthorized
		? "Authorized"
		: isAuthFlowInProgress
			? "Authorizing…"
			: "Login for live test";
	const handleLoginInBrowser = async () => {
		if (!authUrl) return;
		try {
			await openExternal(authUrl);
		} catch {
			// Intentionally no-op: caller will still show the URL in status.
		}
	};

	return (
		<div className="rounded-lg border border-border-subtle bg-bg-secondary p-3">
			<div className="flex items-center justify-between gap-2">
				<div className="text-xs font-semibold text-text-secondary">
					Capture mode
				</div>
				<button
					type="button"
					className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold"
					onClick={() => onRefreshReliability?.()}
				>
					Refresh
				</button>
			</div>
			<div className="mt-2 flex flex-wrap items-center gap-2">
				<span
					className={`inline-flex items-center rounded-full px-2 py-0.5 text-[0.6875rem] font-semibold border ${captureModeClass(captureMode)}`}
				>
					{captureMode}
				</span>
				{captureReliability?.reasons?.[0] ? (
					<span className="text-[0.6875rem] text-text-tertiary">
						{captureReliability.reasons[0]}
					</span>
				) : (
					<span className="text-[0.6875rem] text-text-tertiary">
						Reliability status not yet available.
					</span>
				)}
			</div>
			{appServerStatus ? (
				<div className="mt-3 rounded-md border border-border-subtle bg-bg-tertiary p-2">
					<div className="text-[0.6875rem] font-semibold text-text-secondary">
						Codex App Server
					</div>
					<div className="mt-1 text-[0.6875rem] text-text-tertiary">
						state: <span className="font-mono">{appServerStatus.state}</span> ·
						auth: <span className="font-mono">{appServerStatus.authState}</span>{" "}
						· initialized:{" "}
						<span className="font-mono">
							{appServerStatus.initialized ? "yes" : "no"}
						</span>
					</div>
					{appServerStatus.authState === "authenticated" ? (
						<div className="mt-1 text-[0.6875rem] text-accent-green">
							Authorization status: logged in and authorized.
						</div>
					) : appServerStatus.lastError ? (
						<div className="mt-1 text-[0.6875rem] text-text-tertiary">
							{appServerStatus.lastError}
						</div>
					) : null}
					<div className="mt-2 flex flex-wrap items-center gap-2">
						{isAuthorized ? (
							<button
								type="button"
								className="inline-flex items-center rounded-full border border-accent-green-light bg-accent-green-bg px-2 py-1 text-[0.6875rem] font-semibold text-accent-green"
								disabled
								onClick={() => void onAuthorize?.()}
							>
								{primaryAuthLabel}
							</button>
						) : (
							<button
								type="button"
								className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold disabled:opacity-50"
								disabled={authBusy}
								onClick={() => void onAuthorize?.()}
							>
								{primaryAuthLabel}
							</button>
						)}
						{hasAuthUrl ? (
							<button
								type="button"
								className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold disabled:opacity-50"
								disabled={authBusy}
								onClick={() => {
									void handleLoginInBrowser();
								}}
							>
								Login
							</button>
						) : null}
						{isAuthorized ? (
							<span className="inline-flex items-center rounded-full px-2 py-1 text-[0.625rem] font-semibold border border-accent-green-light bg-accent-green-bg text-accent-green">
								Logged in
							</span>
						) : null}
						<button
							type="button"
							className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold disabled:opacity-50"
							disabled={authBusy}
							onClick={() => void onLogout?.()}
						>
							Logout
						</button>
					</div>
				</div>
			) : null}
		</div>
	);
}

export function CollectorMigrationCard(props: {
	migrationStatus?: CollectorMigrationStatus | null;
	migrationBusy: boolean;
	onMigrateNow: () => Promise<void>;
	onDryRun: () => Promise<void>;
	onRollback: () => Promise<void>;
}) {
	const { migrationStatus, migrationBusy, onMigrateNow, onDryRun, onRollback } =
		props;

	return (
		<div className="rounded-lg border border-border-subtle bg-bg-secondary p-3">
			<div className="text-xs font-semibold text-text-secondary">
				Collector migration
			</div>
			<div className="mt-1 text-[0.6875rem] text-text-tertiary">
				Canonical root:{" "}
				<span className="font-mono">
					{migrationStatus?.canonicalRoot ?? "~/.agents/otel-collector"}
				</span>
			</div>
			<div className="mt-1 text-[0.6875rem] text-text-tertiary">
				Legacy root:{" "}
				<span className="font-mono">
					{migrationStatus?.legacyRoot ?? "~/.agents/otel/collector"}
				</span>
			</div>
			<div className="mt-2 flex flex-wrap items-center gap-2">
				<span
					className={`inline-flex items-center rounded-full px-2 py-0.5 text-[0.6875rem] font-semibold border ${
						migrationStatus?.migrationRequired
							? "bg-accent-amber-bg text-accent-amber border-accent-amber-light"
							: "bg-accent-green-bg text-accent-green border-accent-green-light"
					}`}
				>
					{migrationStatus?.migrationRequired
						? "Migration required"
						: "Canonicalized"}
				</span>
				{migrationStatus?.status ? (
					<span className="inline-flex items-center rounded-full border border-border-subtle bg-bg-tertiary px-2 py-0.5 text-[0.6875rem] text-text-secondary">
						Status: {migrationStatus.status}
					</span>
				) : null}
			</div>
			<div className="mt-2 flex flex-wrap items-center gap-2">
				<ActionButton disabled={migrationBusy} onClick={onMigrateNow}>
					{migrationBusy ? "Migrating…" : "Migrate now"}
				</ActionButton>
				<ActionButton disabled={migrationBusy} onClick={onDryRun}>
					Dry run
				</ActionButton>
				<ActionButton disabled={migrationBusy} onClick={onRollback}>
					Rollback
				</ActionButton>
			</div>
			{migrationStatus?.lastError ? (
				<div className="mt-2 text-[0.6875rem] text-accent-red">
					{migrationStatus.lastError}
				</div>
			) : null}
		</div>
	);
}

export function AutoIngestToggleCard(props: {
	enabled: boolean;
	onToggle: (enabled: boolean) => void;
}) {
	const { enabled, onToggle } = props;
	return (
		<div className="flex items-center justify-between gap-2 rounded-lg border border-border-subtle bg-bg-secondary p-3">
			<div className="flex flex-col gap-1">
				<span className="text-xs font-medium text-text-secondary">
					Enable auto‑ingest
				</span>
				<span className="text-[0.6875rem] text-text-tertiary">
					Process logs in background
				</span>
			</div>
			<Toggle
				checked={enabled}
				onCheckedChange={(checked) => onToggle(checked)}
				aria-label="Enable auto-ingest"
			/>
		</div>
	);
}

export function DiscoveryCard(props: {
	autoIngestEnabled: boolean;
	discoveredSummary: string | null;
	detectionStatus: DetectionStatus;
	statusMessage: string | null;
	onAutoDetect: () => void;
}) {
	const {
		autoIngestEnabled,
		discoveredSummary,
		detectionStatus,
		statusMessage,
		onAutoDetect,
	} = props;
	return (
		<div className="rounded-lg border border-border-subtle bg-bg-secondary p-3">
			<div className="flex flex-wrap items-center gap-2 mb-2">
				<span
					className={`inline-flex items-center rounded-full px-2 py-0.5 text-[0.6875rem] font-semibold ${
						autoIngestEnabled
							? "bg-accent-green-bg text-accent-green border border-accent-green-light"
							: "bg-bg-tertiary text-text-tertiary border border-border-subtle"
					}`}
				>
					{autoIngestEnabled ? "Active" : "Inactive"}
				</span>
				{discoveredSummary ? (
					<span className="inline-flex items-center rounded-full border border-border-subtle bg-bg-tertiary px-2 py-0.5 text-[0.6875rem] font-medium text-text-secondary">
						{discoveredSummary}
					</span>
				) : null}
			</div>
			<div className="text-[0.6875rem] text-text-tertiary">
				Add source folders quickly, then save watch paths. Open advanced editor
				only if you need manual path tuning.
			</div>
			<div className="mt-2 flex items-center gap-2">
				<button
					type="button"
					className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold"
					onClick={onAutoDetect}
					disabled={detectionStatus === "searching"}
				>
					{detectionStatus === "searching"
						? "Auto-detecting…"
						: "Auto-detect paths"}
				</button>
				{statusMessage ? (
					<span
						className={`text-[0.6875rem] ${detectionStatus === "error" ? "text-accent-red" : detectionStatus === "not-found" ? "text-accent-orange" : "text-text-tertiary"}`}
					>
						{statusMessage}
					</span>
				) : null}
			</div>
		</div>
	);
}

export function WatchPathsCard(props: {
	claudePaths: string;
	cursorPaths: string;
	codexPaths: string;
	showAdvancedPaths: boolean;
	onAddClaudeFolder: () => void;
	onAddCursorFolder: () => void;
	onAddCodexFolder: () => void;
	onSaveWatchPaths: () => void;
	onToggleAdvanced: () => void;
	onClaudePathsChange: (next: string) => void;
	onCursorPathsChange: (next: string) => void;
	onCodexPathsChange: (next: string) => void;
}) {
	const {
		claudePaths,
		cursorPaths,
		codexPaths,
		showAdvancedPaths,
		onAddClaudeFolder,
		onAddCursorFolder,
		onAddCodexFolder,
		onSaveWatchPaths,
		onToggleAdvanced,
		onClaudePathsChange,
		onCursorPathsChange,
		onCodexPathsChange,
	} = props;

	return (
		<div className="rounded-lg border border-border-subtle bg-bg-tertiary p-3">
			<div className="flex items-center justify-between mb-2">
				<div className="text-xs font-semibold text-text-secondary">
					Watch Paths
				</div>
				<HelpPopover
					content={
						<div className="space-y-1">
							<p>Locations where Narrator looks for AI conversation logs.</p>
							<p className="font-mono text-[0.625rem] text-text-muted">
								~/.codex/sessions
							</p>
						</div>
					}
				/>
			</div>

			<div className="mt-2 flex flex-wrap gap-2">
				<ActionButton onClick={onAddClaudeFolder}>
					Add Claude folder…
				</ActionButton>
				<ActionButton onClick={onAddCursorFolder}>
					Add Cursor folder…
				</ActionButton>
				<ActionButton onClick={onAddCodexFolder}>
					Add Codex folder…
				</ActionButton>
			</div>

			<div className="mt-3 flex items-center gap-2">
				<button
					type="button"
					className="inline-flex items-center rounded-md border border-accent-blue-light bg-accent-blue-bg px-2 py-1 text-[0.6875rem] font-semibold text-accent-blue hover:bg-accent-blue-light transition duration-200 ease-out active:duration-75 active:scale-[0.98] hover:scale-105"
					onClick={onSaveWatchPaths}
				>
					Save watch paths
				</button>
				<ActionButton onClick={onToggleAdvanced}>
					{showAdvancedPaths ? "Hide advanced editor" : "Show advanced editor"}
				</ActionButton>
			</div>

			{showAdvancedPaths ? (
				<div className="mt-3 space-y-2 rounded-md border border-border-subtle bg-bg-secondary p-2">
					<label
						htmlFor="claude-paths"
						className="text-xs font-semibold text-text-secondary"
					>
						Claude paths (one per line)
					</label>
					<textarea
						id="claude-paths"
						className="mt-1 w-full rounded-md border border-border-subtle bg-bg-tertiary p-2 text-xs text-text-secondary"
						rows={3}
						value={claudePaths}
						onChange={(event) => onClaudePathsChange(event.target.value)}
					/>
					<label
						htmlFor="cursor-paths"
						className="text-xs font-semibold text-text-secondary"
					>
						Cursor paths (one per line)
					</label>
					<textarea
						id="cursor-paths"
						className="mt-1 w-full rounded-md border border-border-subtle bg-bg-tertiary p-2 text-xs text-text-secondary"
						rows={3}
						value={cursorPaths}
						onChange={(event) => onCursorPathsChange(event.target.value)}
					/>
					<label
						htmlFor="codex-log-paths"
						className="text-xs font-semibold text-text-secondary"
					>
						Codex paths (one per line)
					</label>
					<textarea
						id="codex-log-paths"
						className="mt-1 w-full rounded-md border border-border-subtle bg-bg-tertiary p-2 text-xs text-text-secondary"
						rows={2}
						value={codexPaths}
						onChange={(event) => onCodexPathsChange(event.target.value)}
					/>
				</div>
			) : null}
		</div>
	);
}

function ActionButton(props: {
	children: ReactNode;
	disabled?: boolean;
	onClick: () => void | Promise<void>;
}) {
	const { children, disabled = false, onClick } = props;
	return (
		<button
			type="button"
			className="btn-secondary-soft inline-flex items-center rounded-md px-2 py-1 text-[0.6875rem] font-semibold transition duration-200 ease-out active:duration-75 active:scale-[0.98] hover:scale-105 disabled:opacity-50"
			disabled={disabled}
			onClick={() => void onClick()}
		>
			{children}
		</button>
	);
}
