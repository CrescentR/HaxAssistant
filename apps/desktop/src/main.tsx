import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import {
  Activity,
  BarChart3,
  BookOpen,
  History,
  Settings,
  Sparkles,
  Sun,
  Moon,
  Users,
  X,
} from "lucide-react";
import {
  loadDatabase,
  loadSettings,
  saveSettings,
  type LoadedHexBoxDatabase,
} from "./api/dataService";
import {
  isTauriRuntime,
  subscribeLcuStatus,
  subscribeOverlayState,
  subscribeSettingsUpdated,
  tauriGetOverlayState,
  tauriGetSettings,
  tauriProbeLcuStatus,
  tauriToggleOverlay,
  tauriUpdateSettings,
} from "./api/tauriBridge";
import type { AppSettings, LcuStatus, OverlayStatePayload } from "./types/hexbox";
import { OngoingWorkspace } from "./views/OngoingWorkspace";
import { HexdataDataView } from "./views/HexdataDataView";
import { RecommendView } from "./views/RecommendView";
import { KaiheiView } from "./views/KaiheiView";
import { PersonalHistoryView } from "./views/PersonalHistoryView";
import { SettingsHubView } from "./views/SettingsHubView";
import { OverlayApp, type OverlayKind } from "./overlays/OverlayApp";
import "./styles/replica.css";

type NavKey = "ongoing" | "hexdata" | "recommend" | "kaihei" | "history" | "settings";

