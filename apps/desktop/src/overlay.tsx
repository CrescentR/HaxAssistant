import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { loadDatabase, loadSettings, type LoadedHexBoxDatabase } from "./api/dataService";
import {
  isTauriRuntime,
  subscribeDatasetUpdated,
  subscribeOverlayState,
  subscribeSettingsUpdated,
  subscribeShopHint,
  tauriGetOverlayState,
  tauriGetSettings,
  tauriResetOverlayPosition,
  tauriResetOverlayZoom,
  tauriStartOverlayDrag,
  tauriToggleShopHint,
  tauriZoomOverlay,
} from "./api/tauriBridge";
import { ShoppingBag } from "lucide-react";
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
    "item-launcher",
  ];
  return allowed.includes(raw as OverlayKind) ? (raw as OverlayKind) : "augment";
}

/** Pointer travel (px) that turns a press on the launcher into a drag. */
const DRAG_THRESHOLD = 4;

/**
 * The in-game shopping button. Its window is exactly the button, so it never
 * covers the game beyond it; a click toggles the panel, a drag moves it.
 */
const ItemLauncher: React.FC<{ settings: AppSettings }> = ({ settings }) => {
  const [panelOpen, setPanelOpen] = useState(false);
  const press = useRef<{ x: number; y: number; dragged: boolean } | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = subscribeShopHint((v) => {
      if (active) setPanelOpen(v.panel);
    });
    return () => {
      active = false;
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const label = panelOpen ? "收起出装推荐" : "打开出装推荐";
  return (
    <button
      type="button"
      className={`overlay-launcher ${panelOpen ? "open" : ""}`}
      title={label}
      aria-label={label}
      aria-pressed={panelOpen}
      style={{
        opacity: Math.max(0.5, Math.min(1, (settings.overlay_opacity || 95) / 100)),
        zoom: `${settings.overlay_zoom || 100}%`,
      }}
      onPointerDown={(e) => {
        if (e.button === 0) press.current = { x: e.screenX, y: e.screenY, dragged: false };
      }}
      onPointerMove={(e) => {
        const p = press.current;
        if (!p || p.dragged || (e.buttons & 1) === 0) return;
        if (Math.hypot(e.screenX - p.x, e.screenY - p.y) >= DRAG_THRESHOLD) {
          p.dragged = true;
          tauriStartOverlayDrag().catch(() => {});
        }
      }}
      onClick={() => {
        const dragged = press.current?.dragged;
        press.current = null;
        if (dragged) return;
        tauriToggleShopHint()
          .then((v) => {
            if (v) setPanelOpen(v.panel);
          })
          .catch(() => {});
      }}
    >
      <ShoppingBag size={20} aria-hidden="true" />
    </button>
  );
};

const OverlayEntry: React.FC = () => {
  const [db, setDb] = useState<LoadedHexBoxDatabase | null>(null);
  const [liveState, setLiveState] = useState<OverlayStatePayload | null>(null);
  const [settings, setSettings] = useState<AppSettings>(() => loadSettings());
  const kind = resolveOverlayKind();

  useEffect(() => {
    // The launcher is only a button; it needs no dataset.
    if (kind === "item-launcher") return;
    let active = true;
    const refresh = (force: boolean) => {
      // A failed reload keeps whatever the overlay is already showing.
      loadDatabase(force)
        .then((fresh) => {
          if (active) setDb(fresh);
        })
        .catch(() => {});
    };
    refresh(false);
    const unlisten = subscribeDatasetUpdated(() => refresh(true));
    return () => {
      active = false;
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [kind]);

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

  // Dragging the header moves the frameless window; the host remembers where it ends up.
  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button") || !target.closest(".overlay-header")) return;
    tauriStartOverlayDrag().catch(() => {});
  };

  if (kind === "item-launcher") {
    return <ItemLauncher settings={settings} />;
  }

  if (!db) {
    return <div className="overlay-card">正在初始化悬浮窗数据...</div>;
  }

  return (
    <div
      className="overlay-shell"
      onMouseDown={handleMouseDown}
      style={{
        opacity: Math.max(0.5, Math.min(1, (settings.overlay_opacity || 95) / 100)),
        zoom: `${settings.overlay_zoom || 100}%`,
      }}
    >
      <div className="overlay-tools">
        <button title="缩小" onClick={() => tauriZoomOverlay(-1).catch(() => {})}>
          −
        </button>
        <button title="恢复默认大小" onClick={() => tauriResetOverlayZoom().catch(() => {})}>
          {settings.overlay_zoom || 100}%
        </button>
        <button title="放大" onClick={() => tauriZoomOverlay(1).catch(() => {})}>
          +
        </button>
        <button title="重置位置" onClick={() => tauriResetOverlayPosition(kind).catch(() => {})}>
          ⌖
        </button>
      </div>
      <OverlayApp kind={kind} db={db} liveState={liveState ?? undefined} valueMode={settings.augment_value_mode} />
    </div>
  );
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <OverlayEntry />
  </React.StrictMode>
);
