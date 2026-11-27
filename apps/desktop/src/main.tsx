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
  subscribeCloseRequested,
  subscribeDatasetUpdated,
  subscribeLcuStatus,
  subscribeOverlayState,
  subscribeSettingsUpdated,
  subscribeUpdateAvailable,
  tauriGetOverlayState,
  tauriGetSettings,
  tauriHandleMainClose,
  tauriProbeLcuStatus,
  tauriToggleOverlay,
  tauriUpdateSettings,
} from "./api/tauriBridge";
import type {
  AppSettings,
  AppUpdateInfo,
  LcuStatus,
  OverlayStatePayload,
} from "./types/hexbox";
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
    client_running: false,
    connected: false,
    phase: "Disconnected",
    port: null,
    summoner_name: null,
    lockfile_path: null,
  });
  const [liveOverlayState, setLiveOverlayState] = useState<OverlayStatePayload | null>(null);
  const [closePromptOpen, setClosePromptOpen] = useState(false);
  const [rememberClose, setRememberClose] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<AppUpdateInfo | null>(null);

  // Single entry point for every database refresh (startup, background update,
  // manual sync) so the selected hero and the error banner follow the data shown.
  const applyDatabase = (fresh: LoadedHexBoxDatabase) => {
    setDb(fresh);
    setLoadError(null);
    setSelectedHero((current) =>
      fresh.champions.some((c) => c.hero === current) ? current : fresh.champions[0].hero
    );
  };

  useEffect(() => {
    let active = true;
    const refresh = (force: boolean) => {
      loadDatabase(force)
        .then((fresh) => {
          if (active) applyDatabase(fresh);
        })
        .catch((err) => {
          if (active) setLoadError(err instanceof Error ? err.message : String(err));
        });
    };
    refresh(false);
    const unlisten = subscribeDatasetUpdated(() => refresh(true));
    return () => {
      active = false;
      unlisten.then((fn) => fn()).catch(() => {});
    };
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
      subscribeCloseRequested(() => {
        if (active) setClosePromptOpen(true);
      }),
      subscribeUpdateAvailable((info) => {
        if (active) setPendingUpdate(info);
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

  const handleCloseChoice = (action: "tray" | "exit") => {
    setClosePromptOpen(false);
    tauriHandleMainClose(action, rememberClose).catch(() => {});
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
          <img src="/app-logo.png" alt="HaxAssistant" className="brand-logo" />
          <div>
            <div className="brand-name">HaxAssistant</div>
            <div className="brand-sub">海克斯助手</div>
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
                  : lcuStatus.client_running ? "已检测到客户端 · LCU 未连接" : "LCU 未连接"
                : "Web 预览"}
            </span>
          </div>
        </div>
      </aside>

      <main className="app-main">
        {pendingUpdate?.available && (
          <div className="panel-card update-banner">
            <span>
              发现新版本 {pendingUpdate.version}（当前 {pendingUpdate.currentVersion}）
            </span>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                className="hex-btn primary sm"
                onClick={() => {
                  setPendingUpdate(null);
                  setActiveTab("settings");
                }}
              >
                查看并更新
              </button>
              <button className="hex-btn ghost sm" onClick={() => setPendingUpdate(null)}>
                稍后
              </button>
            </div>
          </div>
        )}

        {loadError && (
          <div className="panel-card error-card">
            {db ? "数据刷新失败，继续使用已加载的数据：" : "数据加载失败："}
            {loadError}
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

      {closePromptOpen && (
        <div className="overlay-modal-backdrop" onClick={() => setClosePromptOpen(false)}>
          <div className="panel-card close-prompt" onClick={(e) => e.stopPropagation()}>
            <h3>关闭 HaxAssistant</h3>
            <div className="card-sub">
              最小化到托盘后，悬浮窗、自动接受对局和 OCR 会继续工作；点托盘图标可以重新打开主窗口。
            </div>
            <label className="setting-row" style={{ marginTop: 12 }}>
              <span>记住我的选择（可在设置中心修改）</span>
              <input
                type="checkbox"
                checked={rememberClose}
                onChange={(e) => setRememberClose(e.target.checked)}
              />
            </label>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 12 }}>
              <button className="hex-btn ghost sm" onClick={() => setClosePromptOpen(false)}>
                取消
              </button>
              <button className="hex-btn ghost sm" onClick={() => handleCloseChoice("exit")}>
                退出程序
              </button>
              <button className="hex-btn primary sm" onClick={() => handleCloseChoice("tray")}>
                最小化到托盘
              </button>
            </div>
          </div>
        </div>
      )}

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
                valueMode={settings.augment_value_mode}
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
