import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { demoSnapshot, previewSnapshot } from "./demo";
import type { MonitorSettings, MonitorSnapshot } from "./types";

export type MonitorMode = "native" | "preview" | "demo";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useMonitor() {
  const [mode, setMode] = useState<MonitorMode>(() => isTauri() ? "native" : "preview");
  const [snapshot, setSnapshot] = useState<MonitorSnapshot | null>(() => isTauri() ? null : previewSnapshot());
  const [loading, setLoading] = useState(isTauri);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  const busy = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  useEffect(() => {
    if (mode !== "native") return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    async function connect() {
      try {
        const cleanup = await listen<MonitorSnapshot>("monitor-updated", (event) => {
          if (!disposed) setSnapshot(event.payload);
        });
        if (disposed) { cleanup(); return; }
        unlisten = cleanup;
        const current = await invoke<MonitorSnapshot>("get_snapshot");
        if (!disposed) setSnapshot(current);
      } catch (cause) {
        if (!disposed) setError(errorMessage(cause));
      } finally {
        if (!disposed) setLoading(false);
      }
    }
    void connect();
    function onFocus() {
      void invoke<MonitorSnapshot>("get_snapshot")
        .then((current) => { if (!disposed) setSnapshot(current); })
        .catch((cause: unknown) => { if (!disposed) setError(errorMessage(cause)); });
    }
    window.addEventListener("focus", onFocus);
    return () => { disposed = true; unlisten?.(); window.removeEventListener("focus", onFocus); };
  }, [mode]);

  const refresh = useCallback(async () => {
    if (mode !== "native") {
      if (mode === "demo") setSnapshot((current) => demoSnapshot(current?.settings));
      return;
    }
    if (busy.current) return;
    busy.current = true;
    setLoading(true);
    setError(null);
    try {
      const current = await invoke<MonitorSnapshot>("refresh_devices");
      if (mounted.current) setSnapshot(current);
    } catch (cause) {
      if (mounted.current) setError(errorMessage(cause));
    } finally {
      busy.current = false;
      if (mounted.current) setLoading(false);
    }
  }, [mode]);

  const updateSettings = useCallback(async (settings: MonitorSettings) => {
    setError(null);
    if (mode !== "native") {
      setSnapshot((current) => current ? { ...current, settings } : current);
      return;
    }
    try {
      const current = await invoke<MonitorSnapshot>("update_settings", { settings });
      if (mounted.current) setSnapshot(current);
    } catch (cause) {
      if (mounted.current) setError(errorMessage(cause));
    }
  }, [mode]);

  const enableDemo = useCallback(() => {
    if (isTauri()) return;
    setError(null);
    setSnapshot(demoSnapshot());
    setMode("demo");
  }, []);

  const disableDemo = useCallback(() => {
    if (isTauri()) return;
    setSnapshot(previewSnapshot());
    setMode("preview");
  }, []);

  return { snapshot, loading, error, mode, refresh, updateSettings, enableDemo, disableDemo };
}
