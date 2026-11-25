import React, { useEffect, useRef, useState } from "react";
import { Download, History, Plus, Search, Trash2, Upload, Users } from "lucide-react";
import { loadPlayerMarkers, savePlayerMarkers } from "../api/dataService";
import {
  isTauriRuntime,
  tauriDeletePlayerEncounters,
  tauriDeletePlayerMarker,
  tauriExportPlayerLibrary,
  tauriGetPlayerProfile,
  tauriImportPlayerLibrary,
  tauriListPlayerEncounters,
  tauriListPlayerMarkers,
  tauriPrunePlayerEncounters,
  tauriSavePlayerMarker,
} from "../api/tauriBridge";
import type {
  PlayerEncounterRecord,
  PlayerLibraryBundle,
  PlayerMarker,
  PlayerProfile,
} from "../types/hexbox";

const TAG_META: Record<string, { label: string; badgeClass: string }> = {
  carry: { label: "大腿 · Carry", badgeClass: "success" },
  friend: { label: "熟人 · 固定车队", badgeClass: "" },
  toxic: { label: "避雷 · 压力怪", badgeClass: "warn" },
};

export const KaiheiView: React.FC = () => {
  const [markers, setMarkers] = useState<PlayerMarker[]>(() => loadPlayerMarkers());
  const [encounters, setEncounters] = useState<PlayerEncounterRecord[]>([]);
  const [summonerName, setSummonerName] = useState("");
  const [tag, setTag] = useState<"carry" | "friend" | "toxic">("carry");
  const [note, setNote] = useState("");

  // Player query / SGP profile
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [profile, setProfile] = useState<PlayerProfile | null>(null);
  const [statusMsg, setStatusMsg] = useState<string>("");

  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const reloadBackendLibrary = async () => {
    if (!isTauriRuntime()) return;
    try {
      const [mList, eList] = await Promise.all([
        tauriListPlayerMarkers(),
        tauriListPlayerEncounters(100),
      ]);
      if (mList) {
        setMarkers(mList);
        savePlayerMarkers(mList);
      }
      if (eList) {
        setEncounters(eList);
      }
    } catch {
      // keep fallback state
    }
  };

  useEffect(() => {
    reloadBackendLibrary();
  }, []);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = summonerName.trim();
    if (!trimmed) return;
    const now = new Date();
    const ts = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
      now.getDate()
    ).padStart(2, "0")} ${String(now.getHours()).padStart(2, "0")}:${String(
      now.getMinutes()
    ).padStart(2, "0")}`;

    const newMarker: PlayerMarker = {
      puuid: `local-${Date.now()}`,
      summonerName: trimmed,
      tag,
      tagLabel: TAG_META[tag].label,
      note: note.trim() || "手动添加的玩家标记",
      encounters: 1,
      updatedAt: ts,
    };

    if (isTauriRuntime()) {
      try {
        const saved = await tauriSavePlayerMarker(newMarker);
        if (saved) {
          await reloadBackendLibrary();
          setSummonerName("");
          setNote("");
          setStatusMsg(`已持久化保存玩家标记：${saved.summonerName}`);
          return;
        }
      } catch (err) {
        setStatusMsg(`保存标记失败：${String(err)}`);
      }
    }

    const next: PlayerMarker[] = [newMarker, ...markers];
    setMarkers(next);
    savePlayerMarkers(next);
    setSummonerName("");
    setNote("");
  };

  const handleRemove = async (puuid: string) => {
    if (isTauriRuntime()) {
      try {
        await tauriDeletePlayerMarker(puuid);
        await reloadBackendLibrary();
        return;
      } catch {
        // fallback below
      }
    }
    const next = markers.filter((m) => m.puuid !== puuid);
    setMarkers(next);
    savePlayerMarkers(next);
  };

  const handleSearchProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    const q = searchQuery.trim();
    if (!q) return;
    if (!isTauriRuntime()) {
      setStatusMsg("当前为浏览器预览模式，在桌面端连接 LoL 客户端后可通过 LCU/SGP 查询玩家档案。");
      return;
    }
    setSearching(true);
    setStatusMsg("正在通过 LCU / SGP 查询玩家档案与战绩...");
    try {
      const result = await tauriGetPlayerProfile(q, 15);
      if (result) {
        setProfile(result);
        setStatusMsg(
          `查询完成（来源：${result.source.toUpperCase()}，分析 ${result.gamesAnalyzed} 场近期战绩）`
        );
      }
    } catch (err) {
      setProfile(null);
      setStatusMsg(`查询失败：${String(err)}`);
    } finally {
      setSearching(false);
    }
  };

  const handleExportLibrary = async () => {
    let bundle: PlayerLibraryBundle = {
      schemaVersion: "1",
      exportedAt: Math.floor(Date.now() / 1000),
      markers,
      encounters,
    };
    if (isTauriRuntime()) {
      const exported = await tauriExportPlayerLibrary();
      if (exported) bundle = exported;
    }
    const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `hexbox-player-library-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    setStatusMsg(`已导出 ${bundle.markers.length} 条玩家标记与 ${bundle.encounters.length} 条遭遇记录。`);
  };

  const handleImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as PlayerLibraryBundle;
      if (!parsed || !Array.isArray(parsed.markers)) {
        setStatusMsg("导入失败：JSON 格式无效（缺少 markers 数组）。");
        return;
      }
      if (isTauriRuntime()) {
        const res = await tauriImportPlayerLibrary(parsed, false);
        await reloadBackendLibrary();
        if (res) {
          setStatusMsg(`导入完成：当前库共有 ${res[0]} 条玩家标记、${res[1]} 条遭遇记录。`);
        }
      } else {
        setMarkers(parsed.markers);
        savePlayerMarkers(parsed.markers);
        setEncounters(parsed.encounters || []);
        setStatusMsg(`已导入 ${parsed.markers.length} 条玩家标记。`);
      }
    } catch (err) {
      setStatusMsg(`导入失败：${String(err)}`);
    } finally {
      e.target.value = "";
    }
  };

  const handlePruneEncounters = async () => {
    if (!isTauriRuntime()) return;
    try {
      const removed = await tauriPrunePlayerEncounters(30);
      await reloadBackendLibrary();
      setStatusMsg(`已清理 30 天前旧遭遇记录：共移除 ${removed ?? 0} 条。`);
    } catch (err) {
      setStatusMsg(`清理失败：${String(err)}`);
    }
  };

  const handleDeletePlayerEncounters = async (puuidOrName: string) => {
    if (!isTauriRuntime()) return;
    try {
      const removed = await tauriDeletePlayerEncounters(puuidOrName);
      await reloadBackendLibrary();
      setStatusMsg(`已删除 ${puuidOrName} 的 ${removed ?? 0} 条遭遇记录。`);
    } catch (err) {
      setStatusMsg(`删除失败：${String(err)}`);
    }
  };

  const prefillMarker = (name: string) => {
    setSummonerName(name);
    setStatusMsg(`已填入「${name}」，请选择标签并点击添加标记。`);
  };

  return (
    <div className="workspace-container">
      {/* P1: Player Search & SGP Profile */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <Search size={18} className="gold-text" /> 玩家档案查询（LCU / SGP 战绩 + 同局遭遇）
            </h3>
            <div className="card-sub">
              输入 Riot ID（如 召唤师名#Tag）或 PUUID，实时拉取近期胜率、KDA、均场 HexScore 与历史同局记录
            </div>
          </div>
          {statusMsg && <span className="tag-badge">{statusMsg}</span>}
        </div>

        <form className="marker-form" onSubmit={handleSearchProfile}>
          <input
            className="hex-input flex-2"
            type="text"
            placeholder="输入 召唤师名#Tag 或 PUUID 查询战绩与同局记录..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          <button type="submit" className="hex-btn primary" disabled={searching}>
            <Search size={14} /> {searching ? "查询中..." : "查询档案"}
          </button>
        </form>

        {profile && (
          <div style={{ marginTop: 14, padding: 14, borderRadius: 10, background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.08)" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 10 }}>
              <div>
                <strong style={{ fontSize: 16 }}>
                  {profile.summoner.summonerName}
                  {profile.summoner.tagLine ? `#${profile.summoner.tagLine}` : ""}
                </strong>{" "}
                <span className="tag-badge">Lv.{profile.summoner.summonerLevel}</span>{" "}
                <span className="tag-badge">来源 {profile.source.toUpperCase()}</span>
                {profile.summoner.marker && (
                  <span
                    className={`tag-badge ${TAG_META[profile.summoner.marker.tag]?.badgeClass || ""}`}
                    style={{ marginLeft: 6 }}
                  >
                    {profile.summoner.marker.tagLabel}
                  </span>
                )}
              </div>
              <button
                className="hex-btn ghost sm"
                onClick={() => prefillMarker(profile.summoner.summonerName)}
              >
                <Plus size={13} /> 标记该玩家
              </button>
            </div>
            <div style={{ display: "flex", gap: 16, flexWrap: "wrap", marginTop: 10, fontSize: 13 }}>
              <span>分析场次：<strong>{profile.gamesAnalyzed}</strong></span>
              <span>近期胜率：<strong>{(profile.winRate * 100).toFixed(1)}%</strong></span>
              <span>场均 KDA：<strong>{profile.avgKda}</strong></span>
              <span>均场 HexScore：<strong>{profile.avgHexScore.toFixed(1)}</strong></span>
              <span>历史同局：<strong>{profile.encounterSummary.totalEncounters}</strong> 次（同队 {profile.encounterSummary.allyGames} / 对手 {profile.encounterSummary.enemyGames}）</span>
              {profile.topChampions.length > 0 && (
                <span>常用英雄：<strong>{profile.topChampions.join(" / ")}</strong></span>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Player Markers */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <Users size={18} className="gold-text" /> 开黑车队与玩家标记库
            </h3>
            <div className="card-sub">
              后端持久化保存遇到的 Carry 队友、固定车队好友与避雷玩家，对局加载时自动预警，支持 JSON 导入导出
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
            <span className="tag-badge">已标记 {markers.length} 位玩家</span>
            <button className="hex-btn ghost sm" onClick={handleExportLibrary}>
              <Download size={13} /> 导出 JSON
            </button>
            <button
              className="hex-btn ghost sm"
              onClick={() => fileInputRef.current?.click()}
            >
              <Upload size={13} /> 导入 JSON
            </button>
            <input
              ref={fileInputRef}
              type="file"
              accept="application/json"
              style={{ display: "none" }}
              onChange={handleImportFile}
            />
          </div>
        </div>

        <form className="marker-form" onSubmit={handleAdd}>
          <input
            className="hex-input"
            type="text"
            placeholder="召唤师名称#Tag（如：晚风皆是意#HN1）"
            value={summonerName}
            onChange={(e) => setSummonerName(e.target.value)}
          />
          <select
            className="hex-select"
            value={tag}
            onChange={(e) => setTag(e.target.value as "carry" | "friend" | "toxic")}
          >
            <option value="carry">大腿 · Carry</option>
            <option value="friend">熟人 · 固定车队</option>
            <option value="toxic">避雷 · 压力怪</option>
          </select>
          <input
            className="hex-input flex-2"
            type="text"
            placeholder="备注（擅长英雄 / 风格 / 避雷原因）"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <button type="submit" className="hex-btn primary">
            <Plus size={14} /> 添加标记
          </button>
        </form>

        <div className="table-scroll-lg" style={{ marginTop: 12 }}>
          <table className="hex-table">
            <thead>
              <tr>
                <th>召唤师</th>
                <th>标签</th>
                <th>备注说明</th>
                <th>同局次数</th>
                <th>更新时间</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {markers.map((m) => (
                <tr key={m.puuid}>
                  <td>
                    <strong>{m.summonerName}</strong>
                  </td>
                  <td>
                    <span className={`tag-badge ${TAG_META[m.tag]?.badgeClass || ""}`}>
                      {m.tagLabel}
                    </span>
                  </td>
                  <td>{m.note}</td>
                  <td>{m.encounters} 局</td>
                  <td>{m.updatedAt}</td>
                  <td>
                    <button
                      className="hex-btn ghost sm"
                      onClick={() => handleRemove(m.puuid)}
                    >
                      <Trash2 size={13} /> 删除
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* P1: Auto-recorded Player Encounters */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <History size={18} className="gold-text" /> 自动同局遭遇记录
            </h3>
            <div className="card-sub">
              对局进行中自动记录同局 9 名玩家的英雄、阵营与 KDA，再次排到时在 10 人战力条中提示遭遇次数
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <span className="tag-badge">共 {encounters.length} 条记录</span>
            <button className="hex-btn ghost sm" onClick={handlePruneEncounters}>
              <Trash2 size={13} /> 清理 30 天前记录
            </button>
          </div>
        </div>

        {encounters.length === 0 ? (
          <div className="card-sub" style={{ padding: "12px 0" }}>
            暂无自动遭遇记录。进入一场对局后，HexBox 将自动记录所有同局玩家信息。
          </div>
        ) : (
          <div className="table-scroll-lg">
            <table className="hex-table">
              <thead>
                <tr>
                  <th>玩家</th>
                  <th>使用英雄</th>
                  <th>阵营关系</th>
                  <th>KDA</th>
                  <th>时间</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {encounters.map((enc) => (
                  <tr key={enc.id}>
                    <td>
                      <strong>{enc.summonerName}</strong>
                    </td>
                    <td>{enc.championName}</td>
                    <td>
                      <span className={`tag-badge ${enc.relation === "ally" ? "success" : "warn"}`}>
                        {enc.relation === "ally" ? "同队" : "对手"}
                      </span>
                    </td>
                    <td>{enc.kda}</td>
                    <td>{enc.timeLabel}</td>
                    <td style={{ display: "flex", gap: 6 }}>
                      <button
                        className="hex-btn ghost sm"
                        onClick={() => prefillMarker(enc.summonerName)}
                      >
                        <Plus size={12} /> 标记
                      </button>
                      <button
                        className="hex-btn ghost sm"
                        onClick={() => handleDeletePlayerEncounters(enc.summonerName)}
                      >
                        <Trash2 size={12} /> 清除
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
