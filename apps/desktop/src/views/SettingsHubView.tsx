import React from "react";
import { Eye, RotateCcw, Settings } from "lucide-react";
import { DEFAULT_SETTINGS, type LoadedHexBoxDatabase } from "../api/dataService";
import { tauriResetOverlayPosition } from "../api/tauriBridge";
import type { AppSettings } from "../types/hexbox";

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
  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <Settings size={18} className="gold-text" /> HexBox 设置中心
            </h3>
            <div className="card-sub">
              本地持久化保存快捷键、OCR 推理档位、悬浮窗透明度与选人自动化开关
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
            <h4>OCR 与分辨率预设</h4>
            <label className="setting-row">
              <span>启用局内自动 OCR 监听</span>
              <input
                type="checkbox"
                checked={settings.auto_ocr_enabled}
                onChange={(e) => onUpdateSettings({ auto_ocr_enabled: e.target.checked })}
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
            <h4>悬浮窗与自动化开关</h4>
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
              <button className="hex-btn ghost sm" onClick={() => tauriResetOverlayPosition().catch(() => {})}>
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
    </div>
  );
};
