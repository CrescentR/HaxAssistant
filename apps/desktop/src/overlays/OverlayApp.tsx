import React, { useEffect, useMemo, useState } from "react";
import { Crown, Crosshair, ShoppingBag, Sparkles, Swords, Users, X } from "lucide-react";
import {
  analyzeEnemyCounter,
  evaluate5v5Lineup,
  evaluateAugmentOffer,
  type LoadedHexBoxDatabase,
} from "../api/dataService";
import { getChampionIconUrl, getItemIconUrl, getTierMeta } from "../utils/hexScore";
import { tauriConfirmOcrAugment, tauriGetItemPlan, tauriToggleOverlay } from "../api/tauriBridge";
import type { ItemPlan, OverlayStatePayload } from "../types/hexbox";

export type OverlayKind =
  | "augment"
  | "item"
  | "champ-select"
  | "player-scores"
  | "death-hint"
  | "item-launcher";

interface OverlayAppProps {
  kind: OverlayKind;
  db: LoadedHexBoxDatabase;
  heroName?: string;
  valueMode?: string;
  liveState?: OverlayStatePayload;
}

export const OverlayApp: React.FC<OverlayAppProps> = ({
  kind,
  db,
  heroName = "",
  valueMode = "balanced",
  liveState,
}) => {
  const effectiveHero = liveState ? liveState.heroName : heroName;
  const effectiveStage = liveState?.stage ?? 1;
  const candidateAugments = liveState?.candidateAugments ?? [];
  const confirmedAugments = liveState?.confirmedAugments ?? [];
  const allyTeam = liveState?.allyTeam ?? [];
  const enemyTeam = liveState?.enemyTeam ?? [];
  const benchPool = liveState?.benchPool ?? [];
  const currentGold = liveState?.currentGold ?? 0;
  const [selectionError, setSelectionError] = useState("");

  const augmentCards = useMemo(
    () =>
      evaluateAugmentOffer(
        db,
        effectiveHero,
        candidateAugments,
        effectiveStage,
        confirmedAugments,
        valueMode
      ),
    [db, effectiveHero, candidateAugments, effectiveStage, confirmedAugments, valueMode]
  );

  const formula = db.formulaItemsByHero[effectiveHero];
  const counter = useMemo(
    () => analyzeEnemyCounter(db, effectiveHero, enemyTeam),
    [db, effectiveHero, enemyTeam]
  );
  const lineup = useMemo(
    () => evaluate5v5Lineup(db, allyTeam, enemyTeam, benchPool, effectiveHero),
    [db, allyTeam, enemyTeam, benchPool, effectiveHero]
  );

  // Shopping plan from the host; stays null in a plain browser preview.
  const [itemPlan, setItemPlan] = useState<ItemPlan | null>(null);
  const wantsPlan = kind === "item" || kind === "death-hint";
  const ownedKey = (liveState?.ownedItemIds ?? []).join(",");
  const enemyKey = enemyTeam.join(",");
  const goldKey = Math.floor(currentGold / 50);
  useEffect(() => {
    if (!wantsPlan) return;
    let active = true;
    tauriGetItemPlan(liveState ? {} : { heroName: effectiveHero, currentGold, enemyNames: enemyTeam })
      .then((plan) => {
        if (active) setItemPlan(plan);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantsPlan, db, effectiveHero, goldKey, ownedKey, enemyKey]);

  if (kind === "augment" && (!liveState?.liveGameReady || candidateAugments.length !== 3)) {
    return <div className="overlay-card">等待游戏中的海克斯三选一界面</div>;
  }

  if (kind === "augment") {
    return (
      <div className="overlay-card overlay-augment">
        <div className="overlay-header">
          <span>
            <Sparkles size={14} className="gold-text" /> 局内海克斯三选一悬浮窗 · {effectiveHero}
          </span>
          <span className="overlay-badge">Patch {db.patch}</span>
        </div>
        <div className="overlay-3col">
          {augmentCards.map((c) => (
            <div
              key={c.slot}
              className={`overlay-aug-slot ${c.is_best ? "best" : ""}`}
            >
              <div className="overlay-aug-head">
                <strong>{c.augment_name}</strong>
                {c.is_best && (
                  <span className="best-crown-badge">
                    <Crown size={11} /> 首选
                  </span>
                )}
              </div>
              <div className="overlay-aug-score" style={{ color: c.hex_tier_color }}>
                {c.hex_label} · {c.hex_score.toFixed(1)}
              </div>
              <div className="overlay-aug-meta">
                {c.sample_size > 0
                  ? `胜率 ${(c.win_rate * 100).toFixed(1)}% · 净胜率 ${c.delta_win_rate >= 0 ? "+" : ""}${(c.delta_win_rate * 100).toFixed(1)}%`
                  : "暂无胜率样本"}
              </div>
              {c.synergy_note && <div className="overlay-note ok">{c.synergy_note}</div>}
              {c.conflict_note && <div className="overlay-note warn">{c.conflict_note}</div>}
              <button
                type="button"
                className="hex-btn ghost sm"
                onClick={() => {
                  setSelectionError("");
                  tauriConfirmOcrAugment(c.augment_name).catch((err) => setSelectionError(String(err)));
                }}
              >
                标记已选
              </button>
            </div>
          ))}
        </div>
        {selectionError && <div role="alert" className="overlay-note warn">{selectionError}</div>}
      </div>
    );
  }

  if (kind === "item") {
    return (
      <div className="overlay-card overlay-item">
        <div className="overlay-header">
          <span>
            <Crosshair size={14} className="gold-text" /> 实时流派 & 针对出装 · {effectiveHero}
          </span>
          <span className="overlay-badge">
            {counter.lean === "physical" ? "敌偏物理" : counter.lean === "magic" ? "敌偏魔法" : "均衡"}
          </span>
        </div>
        {formula && (
          <div className="overlay-row">
            {formula.coreItems.slice(0, 5).map((it) => (
              <div key={it.id} className="overlay-item-pill">
                <img
                  src={getItemIconUrl(it.id)}
                  alt={it.name}
                  onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                />
                <span>{it.name}</span>
              </div>
            ))}
          </div>
        )}
        {itemPlan && itemPlan.steps.length > 0 && (
          <div className="overlay-plan">
            {itemPlan.steps
              .filter((st) => st.kind !== "starter" || !st.owned)
              .map((st, i) => (
                <span
                  key={`${st.itemId}-${i}`}
                  className={`overlay-plan-step ${st.owned ? "done" : ""} ${
                    itemPlan.nextItem?.itemId === st.itemId && !st.owned ? "next" : ""
                  } ${st.kind}`}
                  title={st.reason}
                >
                  {st.itemName}
                  {!st.owned && <em>{st.remainingCost}g</em>}
                </span>
              ))}
          </div>
        )}
        {itemPlan && <div className="overlay-note ok">{itemPlan.note}</div>}
        <div className="overlay-sub-list">
          {counter.antiHeal && (
            <div className="overlay-note warn">
              重伤推荐：{counter.antiHeal.itemName} (+{(counter.antiHeal.lift * 100).toFixed(1)}%)
            </div>
          )}
          {counter.boots && (
            <div className="overlay-note ok">
              鞋靴推荐：{counter.boots.itemName} (+{(counter.boots.lift * 100).toFixed(1)}%)
            </div>
          )}
        </div>
      </div>
    );
  }

  if (kind === "champ-select") {
    return (
      <div className="overlay-card overlay-champ-select">
        <div className="overlay-header">
          <span>
            <Users size={14} className="gold-text" /> 选人阵容 & 替补席增益
          </span>
          <span className="overlay-badge">
            胜率 {(lineup.adjustedWin * 100).toFixed(1)}%
          </span>
        </div>
        <div className="overlay-bench-list">
          {lineup.benchSuggestions.map((b) => (
            <div key={b.heroName} className="overlay-bench-row">
              <img
                src={getChampionIconUrl(b.heroId)}
                alt={b.heroName}
                className="hero-avatar-sm"
                onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
              />
              <span className="overlay-bench-name">{b.heroName}</span>
              <span className="tier-pill" style={{ background: b.hexColor, color: "#fff" }}>
                {b.hexLabel} {b.hexScore.toFixed(0)}
              </span>
              <span className={b.gain >= 0 ? "pos-text" : "neg-text"}>
                {b.gain >= 0 ? "+" : ""}
                {(b.gain * 100).toFixed(1)}%
              </span>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (kind === "player-scores") {
    return (
      <div className="overlay-card overlay-scores">
        <div className="overlay-header">
          <span>
            <Swords size={14} className="gold-text" /> 10 人实时战力条 (HexScore)
          </span>
          <span className="overlay-badge">蓝队 vs 红队</span>
        </div>
        <div className="overlay-2col">
          <div>
            {allyTeam.map((h, i) => {
              const ch = db.champions.find((c) => c.hero === h);
              const score = [84.2, 76.5, 71.0, 68.4, 73.1][i] || 70;
              const tier = getTierMeta(score);
              return (
                <div key={h} className="overlay-score-row">
                  <img
                    src={getChampionIconUrl(ch?.id || "22")}
                    alt={h}
                    className="hero-avatar-xs"
                    onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                  />
                  <span>{h}</span>
                  <span style={{ color: tier.color, marginLeft: "auto", fontWeight: 700 }}>
                    {tier.label} {score.toFixed(1)}
                  </span>
                </div>
              );
            })}
          </div>
          <div>
            {enemyTeam.map((h, i) => {
              const ch = db.champions.find((c) => c.hero === h);
              const score = [81.0, 69.2, 74.8, 64.5, 66.0][i] || 68;
              const tier = getTierMeta(score);
              return (
                <div key={h} className="overlay-score-row">
                  <img
                    src={getChampionIconUrl(ch?.id || "157")}
                    alt={h}
                    className="hero-avatar-xs"
                    onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                  />
                  <span>{h}</span>
                  <span style={{ color: tier.color, marginLeft: "auto", fontWeight: 700 }}>
                    {tier.label} {score.toFixed(1)}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    );
  }

  // Only a plan for the hero on screen: never last game's, or another hero's.
  const plan = itemPlan && itemPlan.heroName === effectiveHero ? itemPlan : null;
  return (
    <div className="overlay-card overlay-death-hint">
      <div className="overlay-header">
        <span>
          <ShoppingBag size={14} className="gold-text" /> 出装购物推荐
        </span>
        <span className="overlay-header-end">
          <span className="overlay-badge">当前金币 {Math.round(currentGold).toLocaleString()}g</span>
          <button
            type="button"
            className="overlay-close"
            title="收起"
            aria-label="收起出装推荐"
            onClick={() => tauriToggleOverlay("death-hint").catch(() => {})}
          >
            <X size={13} aria-hidden="true" />
          </button>
        </span>
      </div>
      {!effectiveHero ? (
        <div className="overlay-note">等待对局数据…</div>
      ) : plan && plan.steps.length > 0 ? (
        <>
          <div className="overlay-note ok">
            {plan.buyNow.length > 0
              ? `现在买：${plan.buyNow.map((b) => `${b.itemName}(${b.cost}g)`).join(" → ")}`
              : plan.note}
          </div>
          {plan.buyNow.length > 0 && (
            <div className="overlay-note">
              买完剩 {plan.goldAfter}g · {plan.note}
            </div>
          )}
        </>
      ) : (
        <div className="overlay-note ok">
          优先合成：{counter.antiHeal?.itemName || formula?.coreItems?.[0]?.name || "核心大件"} ·
          剩余金币可补真眼/合剂
        </div>
      )}
    </div>
  );
};
