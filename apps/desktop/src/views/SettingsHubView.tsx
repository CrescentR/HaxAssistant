import React, { useEffect, useState } from "react";
import { CloudDownload, Eye, Globe, RotateCcw, Settings, Trash2 } from "lucide-react";
import { DEFAULT_SETTINGS, loadDatabase, type LoadedHexBoxDatabase } from "../api/dataService";
import {
  isTauriRuntime,
  tauriCleanupOcrCache,
  tauriGetAssetSyncStatus,
  tauriGetClientLocale,
  tauriResetOverlayPosition,
  tauriSyncRemoteAssets,
} from "../api/tauriBridge";
import type { AppSettings, AssetSyncStatus } from "../types/hexbox";

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
  }, []);

  const handleSyncAssets = async () => {
    if (!isTauriRuntime()) {
      setMaintenanceMsg("当前为纯浏览器预览模式，请在桌面端内执行远程数据/模型同步。");
      return;
    }
    setSyncing(true);
    setMaintenanceMsg("正在拉取远程数据与 ONNX 模型并校验 SHA-256...");
    try {
      const result = await tauriSyncRemoteAssets(
        settings.data_sync_url.trim() || undefined,
        true
      );
      if (result) {
        setAssetStatus(result.status);
        await loadDatabase(true);
        const dlCount = result.downloadedDataFiles.length + result.downloadedModelFiles.length;
        const failMsg =
          result.failedFiles.length > 0 ? `，失败 ${result.failedFiles.length} 项` : "";
        setMaintenanceMsg(
          `同步完成（版本 ${result.patch}）：下载 ${dlCount} 项，跳过未变更 ${result.skippedFiles} 项${failMsg}。`
        );
      }
    } catch (err) {
      setMaintenanceMsg(`同步失败：${String(err)}`);
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
              <Settings size={18} className="gold-text" /> HexBox 设置中心
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

      {/* P0: Remote Data/Model Sync & OCR Maintenance */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <CloudDownload size={17} className="gold-text" /> 本地数据 / ONNX 模型同步与维护
            </h3>
            <div className="card-sub">
              支持应用内一键下载数据包与 OCR 模型（带 SHA-256 校验）、清理 OCR 临时缓存与检测客户端语言
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {assetStatus && (
              <>
                <span className={`tag-badge ${assetStatus.readyForOffline ? "success" : "warn"}`}>
                  数据 {assetStatus.presentDataFiles}/{assetStatus.expectedDataFiles}
                </span>
                <span className={`tag-badge ${assetStatus.readyForOcr ? "success" : "warn"}`}>
                  ONNX 模型 {assetStatus.presentModelFiles}/{assetStatus.expectedModelFiles}
                </span>
              </>
            )}
            {localeText && <span className="tag-badge">语言：{localeText}</span>}
          </div>
        </div>

        <div className="settings-section" style={{ marginTop: 8 }}>
          <label className="setting-row">
            <span>远程数据与模型源 URL（留空使用内置/环境变量源）</span>
            <input
              className="hex-input"
              style={{ minWidth: 320 }}
              type="text"
              placeholder="https://your-mirror.example.com/hexbox-assets"
              value={settings.data_sync_url || ""}
              onChange={(e) => onUpdateSettings({ data_sync_url: e.target.value })}
            />
          </label>
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", marginTop: 10 }}>
            <button
              className="hex-btn primary sm"
              onClick={handleSyncAssets}
              disabled={syncing}
            >
              <CloudDownload size={14} /> {syncing ? "正在同步..." : "同步远程数据与模型"}
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
          {assetStatus &&
            (assetStatus.missingDataFiles.length > 0 ||
              assetStatus.missingModelFiles.length > 0) && (
              <div className="card-sub" style={{ marginTop: 10, color: "#f59e0b" }}>
                缺失文件：
                {[...assetStatus.missingDataFiles, ...assetStatus.missingModelFiles]
                  .slice(0, 8)
                  .join("、")}
                {assetStatus.missingDataFiles.length + assetStatus.missingModelFiles.length > 8
                  ? " 等"
                  : ""}
              </div>
            )}
          {maintenanceMsg && (
            <div className="card-sub" style={{ marginTop: 10 }}>
              {maintenanceMsg}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
