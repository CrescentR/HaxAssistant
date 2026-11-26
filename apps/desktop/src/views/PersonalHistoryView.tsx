import React, { useCallback, useEffect, useState } from "react";
import { Award, History, RefreshCw, Trophy, Wifi, WifiOff } from "lucide-react";
import type { LoadedHexBoxDatabase } from "../api/dataService";
import { isTauriRuntime, tauriGetMatchHistory, tauriGetPostgameReview } from "../api/tauriBridge";
import type { MatchHistoryEntry, PostgameReview } from "../types/hexbox";
import { getChampionIconUrl, getTierMeta } from "../utils/hexScore";

const PostgameReviewPanel: React.FC<{ review: PostgameReview }> = ({ review }) => (
  <div className="postgame-review">
    <div className="postgame-head">
      <span className="postgame-grade">{review.grade}</span>
      <div>
        <strong>赛后复盘 · 综合 {review.score.toFixed(1)} 分</strong>
        <div className="card-sub">
          公式核心装 {review.coreItemsBuilt}/{review.coreItemsTotal}
          {review.missingCoreItems.length > 0 && ` · 未出：${review.missingCoreItems.join("、")}`}
        </div>
      </div>
    </div>

    <div className="postgame-metrics">
      {review.metrics.map((m) => (
        <div key={m.key} className="postgame-metric">
          <span>
            {m.label} <strong>{Math.round(m.perMinute).toLocaleString()}</strong>
            {m.median != null && <span className="card-sub"> / 中位 {Math.round(m.median)}</span>}
          </span>
          <div className="postgame-bar">
            <i style={{ width: `${Math.round((m.percentile ?? 0) * 100)}%` }} />
          </div>
          <span className="card-sub">{m.verdict}</span>
        </div>
      ))}
    </div>

    {review.augments.length > 0 && (
      <div className="postgame-augs">
        {review.augments.map((a) => (
          <span
            key={a.name}
            className="tier-pill"
            style={{ color: a.tierColor, border: `1px solid ${a.tierColor}` }}
            title={a.note}
          >
            {a.name} · {a.tierLabel} {a.hexScore.toFixed(1)}
          </span>
        ))}
      </div>
    )}

    <ul className="postgame-list">
      {review.summary.map((line) => (
        <li key={line}>{line}</li>
      ))}
      {review.links.map((line) => (
        <li key={line}>联动：{line}</li>
      ))}
      {review.conflicts.map((line) => (
        <li key={line}>冲突：{line}</li>
      ))}
      {review.betterAugments.length > 0 && (
        <li>这个英雄评分更高、本局没拿的强化：{review.betterAugments.join("、")}</li>
      )}
    </ul>
  </div>
);

interface PersonalHistoryViewProps {
  db: LoadedHexBoxDatabase;
  onSelectHero: (heroName: string) => void;
}

const DEMO_MATCHES: MatchHistoryEntry[] = [
  {
    gameId: "demo-1",
    championId: 22,
    heroName: "寒冰射手",
    result: "WIN",
    kills: 14,
    deaths: 5,
    assists: 27,
    kda: "14 / 5 / 27",
    damage: 62400,
    durationSecs: 1062,
    duration: "17:42",
    queueId: 450,
    gameMode: "ARAM",
    augments: ["易损", "亮出你的剑", "暴击飞弹", "最终形态"],
    hexScore: 86.4,
    gameCreation: 0,
    timeAgo: "2 小时前",
  },
  {
    gameId: "demo-2",
    championId: 157,
    heroName: "疾风剑豪",
    result: "WIN",
    kills: 18,
    deaths: 8,
    assists: 15,
    kda: "18 / 8 / 15",
    damage: 54900,
    durationSecs: 1155,
    duration: "19:15",
    queueId: 450,
    gameMode: "ARAM",
    augments: ["回归基本功", "钢化你心", "杀戮时间到了"],
    hexScore: 81.2,
    gameCreation: 0,
    timeAgo: "昨天 22:30",
  },
  {
    gameId: "demo-3",
    championId: 99,
    heroName: "光辉女郎",
    result: "LOSS",
    kills: 9,
    deaths: 9,
    assists: 21,
    kda: "9 / 9 / 21",
    damage: 41200,
    durationSecs: 1268,
    duration: "21:08",
    queueId: 450,
    gameMode: "ARAM",
    augments: ["尤里卡", "神射法师", "冰寒"],
    hexScore: 63.5,
    gameCreation: 0,
    timeAgo: "昨天 21:45",
  },
  {
    gameId: "demo-4",
    championId: 111,
    heroName: "深海泰坦",
    result: "WIN",
    kills: 5,
    deaths: 7,
    assists: 34,
    kda: "5 / 7 / 34",
    damage: 24800,
    durationSecs: 980,
    duration: "16:20",
    queueId: 450,
    gameMode: "ARAM",
    augments: ["坦克引擎", "不动如山", "歌利亚巨人"],
    hexScore: 78.8,
    gameCreation: 0,
    timeAgo: "2 天前",
  },
];

