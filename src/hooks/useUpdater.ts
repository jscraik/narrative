import { useCallback, useEffect, useState } from 'react';
import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'up-to-date'
  | 'error';

export type UpdateProgress = {
  downloaded: number;
  total?: number;
  percent?: number;
};

export type UpdateState = {
  status: UpdateStatus;
  update?: Update;
  progress?: UpdateProgress;
  error?: string;
};

const MAX_PERCENT = 100;

export function useUpdater() {
  const [state, setState] = useState<UpdateState>({ status: 'idle' });

  const reset = useCallback(() => {
    setState({ status: 'idle' });
  }, []);

  const checkForUpdates = useCallback(
    async (silent = false): Promise<Update | null> => {
      setState({ status: 'checking' });
      try {
        const update = await check();
        if (update) {
          setState({ status: 'available', update });
          return update;
        }
        if (silent) {
          setState({ status: 'idle' });
        } else {
          setState({ status: 'up-to-date' });
        }
        return null;
      } catch (error) {
        if (silent) {
          setState({ status: 'idle' });
        } else {
          setState({
            status: 'error',
            error: error instanceof Error ? error.message : String(error)
          });
        }
        return null;
      }
    },
    []
  );

  const installUpdate = useCallback(async () => {
    const update = state.update;
    if (!update || state.status !== 'available') return;

    let downloaded = 0;
    let total: number | undefined;

    setState((prev) => ({ ...prev, status: 'downloading', progress: { downloaded: 0, total, percent: 0 } }));

    const handleDownloadEvent = (event: DownloadEvent) => {
      if (event.event === 'Started') {
        total = event.data.contentLength;
        setState((prev) => ({
          ...prev,
          status: 'downloading',
          progress: {
            downloaded: 0,
            total,
            percent: total ? 0 : undefined
          }
        }));
      }

      if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        const percent = total ? Math.min(MAX_PERCENT, Math.round((downloaded / total) * MAX_PERCENT)) : undefined;
        setState((prev) => ({
          ...prev,
          status: 'downloading',
          progress: {
            downloaded,
            total,
            percent
          }
        }));
      }

      if (event.event === 'Finished') {
        setState((prev) => ({
          ...prev,
          status: 'installing'
        }));
      }
    };

    try {
      await update.downloadAndInstall(handleDownloadEvent);
      await relaunch();
    } catch (error) {
      setState((prev) => ({
        ...prev,
        status: 'error',
        error: error instanceof Error ? error.message : String(error)
      }));
    }
  }, [state.update, state.status]);

  useEffect(() => {
    void checkForUpdates(true);
  }, [checkForUpdates]);

  useEffect(() => {
    if (state.status !== 'up-to-date' && state.status !== 'error') return;
    const timer = window.setTimeout(() => {
      setState({ status: 'idle' });
    }, 3500);
    return () => window.clearTimeout(timer);
  }, [state.status]);

  return {
    updateState: state,
    checkForUpdates,
    installUpdate,
    resetUpdateState: reset
  };
}