const App: React.FC = () => {
  const [db, setDb] = useState<LoadedHexBoxDatabase | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<NavKey>("ongoing");
  const [selectedHero, setSelectedHero] = useState<string>("寒冰射手");
  const [settings, setSettings] = useState<AppSettings>(() => loadSettings());
  const [overlayPreview, setOverlayPreview] = useState<OverlayKind | null>(null);
  const [lcuStatus, setLcuStatus] = useState<LcuStatus>({
    connected: false,
    phase: "Disconnected",
    port: null,
    summoner_name: null,
    lockfile_path: null,
  });
  const [liveOverlayState, setLiveOverlayState] = useState<OverlayStatePayload | null>(null);

  useEffect(() => {
    loadDatabase()
      .then((loaded) => {
        setDb(loaded);
        if (loaded.champions.length > 0 && !loaded.champions.some((c) => c.hero === selectedHero)) {
          setSelectedHero(loaded.champions[0].hero);
        }
      })
      .catch((err) => setLoadError(String(err)));
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let active = true;
    tauriGetSettings()
      .then((remote) => {
        if (active && remote) {
          saveSettings(remote);
          setSettings(remote);
        }
      })
      .catch(() => {});
    tauriProbeLcuStatus()
      .then((status) => {
        if (active && status) setLcuStatus(status);
      })
      .catch(() => {});
    tauriGetOverlayState()
      .then((state) => {
        if (active && state) setLiveOverlayState(state);
      })
      .catch(() => {});

    const unlisteners: Array<Promise<() => void>> = [
      subscribeLcuStatus((status) => {
        if (active) setLcuStatus(status);
      }),
      subscribeOverlayState((state) => {
        if (active) setLiveOverlayState(state);
      }),
      subscribeSettingsUpdated((remote) => {
        if (active) {
          saveSettings(remote);
          setSettings(remote);
        }
      }),
    ];

    return () => {
      active = false;
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", settings.appearance_mode);
  }, [settings.appearance_mode]);

  const handleUpdateSettings = (patch: Partial<AppSettings>) => {
    const next = saveSettings(patch);
    setSettings(next);
    tauriUpdateSettings(next).catch(() => {});
  };

  const handleOpenHeroRecommend = (heroName: string) => {
    setSelectedHero(heroName);
    setActiveTab("recommend");
  };

  const handleOpenOverlayPreview = (kind: string) => {
    const valid: OverlayKind[] = [
      "augment",
      "item",
      "champ-select",
      "player-scores",
      "death-hint",
    ];
    const resolved: OverlayKind = valid.includes(kind as OverlayKind)
      ? (kind as OverlayKind)
      : "augment";
    if (isTauriRuntime()) {
      tauriToggleOverlay(resolved).catch(() => setOverlayPreview(resolved));
      return;
    }
    setOverlayPreview(resolved);
  };

  const navItems: Array<{ key: NavKey; label: string; icon: React.ReactNode }> = [
    { key: "ongoing", label: "对局工作台", icon: <Activity size={17} /> },
    { key: "hexdata", label: "全量数据库", icon: <BarChart3 size={17} /> },
    { key: "recommend", label: "英雄详情库", icon: <BookOpen size={17} /> },
    { key: "kaihei", label: "开黑与标记", icon: <Users size={17} /> },
    { key: "history", label: "个人战绩", icon: <History size={17} /> },
    { key: "settings", label: "设置中心", icon: <Settings size={17} /> },
  ];

  return (
    <div className="app-shell">
      <aside className="app-sidebar">
        <div className="brand-block">
          <img src="/hexbox-logo.png" alt="HexBox" className="brand-logo" />
          <div>
            <div className="brand-name">HexBox</div>
            <div className="brand-sub">海克斯大乱斗助手</div>
          </div>
        </div>

        <nav className="sidebar-nav">
          {navItems.map((item) => (
            <button
              key={item.key}
              className={`nav-item ${activeTab === item.key ? "active" : ""}`}
              onClick={() => setActiveTab(item.key)}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <button
            className="hex-btn ghost sm full-width"
            onClick={() =>
              handleUpdateSettings({
                appearance_mode: settings.appearance_mode === "dark" ? "light" : "dark",
              })
            }
          >
            {settings.appearance_mode === "dark" ? <Sun size={14} /> : <Moon size={14} />}
            {settings.appearance_mode === "dark" ? "切换浅色模式" : "切换深色模式"}
          </button>
          <div className="sidebar-meta">
            <Sparkles size={12} className="gold-text" />
            <span>
              Patch {db?.patch || "16.22"} ·{" "}
              {isTauriRuntime()
                ? lcuStatus.connected
                  ? `LCU: ${lcuStatus.phase}`
                  : "LCU 未连接"
                : "Web 预览"}
            </span>
          </div>
        </div>
      </aside>

      <main className="app-main">
        {loadError && (
          <div className="panel-card error-card">
            数据加载失败：{loadError}
          </div>
        )}

        {!db && !loadError && (
          <div className="panel-card loading-card">正在加载海克斯全量数据库...</div>
        )}

        {db && activeTab === "ongoing" && (
          <OngoingWorkspace
            db={db}
            settings={settings}
            lcuStatus={lcuStatus}
            liveOverlayState={liveOverlayState}
            onOpenHeroRecommend={handleOpenHeroRecommend}
            onOpenOverlayPreview={handleOpenOverlayPreview}
          />
        )}
        {db && activeTab === "hexdata" && (
          <HexdataDataView db={db} onSelectHero={handleOpenHeroRecommend} />
        )}
        {db && activeTab === "recommend" && (
          <RecommendView
            db={db}
            heroName={selectedHero}
            onSelectHero={setSelectedHero}
          />
        )}
        {db && activeTab === "kaihei" && <KaiheiView />}
        {db && activeTab === "history" && (
          <PersonalHistoryView db={db} onSelectHero={handleOpenHeroRecommend} />
        )}
        {db && activeTab === "settings" && (
          <SettingsHubView
            db={db}
            settings={settings}
            onUpdateSettings={handleUpdateSettings}
            onOpenOverlayPreview={handleOpenOverlayPreview}
          />
        )}
      </main>

      {overlayPreview && db && (
        <div className="overlay-modal-backdrop" onClick={() => setOverlayPreview(null)}>
          <div className="overlay-modal-shell" onClick={(e) => e.stopPropagation()}>
            <div className="overlay-modal-bar">
              <span>悬浮窗实时预览 · {overlayPreview}</span>
              <div className="overlay-modal-actions">
                <a
                  href={`/overlay.html?overlay=${overlayPreview}`}
                  target="_blank"
                  rel="noreferrer"
                  className="hex-btn ghost sm"
                >
                  独立窗口打开
                </a>
                <button className="hex-btn ghost sm" onClick={() => setOverlayPreview(null)}>
                  <X size={14} /> 关闭
                </button>
              </div>
            </div>
            <div className="overlay-modal-content">
              <OverlayApp
                kind={overlayPreview}
                db={db}
                liveState={liveOverlayState ?? undefined}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
