import React, { useMemo, useState } from "react";
import { BookOpen, Search, Shield, Sparkles, Swords } from "lucide-react";
import type { LoadedHexBoxDatabase } from "../api/dataService";
import { getChampionIconUrl, getItemIconUrl, getTierMeta } from "../utils/hexScore";

interface RecommendViewProps {
  db: LoadedHexBoxDatabase;
  heroName: string;
  onSelectHero: (heroName: string) => void;
}

export const RecommendView: React.FC<RecommendViewProps> = ({
  db,
  heroName,
  onSelectHero,
}) => {
  const [rarityFilter, setRarityFilter] = useState<string>("ALL");
  const [searchText, setSearchText] = useState<string>("");

  const heroObj = useMemo(
    () => db.champions.find((c) => c.hero === heroName) || db.champions[0],
    [db.champions, heroName]
  );

  const activeName = heroObj?.hero || heroName;
  const formula = db.formulaItemsByHero[activeName] || db.formulaItemsByHero[heroObj?.id || ""];
  const skillOrder = db.skillOrdersByHeroId[heroObj?.id || ""];
  const heroAugs = db.heroAugmentStats[activeName] || [];

  const filteredAugs = useMemo(() => {
    const q = searchText.trim().toLowerCase();
    return heroAugs.filter((row) => {
      if (rarityFilter !== "ALL" && !row.rarity.includes(rarityFilter)) return false;
      if (!q) return true;
      return row.augment_name.toLowerCase().includes(q);
    });
  }, [heroAugs, rarityFilter, searchText]);

  return (
    <div className="workspace-container">
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <BookOpen size={18} className="gold-text" /> 单英雄海克斯与公式出装详情库
            </h3>
            <div className="card-sub">
              Patch {db.patch} · 覆盖 {db.champions.length} 位英雄的海克斯胜率梯度、核心公式装与加点路线
            </div>
          </div>

          <select
            className="hex-select"
            value={activeName}
            onChange={(e) => onSelectHero(e.target.value)}
          >
            {db.champions.map((c) => (
              <option key={c.id} value={c.hero}>
                {c.hero} · {c.hex_label} ({(c.win_rate * 100).toFixed(1)}%)
              </option>
            ))}
          </select>
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
                  定位：{heroObj.roles.join(" / ")} · 胜率{" "}
                  <strong>{(heroObj.win_rate * 100).toFixed(2)}%</strong> · 登场率{" "}
                  <strong>{(heroObj.pick_rate * 100).toFixed(2)}%</strong> · 样本{" "}
                  <strong>{heroObj.games.toLocaleString()}</strong> 场
                </div>
                {skillOrder && (
                  <div className="skill-order-badge">
                    <Swords size={13} /> 推荐加点：
                    <strong>
                      主 {skillOrder.order[0]} 副 {skillOrder.order[1]}
                    </strong>{" "}
                    ({skillOrder.seq}) · 胜率 {(skillOrder.win_rate * 100).toFixed(1)}%
                  </div>
                )}
              </div>
            </div>

            {formula && (
              <div className="formula-mini-section">
                <div className="section-subtitle">
                  <Shield size={13} className="gold-text" /> {formula.label || "公式出装"}（出门装 → 核心神装）：
                </div>
                <div className="item-icon-row">
                  {formula.starterItems.map((it, i) => (
                    <div key={`st-${i}`} className="item-chip">
                      <img
                        src={getItemIconUrl(it.id)}
                        alt={it.name}
                        onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                      />
                      <span>{it.name}</span>
                    </div>
                  ))}
                  <span className="arrow-sep">→</span>
                  {formula.coreItems.map((it, i) => (
                    <div key={`co-${i}`} className="item-chip core">
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
          </div>
        )}
      </div>

      <div className="panel-card">
        <div className="card-header">
          <h3>
            <Sparkles size={16} className="gold-text" /> {activeName} 适配海克斯梯度表 ({filteredAugs.length})
          </h3>
          <div className="data-filter-bar compact">
            <div className="search-box" style={{ maxWidth: 240 }}>
              <Search size={14} />
              <input
                type="text"
                placeholder="筛选海克斯名称..."
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
              />
            </div>
            <div className="role-filter-row">
              {["ALL", "棱彩", "黄金", "白银"].map((r) => (
                <button
                  key={r}
                  className={`role-chip ${rarityFilter === r ? "active" : ""}`}
                  onClick={() => setRarityFilter(r)}
                >
                  {r === "ALL" ? "全部品阶" : r}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="table-scroll-lg">
          <table className="hex-table">
            <thead>
              <tr>
                <th>排名</th>
                <th>海克斯名称</th>
                <th>品阶</th>
                <th>适配评级</th>
                <th>搭配胜率</th>
                <th>净胜率差</th>
                <th>样本量</th>
                <th>联动核心装备</th>
              </tr>
            </thead>
            <tbody>
              {filteredAugs.map((row, idx) => {
                const t = getTierMeta(row.hex_score);
                return (
                  <tr key={`${row.augment_id}-${idx}`}>
                    <td>#{idx + 1}</td>
                    <td>
                      <strong>{row.augment_name}</strong>
                    </td>
                    <td>
                      <span className="aug-rarity-pill">{row.rarity}</span>
                    </td>
                    <td>
                      <span className="tier-pill" style={{ background: t.bg, color: t.color }}>
                        {t.label} · {row.hex_score.toFixed(1)}
                      </span>
                    </td>
                    <td className={row.win_rate >= 0.52 ? "pos-text" : row.win_rate < 0.48 ? "neg-text" : ""}>
                      <strong>{(row.win_rate * 100).toFixed(2)}%</strong>
                    </td>
                    <td className={row.delta_win_rate >= 0 ? "pos-text" : "neg-text"}>
                      {row.delta_win_rate >= 0 ? "+" : ""}
                      {(row.delta_win_rate * 100).toFixed(2)}%
                    </td>
                    <td>{row.games.toLocaleString()}</td>
                    <td>
                      <div className="mini-items-list">
                        {(row.top_items || []).slice(0, 3).map((it) => (
                          <div key={String(it.item_id)} className="mini-item-pill">
                            <img
                              src={getItemIconUrl(it.item_id)}
                              alt={it.item_name}
                              onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                            />
                            <span>{it.item_name}</span>
                          </div>
                        ))}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
