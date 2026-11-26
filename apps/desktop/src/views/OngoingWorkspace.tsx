import React, { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  Award,
  CheckCircle2,
  Crown,
  Crosshair,
  Eye,
  Flame,
  RefreshCw,
  Shield,
  Sparkles,
  Swords,
  Users,
  Zap,
} from "lucide-react";
import {
  analyzeEnemyCounter,
  evaluate5v5Lineup,
  evaluateAugmentOffer,
  type LoadedHexBoxDatabase,
} from "../api/dataService";
import {
  isTauriRuntime,
  tauriApplyArurfRunePage,
  tauriApplySummonerSpells,
  tauriBenchSwap,
  tauriCancelBenchGrab,
  tauriGetArurfRecommendation,
  tauriGetAugmentOfferStats,
  tauriGetBenchGrabStatus,
  tauriGetGamePlayers,
  tauriGetIntelCacheStatus,
  tauriGetSummonerSpellSnapshot,
  tauriGrabBenchChampion,
  tauriRefreshGamePlayerIntel,
  tauriRunOcrPreset,
  tauriSetManualAugmentOverride,
  tauriSetManualAugmentSelection,
  tauriSetOverlayState,
  tauriSyncItemSetToClient,
} from "../api/tauriBridge";
import type {
  AppSettings,
  ArurfRecommendation,
  AugmentOfferStatsReport,
  BenchGrabStatus,
  GamePlayerIntel,
  IntelCacheStatus,
  LcuStatus,
  OverlayStatePayload,
  SummonerSpellSnapshot,
} from "../types/hexbox";
import { getChampionIconUrl, getItemIconUrl, getTierMeta } from "../utils/hexScore";

interface Props {
  db: LoadedHexBoxDatabase;
  settings?: AppSettings;
  lcuStatus?: LcuStatus;
  liveOverlayState?: OverlayStatePayload | null;
  onOpenHeroRecommend: (heroName: string) => void;
  onOpenOverlayPreview: (overlayKind: string) => void;
}

function sameStringList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((item, idx) => item === b[idx]);
}

function formatOcrStatus(status: string, ocrMs?: number | null): string {
  const trimmed = status.trim();
  if (typeof ocrMs !== "number" || !Number.isFinite(ocrMs) || ocrMs <= 0) {
    return trimmed;
  }
  const suffix = ` · ${ocrMs.toFixed(1)}ms`;
  return trimmed.endsWith(suffix) ? trimmed : `${trimmed}${suffix}`;
}