export const PersonalHistoryView: React.FC<PersonalHistoryViewProps> = ({
  db,
  onSelectHero,
}) => {
  const [matches, setMatches] = useState<MatchHistoryEntry[]>(DEMO_MATCHES);
  const [source, setSource] = useState<"lcu" | "offline">("offline");
  const [summonerName, setSummonerName] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const loadMatches = useCallback(async () => {
    if (!isTauriRuntime()) {
      setSource("offline");
      setMatches(DEMO_MATCHES);
      return;
    }
    setLoading(true);
    try {
      const resp = await tauriGetMatchHistory(20);
      if (resp && resp.source === "lcu" && resp.matches.length > 0) {
        setSource("lcu");
        setSummonerName(resp.summonerName);
        setMatches(resp.matches);
      } else {
        setSource("offline");
        setSummonerName(resp?.summonerName ?? null);
        setMatches(DEMO_MATCHES);
      }
    } catch {
      setSource("offline");
      setMatches(DEMO_MATCHES);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadMatches();
  }, [loadMatches]);

  // Post-game reviews are computed by the desktop host, one match at a time.
  const [openReviewId, setOpenReviewId] = useState<string | null>(null);
  const [reviews, setReviews] = useState<Record<string, PostgameReview>>({});
  const [reviewError, setReviewError] = useState<string | null>(null);
  const toggleReview = useCallback(
    async (m: MatchHistoryEntry) => {
      setReviewError(null);
      if (openReviewId === m.gameId) {
        setOpenReviewId(null);
        return;
      }
      setOpenReviewId(m.gameId);
      if (reviews[m.gameId]) return;
      try {
        const review = await tauriGetPostgameReview(m);
        if (review) {
          setReviews((prev) => ({ ...prev, [m.gameId]: review }));
        } else {
          setReviewError("复盘需要在桌面端运行");
        }
      } catch (err) {
        setReviewError(`复盘失败：${String(err)}`);
      }
    },
    [openReviewId, reviews]
  );

  const wins = matches.filter((m) => m.result === "WIN").length;
  const avgHex =
    matches.reduce((sum, m) => sum + m.hexScore, 0) / Math.max(1, matches.length);

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <History size={18} className="gold-text" /> 个人海克斯大乱斗战绩与复盘
            </h3>
            <div className="card-sub">
              {source === "lcu" ? (
                <>
                  <Wifi size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
                  LCU 实时战绩同步{summonerName ? ` · ${summonerName}` : ""}（共 {matches.length} 局）
                </>
              ) : (
                <>
                  <WifiOff size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
                  未检测到运行中的英雄联盟客户端，当前展示本地离线预览数据
                </>
              )}
            </div>
          </div>
          <div className="lineup-metrics">
            <span className="metric-pill highlight">
              <Trophy size={13} /> 近期胜率：{Math.round((wins / Math.max(1, matches.length)) * 100)}% ({wins}胜{" "}
              {matches.length - wins}负)
            </span>
            <span className="metric-pill">
              <Award size={13} /> 平均选卡 HexScore：{avgHex.toFixed(1)}
            </span>
            <button
              className="hex-btn ghost sm"
              onClick={() => void loadMatches()}
              disabled={loading}
            >
              <RefreshCw size={13} /> {loading ? "同步中..." : "刷新战绩"}
            </button>
          </div>
        </div>
      </div>

      <div className="panel-card">
        <div className="history-list">
          {matches.map((m) => {
            const ch = db.champions.find(
              (c) => c.hero === m.heroName || c.id === String(m.championId)
            );
            const tier = getTierMeta(m.hexScore);
            return (
              <div key={m.gameId} className={`history-row ${m.result === "WIN" ? "win" : "loss"}`}>
                <img
                  src={getChampionIconUrl(ch?.id || String(m.championId || 22))}
                  alt={m.heroName}
                  className="hero-avatar-md"
                  onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                />
                <div className="history-main">
                  <div className="history-title-row">
                    <strong>{m.heroName}</strong>
                    <span className={`tag-badge ${m.result === "WIN" ? "success" : "warn"}`}>
                      {m.result === "WIN" ? "胜利" : "失败"} · {m.duration}
                    </span>
                    <span className="history-kda">{m.kda}</span>
                    <span className="card-sub">
                      {m.gameMode} · 输出 {m.damage.toLocaleString()} · {m.timeAgo}
                    </span>
                  </div>
                  <div className="history-augs">
                    {m.augments.length > 0 ? (
                      m.augments.map((a) => (
                        <span key={a} className="confirmed-aug-chip">
                          {a}
                        </span>
                      ))
                    ) : (
                      <span className="card-sub">常规大乱斗 / 无强化记录</span>
                    )}
                  </div>
                </div>
                <div className="history-right">
                  <span className="tier-pill" style={{ background: tier.bg, color: tier.color }}>
                    {tier.label} {m.hexScore.toFixed(1)}
                  </span>
                  <button className="hex-btn ghost sm" onClick={() => void toggleReview(m)}>
                    {openReviewId === m.gameId ? "收起复盘" : "复盘"}
                  </button>
                  <button
                    className="hex-btn ghost sm"
                    onClick={() => onSelectHero(ch?.hero || m.heroName)}
                  >
                    查看英雄库 →
                  </button>
                </div>
                {openReviewId === m.gameId &&
                  (reviews[m.gameId] ? (
                    <PostgameReviewPanel review={reviews[m.gameId]} />
                  ) : (
                    <div className="postgame-review">{reviewError || "正在生成复盘..."}</div>
                  ))}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
