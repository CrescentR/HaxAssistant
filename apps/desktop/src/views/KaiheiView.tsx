import React, { useState } from "react";
import { Plus, Trash2, Users } from "lucide-react";
import { loadPlayerMarkers, savePlayerMarkers } from "../api/dataService";
import type { PlayerMarker } from "../types/hexbox";

const TAG_META: Record<string, { label: string; badgeClass: string }> = {
  carry: { label: "大腿 · Carry", badgeClass: "success" },
  friend: { label: "熟人 · 固定车队", badgeClass: "" },
  toxic: { label: "避雷 · 压力怪", badgeClass: "warn" },
};

export const KaiheiView: React.FC = () => {
  const [markers, setMarkers] = useState<PlayerMarker[]>(() => loadPlayerMarkers());
  const [summonerName, setSummonerName] = useState("");
  const [tag, setTag] = useState<"carry" | "friend" | "toxic">("carry");
  const [note, setNote] = useState("");

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = summonerName.trim();
    if (!trimmed) return;
    const now = new Date();
    const ts = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
      now.getDate()
    ).padStart(2, "0")} ${String(now.getHours()).padStart(2, "0")}:${String(
      now.getMinutes()
    ).padStart(2, "0")}`;

    const next: PlayerMarker[] = [
      {
        puuid: `local-${Date.now()}`,
        summonerName: trimmed,
        tag,
        tagLabel: TAG_META[tag].label,
        note: note.trim() || "手动添加的玩家标记",
        encounters: 1,
        updatedAt: ts,
      },
      ...markers,
    ];
    setMarkers(next);
    savePlayerMarkers(next);
    setSummonerName("");
    setNote("");
  };

  const handleRemove = (puuid: string) => {
    const next = markers.filter((m) => m.puuid !== puuid);
    setMarkers(next);
    savePlayerMarkers(next);
  };

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <Users size={18} className="gold-text" /> 开黑车队与玩家标记库
            </h3>
            <div className="card-sub">
              本地持久化记录遇到的 Carry 队友、固定车队好友与避雷玩家，对局加载时自动预警
            </div>
          </div>
          <span className="tag-badge">已标记 {markers.length} 位玩家</span>
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
      </div>

      <div className="panel-card">
        <div className="table-scroll-lg">
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
    </div>
  );
};
