import React, { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, Award, History, RefreshCw, Trophy, Wifi, WifiOff } from "lucide-react";
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

/**
 * - `unsupported`: browser preview, no desktop host to read the client from.
 * - `offline`: desktop host found no running League client.
 * - `error`: the client (or the host call) failed.
 * - `ready`: a real SGP/LCU answer, possibly with zero matches.
 */
type LoadStatus = "loading" | "unsupported" | "offline" | "error" | "ready";

const SOURCE_LABELS: Record<string, string> = {
  sgp: "SGP 战绩服务",
  lcu: "LCU 客户端",
};

/** Reviews are keyed by player + game so a summoner switch never reuses one. */
const reviewKey = (puuid: string | null, gameId: string) => `${puuid ?? ""}:${gameId}`;

type ReviewState =
  | { kind: "loading" }
  | { kind: "ready"; review: PostgameReview }
  | { kind: "error"; message: string };

export const PersonalHistoryView: React.FC<PersonalHistoryViewProps> = ({
  db,
  onSelectHero,
}) => {
  const [status, setStatus] = useState<LoadStatus>(() =>
    isTauriRuntime() ? "loading" : "unsupported"
  );
  const [matches, setMatches] = useState<MatchHistoryEntry[]>([]);
  const [source, setSource] = useState<string | null>(null);
  const [summonerName, setSummonerName] = useState<string | null>(null);
  const [puuid, setPuuid] = useState<string | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  // Only the latest request may write state; older in-flight responses are dropped.
  const requestSeq = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const [openReviewKey, setOpenReviewKey] = useState<string | null>(null);
  const [reviews, setReviews] = useState<Record<string, ReviewState>>({});
  const puuidRef = useRef<string | null>(null);

  const clearMatches = useCallback((next: "offline" | "error") => {
    puuidRef.current = null;
    setStatus(next);
    setSource(null);
    setMatches([]);
    setPuuid(null);
    setReviews({});
    setOpenReviewKey(null);
  }, []);

  const loadMatches = useCallback(async () => {
    if (!isTauriRuntime()) {
      setStatus("unsupported");
      return;
    }
    const seq = ++requestSeq.current;
    const isCurrent = () => mounted.current && seq === requestSeq.current;
    setRefreshing(true);
    try {
      const resp = await tauriGetMatchHistory(20);
      if (!isCurrent()) return;
      if (!resp) {
        setStatus("unsupported");
        return;
      }
      const nextPuuid = resp.puuid ?? null;
      setSummonerName(resp.summonerName);
      setErrorText(resp.error ?? null);
      if (resp.source === "sgp" || resp.source === "lcu") {
        if (puuidRef.current !== nextPuuid) {
          // Different summoner: drop reviews computed for the previous one.
          setReviews({});
          setOpenReviewKey(null);
        } else {
          // Close a review whose match is no longer in the list.
          setOpenReviewKey((open) =>
            open && resp.matches.some((m) => reviewKey(nextPuuid, m.gameId) === open)
              ? open
              : null
          );
        }
        puuidRef.current = nextPuuid;
        setPuuid(nextPuuid);
        setStatus("ready");
        setSource(resp.source);
        setMatches(resp.matches);
      } else {
        // Never keep showing a previous summoner's matches as if current.
        clearMatches(resp.source === "offline" ? "offline" : "error");
      }
    } catch (err) {
      if (!isCurrent()) return;
      setErrorText(String(err));
      clearMatches("error");
    } finally {
      if (isCurrent()) setRefreshing(false);
    }
  }, [clearMatches]);

  useEffect(() => {
    void loadMatches();
  }, [loadMatches]);

  const reviewsRef = useRef(reviews);
  reviewsRef.current = reviews;

  const toggleReview = useCallback(
    async (m: MatchHistoryEntry) => {
      const key = reviewKey(puuid, m.gameId);
      if (openReviewKey === key) {
        setOpenReviewKey(null);
        return;
      }
      setOpenReviewKey(key);
      const existing = reviewsRef.current[key];
      if (existing && existing.kind !== "error") return;

      setReviews((prev) => ({ ...prev, [key]: { kind: "loading" } }));
      let next: ReviewState;
      try {
        const review = await tauriGetPostgameReview(m);
        next = review
          ? { kind: "ready", review }
          : { kind: "error", message: "复盘需要在桌面端运行" };
      } catch (err) {
        next = { kind: "error", message: `复盘失败：${String(err)}` };
      }
      if (!mounted.current) return;
      // Results land under their own key, so a late answer can never be shown
      // under a different match; a reset (summoner switch) discards it.
      setReviews((prev) => (prev[key]?.kind === "loading" ? { ...prev, [key]: next } : prev));
    },
    [openReviewKey, puuid]
  );

  const wins = matches.filter((m) => m.result === "WIN").length;
  const rated = matches.filter((m): m is MatchHistoryEntry & { hexScore: number } =>
    typeof m.hexScore === "number"
  );
  const avgHex = rated.length
    ? rated.reduce((sum, m) => sum + m.hexScore, 0) / rated.length
    : null;

  const sourceLabel = source ? SOURCE_LABELS[source] ?? source.toUpperCase() : null;
  const statusLine = (() => {
    switch (status) {
      case "loading":
        return (
          <>
            <RefreshCw size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
            正在读取英雄联盟客户端战绩...
          </>
        );
      case "ready":
        return (
          <>
            <Wifi size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
            来源：{sourceLabel}
            {summonerName ? ` · ${summonerName}` : ""}（共 {matches.length} 局）
          </>
        );
      case "offline":
        return (
          <>
            <WifiOff size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
            未检测到运行中的英雄联盟客户端，启动并登录后点击刷新
          </>
        );
      case "unsupported":
        return (
          <>
            <WifiOff size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
            浏览器预览无法读取客户端战绩，请在桌面端中打开
          </>
        );
      case "error":
        return (
          <>
            <AlertTriangle size={12} style={{ verticalAlign: "middle", marginRight: 4 }} />
            战绩读取失败{summonerName ? `（${summonerName}）` : ""}
          </>
        );
    }
  })();

  const emptyMessage = (() => {
    switch (status) {
      case "loading":
        return "正在加载战绩...";
      case "ready":
        return "该账号暂无可显示的对局记录";
      case "offline":
        return errorText ?? "英雄联盟客户端未运行，暂无战绩";
      case "unsupported":
        return "个人战绩仅在桌面端可用";
      case "error":
        return errorText ?? "战绩读取失败，请稍后重试";
    }
  })();

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <History size={18} className="gold-text" /> 个人海克斯大乱斗战绩与复盘
            </h3>
            <div className="card-sub">{statusLine}</div>
          </div>
          <div className="lineup-metrics">
            {status === "ready" && matches.length > 0 && (
              <>
                <span className="metric-pill highlight">
                  <Trophy size={13} /> 近期胜率：{Math.round((wins / matches.length) * 100)}% ({wins}胜{" "}
                  {matches.length - wins}负)
                </span>
                <span className="metric-pill">
                  <Award size={13} /> 平均选卡 HexScore：
                  {avgHex != null ? `${avgHex.toFixed(1)}（${rated.length} 局有评分）` : "暂无评分"}
                </span>
              </>
            )}
            {status !== "unsupported" && (
              <button
                className="hex-btn ghost sm"
                onClick={() => void loadMatches()}
                disabled={refreshing}
              >
                <RefreshCw size={13} /> {refreshing ? "同步中..." : "刷新战绩"}
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="panel-card">
        {matches.length === 0 ? (
          <div className="card-sub" role="status">
            {emptyMessage}
          </div>
        ) : (
          <div className="history-list">
            {matches.map((m) => {
              const key = reviewKey(puuid, m.gameId);
              const ch = db.champions.find(
                (c) => c.id === String(m.championId) || c.hero === m.heroName
              );
              const tier = m.hexScore != null ? getTierMeta(m.hexScore) : null;
              const review = reviews[key];
              const meta = [
                m.gameMode,
                `输出 ${m.damage.toLocaleString()}`,
                m.timeAgo,
              ].filter(Boolean);
              return (
                <div key={key} className={`history-row ${m.result === "WIN" ? "win" : "loss"}`}>
                  <img
                    src={getChampionIconUrl(ch?.id || String(m.championId))}
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
                      <span className="card-sub">{meta.join(" · ")}</span>
                    </div>
                    <div className="history-augs">
                      {m.augments.length > 0 ? (
                        m.augments.map((a, i) => (
                          <span key={`${a}-${i}`} className="confirmed-aug-chip">
                            {a}
                          </span>
                        ))
                      ) : (
                        <span className="card-sub">无强化记录</span>
                      )}
                    </div>
                  </div>
                  <div className="history-right">
                    {tier && m.hexScore != null ? (
                      <span className="tier-pill" style={{ background: tier.bg, color: tier.color }}>
                        {tier.label} {m.hexScore.toFixed(1)}
                      </span>
                    ) : (
                      <span className="card-sub" title="本局强化不在数据集中，无法评分">
                        无评分
                      </span>
                    )}
                    <button className="hex-btn ghost sm" onClick={() => void toggleReview(m)}>
                      {openReviewKey === key ? "收起复盘" : "复盘"}
                    </button>
                    <button
                      className="hex-btn ghost sm"
                      onClick={() => onSelectHero(ch?.hero || m.heroName)}
                    >
                      查看英雄库 →
                    </button>
                  </div>
                  {openReviewKey === key &&
                    (review?.kind === "ready" ? (
                      <PostgameReviewPanel review={review.review} />
                    ) : (
                      <div className="postgame-review">
                        {review?.kind === "error" ? review.message : "正在生成复盘..."}
                      </div>
                    ))}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
