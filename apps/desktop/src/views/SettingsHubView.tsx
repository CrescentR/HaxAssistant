import React, { useEffect, useState } from "react";
import { CloudDownload, Eye, Globe, RefreshCw, RotateCcw, Settings, Trash2 } from "lucide-react";
import { DEFAULT_SETTINGS, type LoadedHexBoxDatabase } from "../api/dataService";
import {
  isTauriRuntime,
  subscribeUpdateProgress,
  tauriCheckAppUpdate,
  tauriCleanupOcrCache,
  tauriGetAssetSyncStatus,
  tauriFixGameBorderless,
  tauriGetClientLocale,
  tauriGetGameWindowMode,
  tauriInstallAppUpdate,
  tauriResetOverlayPosition,
  tauriSyncRemoteAssets,
} from "../api/tauriBridge";
import type {
  AppSettings,
  AppUpdateInfo,
  AssetSyncStatus,
  LeagueWindowMode,
} from "../types/hexbox";

interface SettingsHubViewProps {
  db: LoadedHexBoxDatabase;
  settings: AppSettings;
  onUpdateSettings: (patch: Partial<AppSettings>) => void;
  onOpenOverlayPreview: (overlayKind: string) => void;
}

export const SettingsHubView: React.FC<SettingsHubViewProps> = ({
  db,
  settings,
  onUpdateSettings,
  onOpenOverlayPreview,
}) => {
  const [assetStatus, setAssetStatus] = useState<AssetSyncStatus | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [localeText, setLocaleText] = useState<string | null>(null);
  const [maintenanceMsg, setMaintenanceMsg] = useState<string>("");
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [updateBusy, setUpdateBusy] = useState<"" | "checking" | "installing">("");
  const [updateMsg, setUpdateMsg] = useState<string>("");
  const [windowMode, setWindowMode] = useState<LeagueWindowMode | null>(null);
  const [windowModeBusy, setWindowModeBusy] = useState(false);
  const [windowModeMsg, setWindowModeMsg] = useState<string>("");

  const refreshWindowMode = async () => {
    try {
      const mode = await tauriGetGameWindowMode();
      if (mode) setWindowMode(mode);
    } catch {
      // ignore: client not running or browser preview
    }
  };

  useEffect(() => {
    refreshWindowMode();
  }, []);

  const handleFixBorderless = async () => {
    if (!isTauriRuntime()) {
      setWindowModeMsg("当前为浏览器预览模式，请在桌面端修改游戏窗口模式。");
      return;
    }
    setWindowModeBusy(true);
    try {
      const mode = await tauriFixGameBorderless();
      if (mode) {
        setWindowMode(mode);
        setWindowModeMsg("已切换为无边框，下次进入游戏时生效。");
      }
    } catch (err) {
      setWindowModeMsg(`切换失败：${String(err)}`);
    } finally {
      setWindowModeBusy(false);
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    subscribeUpdateProgress((p) => {
      if (p.finished) {
        setUpdateMsg("下载完成，正在安装，安装后会自动重启…");
      } else {
        const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
        setUpdateMsg(
          p.total
            ? `正在下载更新 ${mb(p.downloaded)} / ${mb(p.total)} MB`
            : `正在下载更新 ${mb(p.downloaded)} MB`
        );
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const handleCheckUpdate = async () => {
    if (!isTauriRuntime()) {
      setUpdateMsg("当前为浏览器预览模式，请在桌面端检查更新。");
      return;
    }
    setUpdateBusy("checking");
    setUpdateMsg("正在检查更新…");
    try {
      const info = await tauriCheckAppUpdate();
      setUpdateInfo(info);
      if (info?.available) {
        setUpdateMsg(`发现新版本 ${info.version}（当前 ${info.currentVersion}）`);
      } else if (info) {
        setUpdateMsg(`已经是最新版本（${info.currentVersion}）`);
      }
    } catch (err) {
      setUpdateMsg(String(err));
    } finally {
      setUpdateBusy("");
    }
  };

  const handleInstallUpdate = async () => {
    setUpdateBusy("installing");
    setUpdateMsg("正在下载更新…");
    try {
      await tauriInstallAppUpdate();
    } catch (err) {
      setUpdateMsg(String(err));
      setUpdateBusy("");
    }
  };

  const refreshAssetStatus = async () => {
    try {
      const status = await tauriGetAssetSyncStatus();
      if (status) setAssetStatus(status);
    } catch {
      // ignore in browser preview
    }
  };

  useEffect(() => {
    refreshAssetStatus();
  }, [db]);

  const handleSyncAssets = async () => {
    if (!isTauriRuntime()) {
      setMaintenanceMsg("当前为纯浏览器预览模式，请在桌面端内执行数据更新。");
      return;
    }
    setSyncing(true);
    setMaintenanceMsg("正在检查并同步最新统计数据...");
    try {
      const result = await tauriSyncRemoteAssets(
        settings.data_sync_url.trim() || undefined,
        true
      );
      if (result) {
        setAssetStatus(result.status);
        const dlCount = result.downloadedDataFiles.length + result.downloadedModelFiles.length;
        const failMsg =
          result.failedFiles.length > 0 ? `，${result.failedFiles.length} 项更新失败` : "";
        setMaintenanceMsg(
          dlCount > 0
            ? `数据已更新至 Patch ${result.patch}（更新 ${dlCount} 项，已是最新 ${result.skippedFiles} 项${failMsg}）。`
            : result.failedFiles.length > 0
              ? `同步未完成（Patch ${result.patch}，已校验 ${result.skippedFiles} 项${failMsg}）。`
              : `当前已是最新数据（Patch ${result.patch}，已校验 ${result.skippedFiles} 项）。`
        );
      }
    } catch (err) {
      setMaintenanceMsg(`数据更新失败：${String(err)}`);
    } finally {
      setSyncing(false);
    }
  };

  const handleCleanupCache = async () => {
    if (!isTauriRuntime()) {
      setMaintenanceMsg("当前为纯浏览器预览模式，无本地 OCR 缓存目录。");
      return;
    }
    setCleaning(true);
    try {
      const report = await tauriCleanupOcrCache();
      if (report) {
        const kb = (report.freedBytes / 1024).toFixed(1);
        setMaintenanceMsg(
          `OCR 缓存清理完成：移除 ${report.removedFiles} 个临时文件，释放 ${kb} KB。`
        );
      }
    } catch (err) {
      setMaintenanceMsg(`清理缓存失败：${String(err)}`);
    } finally {
      setCleaning(false);
    }
  };

  const handleDetectLocale = async () => {
    if (!isTauriRuntime()) {
      setLocaleText("zh_CN (预览默认)");
      setMaintenanceMsg("当前为浏览器预览模式，默认使用中文识别模型 (ch)。");
      return;
    }
    try {
      const loc = await tauriGetClientLocale();
      if (loc) {
        setLocaleText(loc);
        setMaintenanceMsg(`已检测到客户端语言：${loc}，OCR 侧车将按语言自动切换模型。`);
      }
    } catch (err) {
      setMaintenanceMsg(`检测客户端语言失败：${String(err)}`);
    }
  };

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <Settings size={18} className="gold-text" /> HaxAssistant 设置中心
            </h3>
            <div className="card-sub">
              本地持久化保存快捷键、OCR 推理档位、悬浮窗透明度、召唤师技能与选人自动化开关
            </div>
          </div>
          <button
            className="hex-btn ghost sm"
            onClick={() => onUpdateSettings(DEFAULT_SETTINGS)}
          >
            <RotateCcw size={13} /> 恢复默认设置
          </button>
        </div>

        <div className="settings-grid">
          <div className="settings-section">
            <h4>OCR 与推荐算法</h4>
            <label className="setting-row">
              <span>启用局内自动 OCR 监听</span>
              <input
                type="checkbox"
                checked={settings.auto_ocr_enabled}
                onChange={(e) => onUpdateSettings({ auto_ocr_enabled: e.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>非游戏前台也允许自动 OCR（调试/多屏测试）</span>
              <input
                type="checkbox"
                checked={settings.allow_non_lol_foreground}
                onChange={(e) => onUpdateSettings({ allow_non_lol_foreground: e.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>OCR 性能模式</span>
              <select
                className="hex-select sm"
                value={settings.ocr_performance_mode}
                onChange={(e) =>
                  onUpdateSettings({
                    ocr_performance_mode: e.target.value as AppSettings["ocr_performance_mode"],
                  })
                }
              >
                <option value="eco">节能模式 (2 线程)</option>
                <option value="balanced">均衡模式 (4 线程)</option>
                <option value="performance">高性能模式 (6 线程)</option>
              </select>
            </label>
            <label className="setting-row">
              <span>默认分辨率坐标预设</span>
              <select
                className="hex-select sm"
                value={settings.selected_preset_key}
                onChange={(e) => onUpdateSettings({ selected_preset_key: e.target.value })}
              >
                {db.presets.length > 0 ? (
                  db.presets.map((p) => (
                    <option key={p.key} value={p.key}>
                      {p.key}
                    </option>
                  ))
                ) : (
                  <option value="1920x1080@100">1920x1080@100</option>
                )}
              </select>
            </label>
            <label className="setting-row">
              <span>海克斯强化价值模式</span>
              <select
                className="hex-select sm"
                value={settings.augment_value_mode || "balanced"}
                onChange={(e) => onUpdateSettings({ augment_value_mode: e.target.value })}
              >
                <option value="balanced">均衡综合 (默认)</option>
                <option value="winrate">胜率优先 (放大净胜率差)</option>
                <option value="highroll">上限联动 (偏好棱彩与机制联动)</option>
                <option value="stable">稳健高样本 (惩罚冷门小样本)</option>
              </select>
            </label>
            <label className="setting-row">
              <span>强化识别热键</span>
              <input
                className="hex-input sm"
                value={settings.augment_hotkey}
                onChange={(e) => onUpdateSettings({ augment_hotkey: e.target.value })}
              />
            </label>
            <label className="setting-row">
              <span>流派出装热键</span>
              <input
                className="hex-input sm"
                value={settings.item_overlay_hotkey}
                onChange={(e) => onUpdateSettings({ item_overlay_hotkey: e.target.value })}
              />
            </label>
          </div>

          <div className="settings-section">
            <h4>悬浮窗、选人与对局自动化</h4>
            <label className="setting-row">
              <span>自动弹出海克斯强化悬浮窗</span>
              <input
                type="checkbox"
                checked={settings.auto_show_overlay}
                onChange={(e) => onUpdateSettings({ auto_show_overlay: e.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>自动弹出公式/克制出装悬浮窗</span>
              <input
                type="checkbox"
                checked={settings.auto_show_formula_item_overlay}
                onChange={(e) =>
                  onUpdateSettings({ auto_show_formula_item_overlay: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>启用 10 人战力条悬浮窗</span>
              <input
                type="checkbox"
                checked={settings.player_scores_overlay_enabled}
                onChange={(e) =>
                  onUpdateSettings({ player_scores_overlay_enabled: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>启用 SGP 跨区战绩与 10 人情报查询</span>
              <input
                type="checkbox"
                checked={settings.sgp_enabled}
                onChange={(e) => onUpdateSettings({ sgp_enabled: e.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>启用阵亡购物提示</span>
              <input
                type="checkbox"
                checked={settings.death_shop_hint_enabled}
                onChange={(e) =>
                  onUpdateSettings({ death_shop_hint_enabled: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>选人自动接受对局 (Ready Check)</span>
              <input
                type="checkbox"
                checked={settings.ready_check_auto_accept}
                onChange={(e) =>
                  onUpdateSettings({ ready_check_auto_accept: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>自动接受延迟</span>
              <select
                className="hex-select sm"
                value={settings.ready_check_accept_delay_ms ?? 0}
                onChange={(e) =>
                  onUpdateSettings({ ready_check_accept_delay_ms: Number(e.target.value) })
                }
              >
                <option value={0}>立即接受 (0s)</option>
                <option value={1000}>延迟 1 秒</option>
                <option value={2000}>延迟 2 秒</option>
                <option value={3000}>延迟 3 秒</option>
                <option value={5000}>延迟 5 秒</option>
              </select>
            </label>
            <label className="setting-row">
              <span>替补席自动秒抢</span>
              <input
                type="checkbox"
                checked={settings.bench_autograb_enabled}
                onChange={(e) =>
                  onUpdateSettings({ bench_autograb_enabled: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>闪现按键位置偏好</span>
              <select
                className="hex-select sm"
                value={settings.summoner_flash_slot || "F"}
                onChange={(e) => onUpdateSettings({ summoner_flash_slot: e.target.value })}
              >
                <option value="F">F 键闪现（D 键雪球/屏障/净化）</option>
                <option value="D">D 键闪现（F 键雪球/屏障/净化）</option>
              </select>
            </label>
            <label className="setting-row">
              <span>选人英雄变动时自动应用推荐召唤师技能</span>
              <input
                type="checkbox"
                checked={settings.auto_apply_summoner_spells}
                onChange={(e) =>
                  onUpdateSettings({ auto_apply_summoner_spells: e.target.checked })
                }
              />
            </label>
            <label className="setting-row">
              <span>悬浮窗不透明度 ({settings.overlay_opacity}%)</span>
              <input
                type="range"
                min={50}
                max={100}
                value={settings.overlay_opacity}
                onChange={(e) => onUpdateSettings({ overlay_opacity: Number(e.target.value) })}
              />
            </label>
            <label className="setting-row">
              <span>悬浮窗缩放 ({settings.overlay_zoom}%)</span>
              <input
                type="range"
                min={75}
                max={130}
                value={settings.overlay_zoom}
                onChange={(e) => onUpdateSettings({ overlay_zoom: Number(e.target.value) })}
              />
            </label>
            <div className="setting-row">
              <span>悬浮窗位置（拖动标题栏可移动，位置会被记住）</span>
              <button
                className="hex-btn ghost sm"
                onClick={() => tauriResetOverlayPosition().catch(() => {})}
              >
                全部重置
              </button>
            </div>
            <div className="setting-row">
              <span>
                游戏窗口模式：
                {windowMode?.connected
                  ? windowMode.label
                  : "未连接客户端"}
                {windowMode?.connected && !windowMode.isBorderless
                  ? "（全屏下悬浮窗不可见）"
                  : ""}
              </span>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  className="hex-btn ghost sm"
                  onClick={refreshWindowMode}
                  title="刷新"
                >
                  <RefreshCw size={13} />
                </button>
                <button
                  className="hex-btn primary sm"
                  disabled={
                    windowModeBusy || !windowMode?.connected || windowMode.isBorderless
                  }
                  onClick={handleFixBorderless}
                >
                  {windowModeBusy ? "切换中…" : "一键切换无边框"}
                </button>
              </div>
            </div>
            {windowModeMsg && <div className="card-sub">{windowModeMsg}</div>}
            <label className="setting-row">
              <span>进入选人时自动切换为无边框</span>
              <input
                type="checkbox"
                checked={settings.auto_fix_borderless}
                onChange={(e) =>
                  onUpdateSettings({ auto_fix_borderless: e.target.checked })
                }
              />
            </label>
          </div>
        </div>

        <div className="overlay-preview-strip">
          <span className="sim-label">快速预览 5 种悬浮窗：</span>
          {(
            [
              ["augment", "强化三选一"],
              ["item", "流派与克制出装"],
              ["champ-select", "选人阵容/替补"],
              ["player-scores", "10 人战力条"],
              ["death-hint", "阵亡购物提示"],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              className="hex-btn ghost sm"
              onClick={() => onOpenOverlayPreview(key)}
            >
              <Eye size={13} /> {label}
            </button>
          ))}
        </div>
      </div>

      {/* Data & Component Maintenance */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <CloudDownload size={17} className="gold-text" /> 数据与组件维护
            </h3>
            <div className="card-sub">
              一键更新最新英雄与海克斯统计数据、清理本地识别缓存与检测客户端语言
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <span className={`tag-badge ${assetStatus?.readyForOffline === false ? "warn" : "success"}`}>
              数据 Patch {db.patch}
            </span>
            {assetStatus && (
              <span className={`tag-badge ${assetStatus.readyForOcr ? "success" : "warn"}`}>
                {assetStatus.readyForOcr ? "识别组件就绪" : "识别组件待补齐"}
              </span>
            )}
            {localeText && <span className="tag-badge">语言：{localeText}</span>}
          </div>
        </div>

        <div className="settings-section" style={{ marginTop: 8 }}>
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", marginTop: 4 }}>
            <button
              className="hex-btn primary sm"
              onClick={handleSyncAssets}
              disabled={syncing}
            >
              <CloudDownload size={14} /> {syncing ? "正在更新..." : "检查并更新数据"}
            </button>
            <button
              className="hex-btn ghost sm"
              onClick={handleCleanupCache}
              disabled={cleaning}
            >
              <Trash2 size={14} /> {cleaning ? "清理中..." : "清理 OCR 缓存"}
            </button>
            <button className="hex-btn ghost sm" onClick={handleDetectLocale}>
              <Globe size={14} /> 检测客户端语言
            </button>
          </div>
          {maintenanceMsg && (
            <div className="card-sub" style={{ marginTop: 10 }}>
              {maintenanceMsg}
            </div>
          )}
        </div>
      </div>

      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <RefreshCw size={17} className="gold-text" /> 应用更新
            </h3>
            <div className="card-sub">
              检查并安装客户端新版本，安装包经签名校验后自动更新并重启
            </div>
          </div>
          {updateInfo && (
            <span className={`tag-badge ${updateInfo.available ? "warn" : "success"}`}>
              {updateInfo.available ? `可更新到 ${updateInfo.version}` : `当前 ${updateInfo.currentVersion}`}
            </span>
          )}
        </div>
        <div style={{ padding: "0 4px" }}>
          <label className="setting-row">
            <span>点击主窗口 × 时</span>
            <select
              className="hex-select sm"
              value={settings.close_button_action || "ask"}
              onChange={(e) => onUpdateSettings({ close_button_action: e.target.value })}
            >
              <option value="ask">每次询问</option>
              <option value="tray">最小化到托盘</option>
              <option value="exit">直接退出</option>
            </select>
          </label>
          <label className="setting-row">
            <span>启动时自动检查更新</span>
            <input
              type="checkbox"
              checked={settings.auto_check_update}
              onChange={(e) => onUpdateSettings({ auto_check_update: e.target.checked })}
            />
          </label>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
            <button
              className="hex-btn ghost sm"
              onClick={handleCheckUpdate}
              disabled={updateBusy !== ""}
            >
              <RefreshCw size={14} /> {updateBusy === "checking" ? "检查中..." : "检查更新"}
            </button>
            {updateInfo?.available && (
              <button
                className="hex-btn primary sm"
                onClick={handleInstallUpdate}
                disabled={updateBusy !== ""}
              >
                <CloudDownload size={14} />{" "}
                {updateBusy === "installing" ? "正在更新..." : `下载并安装 ${updateInfo.version}`}
              </button>
            )}
          </div>
          {updateInfo?.available && updateInfo.notes && (
            <div className="card-sub" style={{ marginTop: 10, whiteSpace: "pre-wrap" }}>
              {updateInfo.notes}
            </div>
          )}
          {updateMsg && (
            <div className="card-sub" style={{ marginTop: 10 }}>
              {updateMsg}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
