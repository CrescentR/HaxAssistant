import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { loadDatabase, loadSettings, type LoadedHexBoxDatabase } from "./api/dataService";
import {
  isTauriRuntime,
  subscribeOverlayState,
  subscribeSettingsUpdated,
  tauriGetOverlayState,
  tauriGetSettings,
} from "./api/tauriBridge";
import type { AppSettings, OverlayStatePayload } from "./types/hexbox";
import { OverlayApp, type OverlayKind } from "./overlays/OverlayApp";
import "./styles/replica.css";

function resolveOverlayKind(): OverlayKind {
  const params = new URLSearchParams(window.location.search);
  const raw = params.get("overlay") || "augment";
  const allowed: OverlayKind[] = [
    "augment",
    "item",
    "champ-select",
    "player-scores",
    "death-hint",
  ];
  return allowed.includes(raw as OverlayKind) ? (raw as OverlayKind) : "augment";
}

const OverlayEntry: React.FC = () => {
  const [db, setDb] = useState<LoadedHexBoxDatabase | null>(null);
  const [liveState, setLiveState] = useState<OverlayStatePayload | null>(null);
  const [settings, setSettings] = useState<AppSettings>(() => loadSettings());
  const kind = resolveOverlayKind();

  useEffect(() => {
    loadDatabase().then(setDb).catch(() => {});
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let active = true;
    tauriGetOverlayState()
      .then((st) => {
        if (active && st) setLiveState(st);
      })
      .catch(() => {});
    tauriGetSettings()
      .then((s) => {
        if (active && s) setSettings(s);
      })
      .catch(() => {});

    const unlisteners = [
      subscribeOverlayState((st) => {
        if (active) setLiveState(st);
      }),
      subscribeSettingsUpdated((s) => {
        if (active) setSettings(s);
      }),
    ];

    return () => {
      active = false;
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  if (!db) {
    return <div className="overlay-card">正在初始化悬浮窗数据...</div>;
  }

  return (
    <div
      style={{
        opacity: Math.max(0.5, Math.min(1, (settings.overlay_opacity || 95) / 100)),
        zoom: `${settings.overlay_zoom || 100}%`,
      }}
    >
      <OverlayApp kind={kind} db={db} liveState={liveState ?? undefined} />
    </div>
  );
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <OverlayEntry />
  </React.StrictMode>
);