export const OngoingWorkspace: React.FC<Props> = ({
  db,
  settings,
  lcuStatus,
  liveOverlayState,
  onOpenHeroRecommend,
  onOpenOverlayPreview,
}) => {
  const [phase, setPhase] = useState<"ChampSelect" | "InProgress" | "PostgameReview">("InProgress");
  const [selectedHero, setSelectedHero] = useState<string>("寒冰射手");
  const [stage, setStage] = useState<number>(2);
  const [confirmedAugments, setConfirmedAugments] = useState<string[]>(["易损"]);
  const [candidateOffer, setCandidateOffer] = useState<string[]>([
    "亮出你的剑",
    "暴击飞弹",
    "最终形态",
  ]);
  const [allyTeam, setAllyTeam] = useState<string[]>([
    "寒冰射手",
    "光辉女郎",
    "探险家",
    "皮城女警",
    "深海泰坦",
  ]);
  const [enemyTeam, setEnemyTeam] = useState<string[]>([
    "疾风剑豪",
    "暗裔剑魔",
    "祖安狂人",
    "众星之子",
    "影流之主",
  ]);
  const [benchPool, setBenchPool] = useState<string[]>([
    "曙光女神",
    "卡牌大师",
    "无双剑姬",
    "虚空女皇",
  ]);
  const [autoGrabIds, setAutoGrabIds] = useState<string[]>(["卡牌大师", "曙光女神"]);
  const [benchGrabStatus, setBenchGrabStatus] = useState<BenchGrabStatus | null>(null);
  const [spellSnapshot, setSpellSnapshot] = useState<SummonerSpellSnapshot | null>(null);
  const [arurfRec, setArurfRec] = useState<ArurfRecommendation | null>(null);
  const [clientActionMsg, setClientActionMsg] = useState<string>("");
  const [liveIntel, setLiveIntel] = useState<GamePlayerIntel[]>([]);
  const [intelCache, setIntelCache] = useState<IntelCacheStatus | null>(null);
  const [intelLoading, setIntelLoading] = useState<boolean>(false);
  const [offerStats, setOfferStats] = useState<AugmentOfferStatsReport | null>(null);
  const [ocrStatus, setOcrStatus] = useState<string>("ONNX Runtime 就绪 · 35.1ms");
  const [ocrRunning, setOcrRunning] = useState<boolean>(false);

  const heroObj = useMemo(
    () => db.champions.find((c) => c.hero === selectedHero) || db.champions[0],
    [db.champions, selectedHero]
  );

  const formulaBuild = useMemo(
    () => db.formulaItemsByHero[selectedHero] || db.formulaItemsByHero[heroObj?.id || "22"],
    [db.formulaItemsByHero, selectedHero, heroObj]
  );

  const skillOrder = useMemo(
    () => db.skillOrdersByHeroId[heroObj?.id || "22"],
    [db.skillOrdersByHeroId, heroObj]
  );

  const evaluatedCards = useMemo(
    () =>
      evaluateAugmentOffer(
        db,
        selectedHero,
        candidateOffer,
        stage,
        confirmedAugments,
        settings?.augment_value_mode || "balanced"
      ),
    [db, selectedHero, candidateOffer, stage, confirmedAugments, settings?.augment_value_mode]
  );

  const enemyCounter = useMemo(
    () => analyzeEnemyCounter(db, selectedHero, enemyTeam),
    [db, selectedHero, enemyTeam]
  );

  const lineupPrediction = useMemo(
    () => evaluate5v5Lineup(db, allyTeam, enemyTeam, benchPool, selectedHero),
    [db, allyTeam, enemyTeam, benchPool, selectedHero]
  );

  useEffect(() => {
    if (!liveOverlayState) return;
    if (liveOverlayState.lcu?.connected) {
      if (liveOverlayState.phase === "ChampSelect") {
        setPhase((prev) => (prev === "ChampSelect" ? prev : "ChampSelect"));
      } else if (liveOverlayState.phase === "InProgress") {
        setPhase((prev) => (prev === "InProgress" ? prev : "InProgress"));
      }
      if (liveOverlayState.heroName) {
        setSelectedHero((prev) =>
          prev === liveOverlayState.heroName ? prev : liveOverlayState.heroName
        );
      }
      if (liveOverlayState.allyTeam.length > 0) {
        setAllyTeam((prev) =>
          sameStringList(prev, liveOverlayState.allyTeam) ? prev : liveOverlayState.allyTeam
        );
      }
      if (liveOverlayState.enemyTeam.length > 0) {
        setEnemyTeam((prev) =>
          sameStringList(prev, liveOverlayState.enemyTeam) ? prev : liveOverlayState.enemyTeam
        );
      }
      if (liveOverlayState.benchPool.length > 0) {
        setBenchPool((prev) =>
          sameStringList(prev, liveOverlayState.benchPool) ? prev : liveOverlayState.benchPool
        );
      }
    }
    if (liveOverlayState.candidateAugments.length === 3) {
      setCandidateOffer((prev) =>
        sameStringList(prev, liveOverlayState.candidateAugments)
          ? prev
          : liveOverlayState.candidateAugments
      );
    }
    if (liveOverlayState.ocrStatus) {
      const formatted = formatOcrStatus(liveOverlayState.ocrStatus, liveOverlayState.ocrMs);
      setOcrStatus((prev) => (prev === formatted ? prev : formatted));
    }
  }, [liveOverlayState]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const nextPhase = lcuStatus?.connected ? lcuStatus.phase : phase;
    if (
      liveOverlayState &&
      liveOverlayState.heroName === selectedHero &&
      liveOverlayState.phase === nextPhase &&
      liveOverlayState.stage === stage &&
      sameStringList(liveOverlayState.candidateAugments, candidateOffer) &&
      sameStringList(liveOverlayState.confirmedAugments, confirmedAugments) &&
      sameStringList(liveOverlayState.allyTeam, allyTeam) &&
      sameStringList(liveOverlayState.enemyTeam, enemyTeam) &&
      sameStringList(liveOverlayState.benchPool, benchPool)
    ) {
      return;
    }
    const payload: OverlayStatePayload = {
      heroName: selectedHero,
      phase: nextPhase,
      stage,
      candidateAugments: candidateOffer,
      confirmedAugments,
      allyTeam,
      enemyTeam,
      benchPool,
      currentGold: liveOverlayState?.currentGold ?? 2850,
      ownedItemIds: liveOverlayState?.ownedItemIds ?? [],
      isDead: liveOverlayState?.isDead ?? false,
      lcu: lcuStatus ?? {
        connected: false,
        phase: "Disconnected",
        port: null,
        summoner_name: null,
        lockfile_path: null,
        locale: liveOverlayState?.lcu?.locale ?? null,
      },
      livePlayers: liveOverlayState?.livePlayers ?? [],
      ocrStatus: liveOverlayState?.ocrStatus ?? null,
      ocrMs: liveOverlayState?.ocrMs ?? null,
    };
    tauriSetOverlayState(payload).catch(() => {});
  }, [selectedHero, stage, candidateOffer, confirmedAugments, allyTeam, enemyTeam, benchPool]);

  const handleTriggerLiveOcr = async () => {
    setOcrRunning(true);
    setOcrStatus("正在调用常驻 Python ONNX 侧车识别 3 张海克斯卡片...");
    try {
      const presetKey = settings?.selected_preset_key || "1920x1080@100";
      const data = await tauriRunOcrPreset(presetKey);
      if (!data) {
        setOcrStatus("当前为纯浏览器预览模式（在 Tauri 桌面端内可实时调用常驻 ONNX Worker）");
        return;
      }
      const list = (data.ocr_matches ?? data.result) as Array<{ match_name?: string }> | undefined;
      if (data.status === "ok" && Array.isArray(list)) {
        const names = list
          .map((r) => r.match_name)
          .filter((n): n is string => typeof n === "string" && n.length > 0);
        if (names.length === 3) {
          setCandidateOffer((prev) => (sameStringList(prev, names) ? prev : names));
        }
        const timings = data.timings as { ocr_ms?: number; total_ms?: number } | undefined;
        const ocrMsText =
          typeof timings?.ocr_ms === "number" ? timings.ocr_ms.toFixed(1) : "0.0";
        const totalMsText =
          typeof timings?.total_ms === "number" ? timings.total_ms.toFixed(1) : "0.0";
        setOcrStatus(
          `常驻 ONNX 识别成功：${names.join(" / ")} · 推理 ${ocrMsText}ms (总耗时 ${totalMsText}ms)`
        );
      } else {
        setOcrStatus(`OCR 返回异常：${String(data.error ?? "unknown")}`);
      }
    } catch (err) {
      setOcrStatus(`OCR 调用失败：${String(err)}`);
    } finally {
      setOcrRunning(false);
    }
  };

  const handleSwapBench = (benchHero: string) => {
    setBenchPool((prev) => prev.map((h) => (h === benchHero ? selectedHero : h)));
    setAllyTeam((prev) => prev.map((h) => (h === selectedHero ? benchHero : h)));
    setSelectedHero(benchHero);
    if (isTauriRuntime() && lcuStatus?.connected && lcuStatus.phase === "ChampSelect") {
      const target = db.champions.find((c) => c.hero === benchHero);
      const champId = Number(target?.id);
      if (Number.isFinite(champId) && champId > 0) {
        tauriBenchSwap(champId).catch(() => {});
      }
    }
  };

  const toggleAutoGrab = async (heroName: string) => {
    const exists = autoGrabIds.includes(heroName);
    setAutoGrabIds((prev) =>
      prev.includes(heroName) ? prev.filter((x) => x !== heroName) : [...prev, heroName]
    );
    if (!isTauriRuntime()) return;
    if (exists) {
      const st = await tauriCancelBenchGrab().catch(() => null);
      if (st) setBenchGrabStatus(st);
    } else {
      const target = db.champions.find((c) => c.hero === heroName);
      const champId = Number(target?.id);
      if (Number.isFinite(champId) && champId > 0) {
        const st = await tauriGrabBenchChampion(champId).catch(() => null);
        if (st) setBenchGrabStatus(st);
      }
    }
  };

  const handleCancelBenchGrab = async () => {
    setAutoGrabIds([]);
    if (!isTauriRuntime()) {
      setBenchGrabStatus({
        active: false,
        targetChampionId: null,
        targetHeroName: null,
        statusText: "已取消本局自动秒抢",
      });
      return;
    }
    const st = await tauriCancelBenchGrab().catch(() => null);
    if (st) setBenchGrabStatus(st);
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    tauriGetBenchGrabStatus().then((st) => st && setBenchGrabStatus(st)).catch(() => {});
    tauriGetSummonerSpellSnapshot().then((sp) => sp && setSpellSnapshot(sp)).catch(() => {});
    tauriGetArurfRecommendation(selectedHero).then((r) => r && setArurfRec(r)).catch(() => {});
    tauriGetAugmentOfferStats(8).then((r) => r && setOfferStats(r)).catch(() => {});
    tauriGetIntelCacheStatus().then((c) => c && setIntelCache(c)).catch(() => {});
  }, [selectedHero, phase]);

  const handleApplySpells = async () => {
    if (!isTauriRuntime()) {
      setClientActionMsg("当前为浏览器预览模式，在桌面端选人阶段可一键应用推荐召唤师技能。");
      return;
    }
    try {
      const updated = await tauriApplySummonerSpells();
      if (updated) {
        setSpellSnapshot(updated);
        setClientActionMsg(
          `已应用召唤师技能：${updated.recommendedSpell1Name} + ${updated.recommendedSpell2Name}（闪现 ${updated.flashSlot} 键）`
        );
      }
    } catch (err) {
      setClientActionMsg(`应用召唤师技能失败：${String(err)}`);
    }
  };

  const handleSyncItemSet = async () => {
    if (!isTauriRuntime()) {
      setClientActionMsg("当前为浏览器预览模式，在桌面端连接客户端后可一键同步装备方案。");
      return;
    }
    try {
      const res = await tauriSyncItemSetToClient(selectedHero);
      if (res) {
        setClientActionMsg(res.message);
      }
    } catch (err) {
      setClientActionMsg(`同步装备方案失败：${String(err)}`);
    }
  };

  const handleApplyArurfRunes = async () => {
    if (!isTauriRuntime()) {
      setClientActionMsg("当前为浏览器预览模式，在桌面端连接客户端后可一键写入无限火力符文页。");
      return;
    }
    try {
      const page = await tauriApplyArurfRunePage(selectedHero);
      if (page) {
        setClientActionMsg(`已写入客户端符文页：${page.name}（基石：${page.keystoneName}）`);
      }
    } catch (err) {
      setClientActionMsg(`写入符文页失败：${String(err)}`);
    }
  };

  const handleRefreshIntel = async (force = true) => {
    if (!isTauriRuntime()) {
      setClientActionMsg("当前为浏览器预览模式，连接客户端进入对局后可拉取真实 10 人 SGP/LCU 战力情报。");
      return;
    }
    setIntelLoading(true);
    try {
      const players = force ? await tauriRefreshGamePlayerIntel() : await tauriGetGamePlayers();
      if (players) setLiveIntel(players);
      const cache = await tauriGetIntelCacheStatus();
      if (cache) setIntelCache(cache);
    } catch {
      // ignore
    } finally {
      setIntelLoading(false);
    }
  };

  const presetOffers: Array<{ label: string; cards: string[]; confirmed: string[] }> = [
    {
      label: "暴击联动局 (含【易损】联动)",
      cards: ["亮出你的剑", "暴击飞弹", "最终形态"],
      confirmed: ["易损"],
    },
    {
      label: "废卡冲突局 (已选【回归基本功】)",
      cards: ["风语者的祝福", "终极刷新", "杀戮时间到了"],
      confirmed: ["回归基本功"],
    },
    {
      label: "棱彩神仙打架局",
      cards: ["亮出你的剑", "男爵之手", "艾卡西亚的陷落"],
      confirmed: ["泰坦的坚决"],
    },
  ];

  return (
    <div className="workspace-container">
      {/* Top Gameflow Status Bar */}
      <div className="panel-card status-banner">
        <div className="status-left">
          <span className="pulse-dot online" />
          <div>
            <div className="status-title">
              海克斯大乱斗 ·{" "}
              <span className="gold-text">HaxAssistant 海克斯助手</span>
            </div>
            <div className="status-sub">
              当前版本 Patch {db.patch} ({db.reportDate}) · {ocrStatus}
            </div>
          </div>
        </div>

        <div className="phase-tabs">
          <button
            className={`phase-tab ${phase === "ChampSelect" ? "active" : ""}`}
            onClick={() => setPhase("ChampSelect")}
          >
            <Users size={15} /> 1. 选人阶段 (秒抢/阵容胜率)
          </button>
          <button
            className={`phase-tab ${phase === "InProgress" ? "active" : ""}`}
            onClick={() => setPhase("InProgress")}
          >
            <Sparkles size={15} /> 2. 局内进行中 (OCR三选一/针对出装)
          </button>
          <button
            className={`phase-tab ${phase === "PostgameReview" ? "active" : ""}`}
            onClick={() => setPhase("PostgameReview")}
          >
            <Award size={15} /> 3. 赛后三选一复盘
          </button>
        </div>

        <div className="status-actions">
          <select
            className="hex-select"
            value={selectedHero}
            onChange={(e) => {
              const next = e.target.value;
              setAllyTeam((prev) => prev.map((h) => (h === selectedHero ? next : h)));
              setSelectedHero(next);
            }}
          >
            {db.champions.map((c) => (
              <option key={c.id} value={c.hero}>
                当前英雄：{c.hero} ({(c.win_rate * 100).toFixed(1)}%)
              </option>
            ))}
          </select>
          <button className="hex-btn primary" onClick={handleTriggerLiveOcr} disabled={ocrRunning}>
            <RefreshCw size={14} className={ocrRunning ? "spin" : ""} /> 触发 OCR 识别 (F8)
          </button>
        </div>
      </div>

      {/* 5v5 Lineup Matchup Winrate Bar (Always visible across phases) */}
      <div className="panel-card lineup-bar-card">
        <div className="lineup-header">
          <div className="lineup-title">
            <Swords size={16} className="gold-text" />
            <span>5v5 阵容胜率预测 & 敌我克制修正 (Ultra 本地模型已激活)</span>
          </div>
          <div className="lineup-metrics">
            <span className="metric-pill">
              基础阵容胜率：<strong>{(lineupPrediction.baseWin * 100).toFixed(1)}%</strong>
            </span>
            <span className="metric-pill highlight">
              克制修正后胜率：<strong>{(lineupPrediction.adjustedWin * 100).toFixed(1)}%</strong>
            </span>
            <span
              className={`metric-pill ${
                lineupPrediction.correction >= 0 ? "pos-text" : "neg-text"
              }`}
            >
              克制修正：{lineupPrediction.correction >= 0 ? "+" : ""}
              {(lineupPrediction.correction * 100).toFixed(1)} 个百分点
            </span>
          </div>
        </div>

        <div className="winrate-progress-track">
          <div
            className="winrate-fill ally"
            style={{ width: `${(lineupPrediction.adjustedWin * 100).toFixed(1)}%` }}
          >
            我方蓝队 {(lineupPrediction.adjustedWin * 100).toFixed(1)}%
          </div>
          <div className="winrate-fill enemy">
            敌方红队 {((1 - lineupPrediction.adjustedWin) * 100).toFixed(1)}%
          </div>
        </div>

        <div className="lineup-notes-row">
          {lineupPrediction.notes.map((n, i) => (
            <span key={i} className="tag-badge">
              {n}
            </span>
          ))}
          <button
            className="hex-btn ghost sm"
            onClick={() => onOpenOverlayPreview("champ-select")}
          >
            <Eye size={13} /> 预览选人悬浮窗
          </button>
        </div>
      </div>

      {/* PHASE 1: CHAMP SELECT */}
      {phase === "ChampSelect" && (
        <div className="grid-2col">
          <div className="panel-card">
            <div className="card-header">
              <h3>
                <Zap size={16} className="gold-text" /> 替补席毫秒级秒抢 & 换人胜率增益
              </h3>
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <span className="tag-badge success">
                  {benchGrabStatus?.active
                    ? `秒抢待命：${benchGrabStatus.targetHeroName || benchGrabStatus.targetChampionId}`
                    : "bench_autograb_worker 监听中 (30ms)"}
                </span>
                <button className="hex-btn ghost sm" onClick={handleCancelBenchGrab}>
                  取消本局自动抢
                </button>
              </div>
            </div>
            {benchGrabStatus?.statusText && (
              <div className="card-sub" style={{ marginBottom: 8 }}>
                秒抢状态：{benchGrabStatus.statusText}
              </div>
            )}
            <p className="card-desc">
              点击“立即换入”模拟 LCU <code>POST /lol-champ-select/v1/session/bench/swap/&#123;id&#125;</code>
              ；勾选“预订秒抢”后一旦队友换下该英雄将毫秒级自动抢入。
            </p>
            <div className="bench-grid">
              {lineupPrediction.benchSuggestions.map((b) => {
                const isBooked = autoGrabIds.includes(b.heroName);
                return (
                  <div key={b.heroName} className={`bench-item-card ${isBooked ? "booked" : ""}`}>
                    <img
                      src={getChampionIconUrl(b.heroId)}
                      alt={b.heroName}
                      className="hero-avatar-md"
                      onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                    />
                    <div className="bench-info">
                      <div className="bench-name-row">
                        <strong>{b.heroName}</strong>
                        <span
                          className="tier-pill"
                          style={{ background: b.hexColor, color: "#fff" }}
                        >
                          {b.hexLabel} {b.hexScore.toFixed(0)}
                        </span>
                      </div>
                      <div className="bench-sub">
                        单英雄胜率 {(b.heroWinRate * 100).toFixed(1)}% ·{" "}
                        <span className={b.gain >= 0 ? "pos-text" : "neg-text"}>
                          换入阵容 {b.gain >= 0 ? "+" : ""}
                          {(b.gain * 100).toFixed(1)}%
                        </span>
                      </div>
                    </div>
                    <div className="bench-btns">
                      <button
                        className="hex-btn primary sm"
                        onClick={() => handleSwapBench(b.heroName)}
                      >
                        立即换入
                      </button>
                      <button
                        className={`hex-btn sm ${isBooked ? "gold" : "ghost"}`}
                        onClick={() => toggleAutoGrab(b.heroName)}
                      >
                        {isBooked ? "已预订秒抢" : "预订秒抢"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="panel-card">
            <div className="card-header">
              <h3>
                <Shield size={16} className="gold-text" /> 当前锁定英雄速览与一键配置
              </h3>
              <button
                className="hex-btn ghost sm"
                onClick={() => onOpenHeroRecommend(selectedHero)}
              >
                查看完整海克斯/出装库 →
              </button>
            </div>
            {heroObj && (
              <div className="hero-quick-card">
                <div className="hero-quick-top">
                  <img
                    src={getChampionIconUrl(heroObj.id)}
                    alt={heroObj.hero}
                    className="hero-avatar-lg"
                    onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                  />
                  <div>
                    <h4>
                      {heroObj.hero}{" "}
                      <span
                        className="tier-pill"
                        style={{ background: heroObj.hex_tier_color, color: "#fff" }}
                      >
                        {heroObj.hex_label} · Hex {heroObj.hex_score.toFixed(1)}
                      </span>
                    </h4>
                    <div className="quick-stats-line">
                      版本胜率 <strong>{(heroObj.win_rate * 100).toFixed(2)}%</strong> · 登场率{" "}
                      <strong>{(heroObj.pick_rate * 100).toFixed(2)}%</strong> · 样本{" "}
                      <strong>{heroObj.games.toLocaleString()}</strong> 场
                    </div>
                    {skillOrder && (
                      <div className="skill-order-badge">
                        推荐加点：<strong>主 {skillOrder.order[0]} 副 {skillOrder.order[1]}</strong>{" "}
                        ({skillOrder.seq.slice(0, 10)}...) · 胜率 {(skillOrder.win_rate * 100).toFixed(1)}%
                      </div>
                    )}
                  </div>
                </div>

                {formulaBuild && (
                  <div className="formula-mini-section">
                    <div className="section-subtitle">公式化出门装 & 核心神装：</div>
                    <div className="item-icon-row">
                      {formulaBuild.starterItems.map((it, idx) => (
                        <div key={`st-${idx}`} className="item-chip">
                          <img
                            src={getItemIconUrl(it.id)}
                            alt={it.name}
                            onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                          />
                          <span>{it.name}</span>
                        </div>
                      ))}
                      <span className="arrow-sep">→</span>
                      {formulaBuild.coreItems.map((it, idx) => (
                        <div key={`co-${idx}`} className="item-chip core">
                          <img
                            src={getItemIconUrl(it.id)}
                            alt={it.name}
                            onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                          />
                          <span>{it.name}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* P1: One-click client actions: Summoner Spells, Item Set Sync, ARURF Runes */}
                <div
                  style={{
                    marginTop: 12,
                    paddingTop: 12,
                    borderTop: "1px solid rgba(255,255,255,0.08)",
                    display: "flex",
                    flexDirection: "column",
                    gap: 8,
                  }}
                >
                  <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
                    <button className="hex-btn primary sm" onClick={handleApplySpells}>
                      一键应用召唤师技能 (
                      {spellSnapshot
                        ? `${spellSnapshot.recommendedSpell1Name}+${spellSnapshot.recommendedSpell2Name}`
                        : settings?.summoner_flash_slot === "D"
                        ? "闪现+标记"
                        : "标记+闪现"}
                      )
                    </button>
                    <button className="hex-btn ghost sm" onClick={handleSyncItemSet}>
                      同步装备方案到客户端
                    </button>
                    <button className="hex-btn ghost sm" onClick={handleApplyArurfRunes}>
                      一键写入无限火力符文
                      {arurfRec ? ` (${arurfRec.runePage.keystoneName})` : ""}
                    </button>
                  </div>
                  {arurfRec && (
                    <div className="card-sub">
                      ARURF 推荐：{arurfRec.roleTag} · {arurfRec.runePage.primaryName} /{" "}
                      {arurfRec.runePage.subName}（基石：{arurfRec.runePage.keystoneName}）· 召唤师技能{" "}
                      {arurfRec.spellNames.join(" + ")}
                    </div>
                  )}
                  {clientActionMsg && (
                    <div className="card-sub" style={{ color: "#facc15" }}>
                      {clientActionMsg}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* PHASE 2: IN-GAME OCR & DYNAMIC ITEM BUILD */}
      {phase === "InProgress" && (
        <>
          {/* Hextech Augment 3-Card OCR & Evaluation Section */}
          <div className="panel-card">
            <div className="card-header">
              <div>
                <h3>
                  <Sparkles size={17} className="gold-text" /> 局内海克斯三选一实时 OCR 识别与评分引擎
                </h3>
                <div className="card-sub">
                  支持自由切换当前回合阶段 (Stage 1~4)、已选海克斯与候选三张卡，实时验证阶段价值偏移与机制联动/废卡互斥
                </div>
              </div>
              <div className="header-controls">
                <div className="stage-selector">
                  {[1, 2, 3, 4].map((s) => (
                    <button
                      key={s}
                      className={`stage-btn ${stage === s ? "active" : ""}`}
                      onClick={() => setStage(s)}
                    >
                      第 {s} 轮选择
                    </button>
                  ))}
                </div>
                <button
                  className="hex-btn ghost sm"
                  onClick={() => onOpenOverlayPreview("augment")}
                >
                  <Eye size={13} /> 打开局内强化悬浮窗
                </button>
              </div>
            </div>

            {/* Preset Simulation Switcher + Confirmed Augments */}
            <div className="augment-sim-toolbar">
              <div className="sim-group">
                <span className="sim-label">快速测试场景：</span>
                {presetOffers.map((po, i) => (
                  <button
                    key={i}
                    className="hex-btn ghost sm"
                    onClick={() => {
                      setCandidateOffer(po.cards);
                      setConfirmedAugments(po.confirmed);
                    }}
                  >
                    {po.label}
                  </button>
                ))}
              </div>
              <div className="sim-group">
                <span className="sim-label">本局已选海克斯：</span>
                {confirmedAugments.map((ca) => (
                  <span key={ca} className="confirmed-aug-chip">
                    {ca}
                    <button
                      onClick={() => {
                        const next = confirmedAugments.filter((x) => x !== ca);
                        setConfirmedAugments(next);
                        tauriSetManualAugmentSelection(next).catch(() => {});
                      }}
                    >
                      ×
                    </button>
                  </span>
                ))}
                <select
                  className="hex-select sm"
                  value=""
                  onChange={(e) => {
                    if (e.target.value && !confirmedAugments.includes(e.target.value)) {
                      const next = [...confirmedAugments, e.target.value];
                      setConfirmedAugments(next);
                      tauriSetManualAugmentSelection(next)
                        .then(() => tauriGetAugmentOfferStats(8).then((r) => r && setOfferStats(r)))
                        .catch(() => {});
                    }
                  }}
                >
                  <option value="">+ 添加已选海克斯...</option>
                  {db.augments.slice(0, 120).map((a) => (
                    <option key={a.id} value={a.augment_name}>
                      {a.augment_name} ({a.rarity})
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* 3 Augment Cards Side-by-Side */}
            <div className="augment-3cards-grid">
              {evaluatedCards.map((card, idx) => (
                <div
                  key={card.slot}
                  className={`augment-eval-card ${card.is_best ? "best-choice" : ""} ${
                    card.conflict_note ? "conflict-choice" : ""
                  }`}
                >
                  <div className="aug-card-top">
                    <div className="slot-picker">
                      <span className="slot-num">槽位 #{card.slot}</span>
                      <select
                        className="hex-select sm"
                        value={card.augment_name}
                        onChange={(e) => {
                          const next = [...candidateOffer];
                          next[idx] = e.target.value;
                          setCandidateOffer(next);
                          tauriSetManualAugmentOverride(idx + 1, e.target.value).catch(() => {});
                        }}
                      >
                        {db.augments.map((a) => (
                          <option key={a.id} value={a.augment_name}>
                            {a.augment_name} ({a.rarity})
                          </option>
                        ))}
                      </select>
                    </div>
                    {card.is_best && (
                      <span className="best-crown-badge">
                        <Crown size={13} /> 首选推荐
                      </span>
                    )}
                  </div>

                  <div className="aug-verdict-row">
                    <div>
                      <div className="aug-title">{card.augment_name}</div>
                      <span className="aug-rarity-pill">{card.rarity}</span>
                    </div>
                    <div
                      className="verdict-seal-box"
                      style={{
                        borderColor: card.hex_tier_color,
                        color: card.hex_tier_color,
                      }}
                    >
                      <span className="seal-tier">{card.hex_label}</span>
                      <span className="seal-score">{card.hex_score.toFixed(1)}</span>
                    </div>
                  </div>

                  <div className="aug-stats-grid">
                    <div className="stat-cell">
                      <span className="stat-lbl">搭配胜率</span>
                      <strong className={card.win_rate >= 0.51 ? "pos-text" : ""}>
                        {(card.win_rate * 100).toFixed(2)}%
                      </strong>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-lbl">同阶净胜率差</span>
                      <strong className={card.delta_win_rate >= 0 ? "pos-text" : "neg-text"}>
                        {card.delta_win_rate >= 0 ? "+" : ""}
                        {(card.delta_win_rate * 100).toFixed(2)}%
                      </strong>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-lbl">样本场次</span>
                      <strong>{card.sample_size.toLocaleString()} 场</strong>
                    </div>
                  </div>

                  <div className="aug-reason-line">{card.reason}</div>

                  {card.synergy_note && (
                    <div className="synergy-alert-box">
                      <Flame size={14} /> {card.synergy_note}
                    </div>
                  )}

                  {card.conflict_note && (
                    <div className="conflict-alert-box">
                      <AlertTriangle size={14} /> {card.conflict_note}
                    </div>
                  )}

                  {card.top_items.length > 0 && (
                    <div className="aug-linked-items">
                      <span className="linked-lbl">该海克斯最佳联动装备：</span>
                      <div className="mini-items-list">
                        {card.top_items.map((it) => (
                          <div key={it.item_id} className="mini-item-pill" title={`平均第 ${it.average_index.toFixed(1)} 件出`}>
                            <img
                              src={getItemIconUrl(it.item_id)}
                              alt={it.item_name}
                              onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                            />
                            <span>
                              {it.item_name} {(it.win_rate * 100).toFixed(1)}%
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>

            {offerStats && offerStats.totalOffers > 0 && (
              <div
                style={{
                  marginTop: 12,
                  padding: "8px 12px",
                  borderRadius: 8,
                  background: "rgba(255,255,255,0.03)",
                  fontSize: 12,
                  display: "flex",
                  gap: 10,
                  flexWrap: "wrap",
                  alignItems: "center",
                }}
              >
                <span className="gold-text">
                  本机海克斯出现统计（累计识别 {offerStats.totalOffers} 次 · 已选 {offerStats.totalPicks} 次）：
                </span>
                {offerStats.topOffered.slice(0, 5).map((entry) => (
                  <span key={entry.augmentName} className="tag-badge">
                    {entry.augmentName} ({entry.offered}次 / 选{entry.picked})
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Dynamic Formula & Enemy Counter Item Build + 10-Player Intel */}
          <div className="grid-2col">
            <div className="panel-card">
              <div className="card-header">
                <h3>
                  <Crosshair size={16} className="gold-text" /> 局内实时流派 & 针对敌方阵容动态克制出装
                </h3>
                <button
                  className="hex-btn ghost sm"
                  onClick={() => onOpenOverlayPreview("item")}
                >
                  <Eye size={13} /> 预览流派悬浮窗 (F7)
                </button>
              </div>

              <div className="damage-share-bar-wrap">
                <div className="damage-bar-labels">
                  <span>
                    敌方伤害威胁构成：物理 {(enemyCounter.physicalShare * 100).toFixed(0)}% · 魔法{" "}
                    {(enemyCounter.magicShare * 100).toFixed(0)}% · 真实{" "}
                    {(enemyCounter.trueShare * 100).toFixed(0)}%
                  </span>
                  <span className="gold-text">
                    倾向：{enemyCounter.lean === "physical" ? "偏物理" : enemyCounter.lean === "magic" ? "偏魔法" : "均衡混合"}
                  </span>
                </div>
                <div className="damage-share-track">
                  <div
                    className="dmg-seg phys"
                    style={{ width: `${enemyCounter.physicalShare * 100}%` }}
                  />
                  <div
                    className="dmg-seg magic"
                    style={{ width: `${enemyCounter.magicShare * 100}%` }}
                  />
                  <div
                    className="dmg-seg true-dmg"
                    style={{ width: `${enemyCounter.trueShare * 100}%` }}
                  />
                </div>
              </div>

              <div className="counter-suggestions-list">
                {enemyCounter.antiHeal && (
                  <div className="counter-alert-item antiheal">
                    <img
                      src={getItemIconUrl(enemyCounter.antiHeal.itemId)}
                      alt={enemyCounter.antiHeal.itemName}
                    />
                    <div>
                      <div className="counter-item-title">
                        【重伤必出】{enemyCounter.antiHeal.itemName}{" "}
                        <span className="pos-text">
                          预估胜率 +{(enemyCounter.antiHeal.lift * 100).toFixed(1)}%
                        </span>
                      </div>
                      <div className="counter-item-reason">{enemyCounter.antiHeal.reason}</div>
                    </div>
                  </div>
                )}

                {enemyCounter.boots && (
                  <div className="counter-alert-item">
                    <img
                      src={getItemIconUrl(enemyCounter.boots.itemId)}
                      alt={enemyCounter.boots.itemName}
                    />
                    <div>
                      <div className="counter-item-title">
                        【针对鞋靴】{enemyCounter.boots.itemName}{" "}
                        <span className="pos-text">
                          +{(enemyCounter.boots.lift * 100).toFixed(1)}%
                        </span>
                      </div>
                      <div className="counter-item-reason">{enemyCounter.boots.reason}</div>
                    </div>
                  </div>
                )}

                {enemyCounter.defensiveItems.map((def) => (
                  <div key={def.itemId} className="counter-alert-item">
                    <img src={getItemIconUrl(def.itemId)} alt={def.itemName} />
                    <div>
                      <div className="counter-item-title">
                        【破甲/防装】{def.itemName}{" "}
                        <span className="pos-text">+{(def.lift * 100).toFixed(1)}%</span>
                      </div>
                      <div className="counter-item-reason">{def.reason}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel-card">
              <div className="card-header">
                <h3>
                  <Activity size={16} className="gold-text" /> 10 人实时战力评分 (HexScore) & 车队识别
                </h3>
                <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
                  {intelCache && (
                    <span className="tag-badge">
                      缓存 {intelCache.cachedPlayers} 人{intelCache.sgpEnabled ? " · SGP" : " · LCU"}
                    </span>
                  )}
                  <button
                    className="hex-btn ghost sm"
                    onClick={() => handleRefreshIntel(true)}
                    disabled={intelLoading}
                  >
                    <RefreshCw size={13} className={intelLoading ? "spin" : ""} />{" "}
                    {intelLoading ? "拉取中..." : "刷新实时情报"}
                  </button>
                  <button
                    className="hex-btn ghost sm"
                    onClick={() => onOpenOverlayPreview("player-scores")}
                  >
                    <Eye size={13} /> 预览战力悬浮窗
                  </button>
                </div>
              </div>

              {liveIntel.length > 0 ? (
                <div className="roster-dual-columns">
                  {(["ORDER", "CHAOS"] as const).map((side) => {
                    const list = liveIntel.filter((p) => p.team === side);
                    return (
                      <div key={side} className="roster-col">
                        <div className={`roster-col-title ${side === "ORDER" ? "ally" : "enemy"}`}>
                          {side === "ORDER" ? "蓝队实时情报" : "红队实时情报"} ({list.length} 人)
                        </div>
                        {list.map((p, idx) => {
                          const ch = db.champions.find((c) => c.hero === p.championName);
                          const score = p.avgHexScore ?? 60;
                          const tier = getTierMeta(score);
                          return (
                            <div key={`${p.summonerName}-${idx}`} className="roster-player-row">
                              <img
                                src={getChampionIconUrl(ch?.id || "22")}
                                alt={p.championName}
                                className="hero-avatar-sm"
                                onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                              />
                              <div className="roster-p-main">
                                <div className="roster-p-name">
                                  <strong>{p.summonerName}</strong>{" "}
                                  <span style={{ opacity: 0.75, fontSize: 12 }}>{p.championName}</span>
                                  {p.marker && (
                                    <span className="premade-badge">{p.marker.tagLabel}</span>
                                  )}
                                  {p.encounterCount > 0 && (
                                    <span className="self-badge">偶遇 {p.encounterCount} 次</span>
                                  )}
                                </div>
                                <div className="roster-p-sub">
                                  {p.recentGames > 0
                                    ? `近 ${p.recentGames} 场胜率 ${((p.recentWinRate ?? 0) * 100).toFixed(0)}% · KDA ${p.avgKda ?? "-"}`
                                    : p.tags.join(" · ") || "暂无近期战绩缓存"}
                                </div>
                              </div>
                              <div className="roster-score-right">
                                <span
                                  className="tier-pill"
                                  style={{ background: tier.bg, color: tier.color }}
                                >
                                  {tier.label} {score.toFixed(1)}
                                </span>
                                <span className="mmr-sub">{p.source.toUpperCase()}</span>
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="roster-dual-columns">
                <div className="roster-col">
                  <div className="roster-col-title ally">我方蓝队 (平均 HexScore 74.6 · 预估 2480分)</div>
                  {allyTeam.map((hname, idx) => {
                    const ch = db.champions.find((c) => c.hero === hname);
                    const simScore = [84.2, 76.5, 71.0, 68.4, 73.1][idx] || 70;
                    const tier = getTierMeta(simScore);
                    return (
                      <div key={idx} className="roster-player-row">
                        <img
                          src={getChampionIconUrl(ch?.id || "22")}
                          alt={hname}
                          className="hero-avatar-sm"
                        />
                        <div className="roster-p-main">
                          <div className="roster-p-name">
                            <strong>{hname}</strong>
                            {idx < 2 && <span className="premade-badge">双排 #1</span>}
                            {idx === 0 && <span className="self-badge">我</span>}
                          </div>
                          <div className="roster-p-sub">
                            近期胜率 {Math.round(52 + (simScore - 65) * 0.8)}% · KDA{" "}
                            {(2.6 + (simScore - 60) * 0.05).toFixed(1)}
                          </div>
                        </div>
                        <div className="roster-score-right">
                          <span
                            className="tier-pill"
                            style={{ background: tier.bg, color: tier.color }}
                          >
                            {tier.label} {simScore.toFixed(1)}
                          </span>
                          <span className="mmr-sub">
                            {Math.round(1500 + (simScore - 50) * 28)} 分
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>

                <div className="roster-col">
                  <div className="roster-col-title enemy">
                    敌方红队 (支持切换敌方阵容实时重算克制装)
                  </div>
                  {enemyTeam.map((hname, idx) => {
                    const ch = db.champions.find((c) => c.hero === hname);
                    const simScore = [81.0, 69.2, 74.8, 64.5, 66.0][idx] || 68;
                    const tier = getTierMeta(simScore);
                    return (
                      <div key={idx} className="roster-player-row">
                        <img
                          src={getChampionIconUrl(ch?.id || "157")}
                          alt={hname}
                          className="hero-avatar-sm"
                        />
                        <div className="roster-p-main">
                          <select
                            className="hex-select xs"
                            value={hname}
                            onChange={(e) => {
                              const next = [...enemyTeam];
                              next[idx] = e.target.value;
                              setEnemyTeam(next);
                            }}
                          >
                            {db.champions.map((c) => (
                              <option key={c.id} value={c.hero}>
                                {c.hero}
                              </option>
                            ))}
                          </select>
                          <div className="roster-p-sub">
                            {idx >= 1 && idx <= 3 ? (
                              <span className="premade-badge enemy">三排车队</span>
                            ) : (
                              "单排玩家"
                            )}
                          </div>
                        </div>
                        <div className="roster-score-right">
                          <span
                            className="tier-pill"
                            style={{ background: tier.bg, color: tier.color }}
                          >
                            {tier.label} {simScore.toFixed(1)}
                          </span>
                          <span className="mmr-sub">
                            {Math.round(1500 + (simScore - 50) * 28)} 分
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
              )}
            </div>
          </div>
        </>
      )}

      {/* PHASE 3: POSTGAME AUGMENT REVIEW */}
      {phase === "PostgameReview" && (
        <div className="panel-card">
          <div className="card-header">
            <h3>
              <Award size={17} className="gold-text" /> 赛后海克斯三选一决策复盘 & 同英雄分位数对比
            </h3>
            <span className="tag-badge success">赛后复盘</span>
          </div>
          <div className="postgame-rounds-list">
            {[
              {
                round: 1,
                offered: ["易损", "科学狂人", "不动如山"],
                chosen: "易损",
                best: "易损",
                optimal: true,
                note: "第 1 轮选出【易损】为后续暴击/技能流奠定核心增伤 (+4.2% 胜率)",
              },
              {
                round: 2,
                offered: candidateOffer,
                chosen: evaluatedCards[0]?.augment_name || "亮出你的剑",
                best:
                  evaluatedCards.find((c) => c.is_best)?.augment_name ||
                  evaluatedCards[0]?.augment_name,
                optimal:
                  (evaluatedCards[0]?.augment_name || "") ===
                  (evaluatedCards.find((c) => c.is_best)?.augment_name || ""),
                note: "本轮三选一由实时 OCR 记录，联动增益显著",
              },
              {
                round: 3,
                offered: ["男爵之手", "冰寒", "神射法师"],
                chosen: "冰寒",
                best: "男爵之手",
                optimal: false,
                note: "漏选【男爵之手】（HexScore 81.2，比【冰寒】高出 +5.4% 胜率）",
              },
            ].map((r) => (
              <div key={r.round} className={`postgame-round-card ${r.optimal ? "ok" : "warn"}`}>
                <div className="pg-round-num">第 {r.round} 轮强化</div>
                <div className="pg-round-main">
                  <div className="pg-offered">
                    当时出现：{r.offered.map((o) => `【${o}】`).join(" / ")}
                  </div>
                  <div className="pg-note">{r.note}</div>
                </div>
                <div className="pg-verdict">
                  {r.optimal ? (
                    <span className="pos-text">
                      <CheckCircle2 size={15} /> 最优选择 ({r.chosen})
                    </span>
                  ) : (
                    <span className="neg-text">
                      <AlertTriangle size={15} /> 实际选【{r.chosen}】· 最优为【{r.best}】
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
