import React from "react";
import { Award, History, Trophy } from "lucide-react";
import type { LoadedHexBoxDatabase } from "../api/dataService";
import { getChampionIconUrl, getTierMeta } from "../utils/hexScore";

interface PersonalHistoryViewProps {
  db: LoadedHexBoxDatabase;
  onSelectHero: (heroName: string) => void;
}

interface DemoMatch {
  id: string;
  heroName: string;
  result: "WIN" | "LOSS";
  kda: string;
  damage: number;
  duration: string;
  augments: string[];
  hexScore: number;
  timeAgo: string;
}

const DEMO_MATCHES: DemoMatch[] = [
  {
    id: "m-1",
    heroName: "寒冰射手",
    result: "WIN",
    kda: "14 / 5 / 27",
    damage: 62400,
    duration: "17:42",
    augments: ["易损", "亮出你的剑", "暴击飞弹", "最终形态"],
    hexScore: 86.4,
    timeAgo: "2 小时前",
  },
  {
    id: "m-2",
    heroName: "疾风剑豪",
    result: "WIN",
    kda: "18 / 8 / 15",
    damage: 54900,
    duration: "19:15",
    augments: ["回归基本功", "钢化你心", "杀戮时间到了"],
    hexScore: 81.2,
    timeAgo: "昨天 22:30",
  },
  {
    id: "m-3",
    heroName: "光辉女郎",
    result: "LOSS",
    kda: "9 / 9 / 21",
    damage: 41200,
    duration: "21:08",
    augments: ["尤里卡", "神射法师", "冰寒"],
    hexScore: 63.5,
    timeAgo: "昨天 21:45",
  },
  {
    id: "m-4",
    heroName: "深海泰坦",
    result: "WIN",
    kda: "5 / 7 / 34",
    damage: 24800,
    duration: "16:20",
    augments: ["坦克引擎", "不动如山", "歌利亚巨人"],
    hexScore: 78.8,
    timeAgo: "2 天前",
  },
];

export const PersonalHistoryView: React.FC<PersonalHistoryViewProps> = ({
  db,
  onSelectHero,
}) => {
  const wins = DEMO_MATCHES.filter((m) => m.result === "WIN").length;
  const avgHex =
    DEMO_MATCHES.reduce((sum, m) => sum + m.hexScore, 0) / Math.max(1, DEMO_MATCHES.length);

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <History size={18} className="gold-text" /> 个人海克斯大乱斗战绩与复盘
            </h3>
            <div className="card-sub">
              本地离线预览数据（接入 Rust LCU / SGP 后自动同步真实对局记录）
            </div>
          </div>
          <div className="lineup-metrics">
            <span className="metric-pill highlight">
              <Trophy size={13} /> 近期胜率：{Math.round((wins / DEMO_MATCHES.length) * 100)}% ({wins}胜{" "}
              {DEMO_MATCHES.length - wins}负)
            </span>
            <span className="metric-pill">
              <Award size={13} /> 平均选卡 HexScore：{avgHex.toFixed(1)}
            </span>
          </div>
        </div>
      </div>

      <div className="panel-card">
        <div className="history-list">
          {DEMO_MATCHES.map((m) => {
            const ch = db.champions.find((c) => c.hero === m.heroName);
            const tier = getTierMeta(m.hexScore);
            return (
              <div key={m.id} className={`history-row ${m.result === "WIN" ? "win" : "loss"}`}>
                <img
                  src={getChampionIconUrl(ch?.id || "22")}
                  alt={m.heroName}
                  className="hero-avatar-md"
                  onError={(e) => ((e.target as HTMLImageElement).src = "/hexbox-logo.png")}
                />
                <div className="history-main">
                  <div className="history-title-row">
                    <strong>{m.heroName}</strong>
                    <span className={`tag-badge ${m.result === "WIN" ? "success" : "warn"}`}>
                      {m.result === "WIN" ? "胜利" : "失败"} · {m.duration}
                    </span>
                    <span className="history-kda">{m.kda}</span>
                    <span className="card-sub">输出 {m.damage.toLocaleString()} · {m.timeAgo}</span>
                  </div>
                  <div className="history-augs">
                    {m.augments.map((a) => (
                      <span key={a} className="confirmed-aug-chip">
                        {a}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="history-right">
                  <span className="tier-pill" style={{ background: tier.bg, color: tier.color }}>
                    {tier.label} {m.hexScore.toFixed(1)}
                  </span>
                  <button
                    className="hex-btn ghost sm"
                    onClick={() => onSelectHero(m.heroName)}
                  >
                    查看英雄库 →
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
