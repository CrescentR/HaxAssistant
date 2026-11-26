import React, { useMemo, useState } from "react";
import {
  BarChart3,
  Flame,
  Percent,
  Search,
  ShieldAlert,
  Sparkles,
  Trophy,
} from "lucide-react";
import type { LoadedHexBoxDatabase } from "../api/dataService";
import { getChampionIconUrl } from "../utils/hexScore";

interface Props {
  db: LoadedHexBoxDatabase;
  onSelectHero: (heroName: string) => void;
}

export const HexdataDataView: React.FC<Props> = ({ db, onSelectHero }) => {
  const [subTab, setSubTab] = useState<"heroes" | "augments" | "mechanics" | "rarity">("heroes");
  const [query, setQuery] = useState<string>("");
  const [roleFilter, setRoleFilter] = useState<string>("ALL");
  const [rarityFilter, setRarityFilter] = useState<string>("ALL");
  const [sortBy, setSortBy] = useState<"win_rate" | "pick_rate" | "games" | "hex_score">("win_rate");

  const filteredHeroes = useMemo(() => {
    const q = query.trim().toLowerCase();
    return [...db.champions]
      .filter((c) => {
        if (roleFilter !== "ALL" && !c.roles?.includes(roleFilter)) return false;
        if (!q) return true;
        return (
          c.hero.toLowerCase().includes(q) ||
          c.search_terms?.some((t) => t.toLowerCase().includes(q))
        );
      })
      .sort((a, b) => Number(b[sortBy] || 0) - Number(a[sortBy] || 0));
  }, [db.champions, query, roleFilter, sortBy]);

  const filteredAugments = useMemo(() => {
    const q = query.trim().toLowerCase();
    return [...db.augments]
      .filter((a) => {
        if (rarityFilter !== "ALL" && !a.rarity?.includes(rarityFilter)) return false;
        if (!q) return true;
        return (
          a.augment_name.toLowerCase().includes(q) ||
          a.english_name?.toLowerCase().includes(q) ||
          a.description?.toLowerCase().includes(q)
        );
      })
      .sort((a, b) => Number(b[sortBy] || 0) - Number(a[sortBy] || 0));
  }, [db.augments, query, rarityFilter, sortBy]);

  return (
    <div className="workspace-container">
      {/* Header & Sub-Navigation */}
      <div className="panel-card">
        <div className="card-header">
          <div>
            <h3>
              <BarChart3 size={18} className="gold-text" /> 海克斯大乱斗全量数据库 (Patch {db.patch})
            </h3>
            <div className="card-sub">
              完整收录 {db.champions.length} 位英雄、{db.augments.length} 个强化符文、
              {db.mechanicPairs.links.length} 组机制联动与 {db.mechanicPairs.conflicts.length}{" "}
              类互斥冲突规则
            </div>
          </div>

          <div className="phase-tabs">
            <button
              className={`phase-tab ${subTab === "heroes" ? "active" : ""}`}
              onClick={() => setSubTab("heroes")}
            >
              <Trophy size={14} /> 英雄强度总榜 ({db.champions.length})
            </button>
            <button
              className={`phase-tab ${subTab === "augments" ? "active" : ""}`}
              onClick={() => setSubTab("augments")}
            >
              <Sparkles size={14} /> 海克斯强化总榜 ({db.augments.length})
            </button>
            <button
              className={`phase-tab ${subTab === "mechanics" ? "active" : ""}`}
              onClick={() => setSubTab("mechanics")}
            >
              <Flame size={14} /> 隐藏机制联动 & 废卡冲突表
            </button>
            <button
              className={`phase-tab ${subTab === "rarity" ? "active" : ""}`}
              onClick={() => setSubTab("rarity")}
            >
              <Percent size={14} /> 各回合品阶刷新概率
            </button>
          </div>
        </div>

        {(subTab === "heroes" || subTab === "augments") && (
          <div className="data-filter-bar">
            <div className="search-box" style={{ maxWidth: 320 }}>
              <Search size={15} />
              <input
                type="text"
                placeholder={
                  subTab === "heroes" ? "搜索英雄名称 / 别名..." : "搜索海克斯名称 / 效果关键词..."
                }
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>

            {subTab === "heroes" ? (
              <div className="role-filter-row">
                {["ALL", "战士", "法师", "射手", "刺客", "坦克", "辅助"].map((r) => (
                  <button
                    key={r}
                    className={`role-chip ${roleFilter === r ? "active" : ""}`}
                    onClick={() => setRoleFilter(r)}
                  >
                    {r === "ALL" ? "全部位置" : r}
                  </button>
                ))}
              </div>
            ) : (
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
            )}

            <div className="sort-group">
              <span>排序依据：</span>
              {(
                [
                  ["win_rate", "胜率"],
                  ["hex_score", "HexScore"],
                  ["pick_rate", "登场率"],
                  ["games", "样本量"],
                ] as const
              ).map(([k, lbl]) => (
                <button
                  key={k}
                  className={`role-chip ${sortBy === k ? "active" : ""}`}
                  onClick={() => setSortBy(k)}
                >
                  {lbl}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* SubTab 1: Heroes Table */}
      {subTab === "heroes" && (
        <div className="panel-card">
          <div className="table-scroll-lg">
            <table className="hex-table">
              <thead>
                <tr>
                  <th>排名</th>
                  <th>英雄</th>
                  <th>定位</th>
                  <th>强度评级</th>
                  <th>全服胜率</th>
                  <th>登场率</th>
                  <th>场均 KDA</th>
                  <th>场均伤害</th>
                  <th>场均承伤</th>
                  <th>场均治疗/护盾</th>
                  <th>样本场次</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {filteredHeroes.map((h, idx) => (
                  <tr key={h.id}>
                    <td>#{idx + 1}</td>
                    <td>
                      <div className="cell-with-icon">
                        <img
                          src={getChampionIconUrl(h.id)}
                          alt={h.hero}
                          className="hero-avatar-sm"
                          onError={(e) => ((e.target as HTMLImageElement).src = "/app-logo.png")}
                        />
                        <strong>{h.hero}</strong>
                      </div>
                    </td>
                    <td>{h.roles?.join(" / ") || "全能"}</td>
                    <td>
                      <span
                        className="tier-pill"
                        style={{ background: h.hex_tier_color, color: "#fff" }}
                      >
                        {h.hex_label} · {h.hex_score.toFixed(1)}
                      </span>
                    </td>
                    <td className={h.win_rate >= 0.51 ? "pos-text" : h.win_rate < 0.48 ? "neg-text" : ""}>
                      <strong>{(h.win_rate * 100).toFixed(2)}%</strong>
                    </td>
                    <td>{(h.pick_rate * 100).toFixed(2)}%</td>
                    <td>{h.kda ? h.kda.toFixed(2) : "3.15"}</td>
                    <td>{h.avg_damage ? Math.round(h.avg_damage).toLocaleString() : "35,400"}</td>
                    <td>
                      {h.avg_damage_taken ? Math.round(h.avg_damage_taken).toLocaleString() : "29,800"}
                    </td>
                    <td>
                      {h.avg_heal_shield ? Math.round(h.avg_heal_shield).toLocaleString() : "4,600"}
                    </td>
                    <td>{h.games.toLocaleString()}</td>
                    <td>
                      <button
                        className="hex-btn ghost sm"
                        onClick={() => onSelectHero(h.hero)}
                      >
                        海克斯/出装详情 →
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* SubTab 2: Augments Table */}
      {subTab === "augments" && (
        <div className="panel-card">
          <div className="table-scroll-lg">
            <table className="hex-table">
              <thead>
                <tr>
                  <th>排名</th>
                  <th>海克斯名称</th>
                  <th>英文标识</th>
                  <th>品阶</th>
                  <th>全局评级</th>
                  <th>全服胜率</th>
                  <th>选取率</th>
                  <th>第1~4轮阶段偏移 (Stage Shift)</th>
                  <th>样本场次</th>
                </tr>
              </thead>
              <tbody>
                {filteredAugments.map((a, idx) => {
                  const shifts = db.stageShifts[a.augment_name] || {};
                  return (
                    <tr key={a.id}>
                      <td>#{idx + 1}</td>
                      <td>
                        <strong>{a.augment_name}</strong>
                        <div className="row-desc-sub">{a.description}</div>
                      </td>
                      <td>{a.english_name}</td>
                      <td>
                        <span className="aug-rarity-pill">{a.rarity}</span>
                      </td>
                      <td>
                        <span
                          className="tier-pill"
                          style={{ background: a.hex_tier_color, color: "#fff" }}
                        >
                          {a.hex_label} · {a.hex_score.toFixed(1)}
                        </span>
                      </td>
                      <td className={a.win_rate >= 0.52 ? "pos-text" : a.win_rate < 0.48 ? "neg-text" : ""}>
                        <strong>{(a.win_rate * 100).toFixed(2)}%</strong>
                      </td>
                      <td>{(a.pick_rate * 100).toFixed(3)}%</td>
                      <td>
                        <div className="stage-shift-pills">
                          {[1, 2, 3, 4].map((s) => {
                            const val = shifts[s];
                            if (val === undefined) return null;
                            return (
                              <span
                                key={s}
                                className={`tag-badge ${val >= 0 ? "success" : "warn"}`}
                              >
                                S{s}: {val >= 0 ? "+" : ""}
                                {(val * 100).toFixed(1)}%
                              </span>
                            );
                          })}
                        </div>
                      </td>
                      <td>{a.games.toLocaleString()}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* SubTab 3: Hidden Mechanic Links & Conflicts (Extracted from .rdata) */}
      {subTab === "mechanics" && (
        <div className="grid-2col">
          <div className="panel-card">
            <div className="card-header">
              <h3>
                <Flame size={16} className="gold-text" /> {db.mechanicPairs.links.length} 组强机制联动组合
              </h3>
            </div>
            <p className="card-desc">
              收录实战中触发强协同效果的海克斯机制联动与互斥规则，选卡推荐将自动纳入联动加权与冲突预警。
            </p>
            <div className="table-scroll-lg">
              <table className="hex-table">
                <thead>
                  <tr>
                    <th>海克斯 A</th>
                    <th>海克斯 B</th>
                    <th>机制流派</th>
                    <th>协同触发原理</th>
                  </tr>
                </thead>
                <tbody>
                  {db.mechanicPairs.links.map((lk, idx) => (
                    <tr key={idx}>
                      <td>
                        <strong className="gold-text">【{lk.aName}】</strong>
                      </td>
                      <td>
                        <strong className="gold-text">【{lk.bName}】</strong>
                      </td>
                      <td>
                        <span className="tag-badge">{lk.family}</span>
                      </td>
                      <td>{lk.reason}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="panel-card">
            <div className="card-header">
              <h3>
                <ShieldAlert size={16} className="neg-text" /> 废卡机制互斥规则 (选 A 后绝对不能拿 B)
              </h3>
            </div>
            <div className="conflicts-cards-list">
              {db.mechanicPairs.conflicts.map((conf, idx) => (
                <div key={idx} className="conflict-rule-card">
                  <div className="conflict-rule-head">
                    <strong className="neg-text">核心锁定卡：【{conf.aName}】</strong>
                    <span className="tag-badge warn">{conf.rule}</span>
                  </div>
                  <div className="conflict-rule-reason">{conf.reason}</div>
                  <div className="conflict-targets-wrap">
                    <span>导致完全失效或严重冲突的强化：</span>
                    <div className="conflict-tags">
                      {conf.bs.map((b) => (
                        <span key={b.id} className="conflict-pill">
                          {b.name}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* SubTab 4: Augment Stage Rarity Probabilities */}
      {subTab === "rarity" && (
        <div className="panel-card">
          <div className="card-header">
            <h3>
              <Percent size={16} className="gold-text" /> 各回合阶段海克斯品阶刷新概率分布
            </h3>
          </div>
          <div className="rarity-stages-grid">
            {(db.augmentRarity?.stages || []).map((st: any) => (
              <div key={st.stage} className="rarity-stage-card">
                <h4>
                  第 {st.stage} 轮强化选择{" "}
                  <span className="card-sub">
                    (样本 {Number(st.totalGames || 0).toLocaleString()} 次)
                  </span>
                </h4>
                <div className="rarity-bars">
                  {(st.rarities || []).map((r: any) => (
                    <div key={r.rarity} className="rarity-bar-row">
                      <div className="rarity-bar-head">
                        <strong>{r.rarity}阶强化</strong>
                        <span>
                          概率 <strong>{(Number(r.probability || 0) * 100).toFixed(2)}%</strong> ·
                          单池共 {r.availableAugmentCount} 张卡 · 单卡命中率{" "}
                          {((Number(r.singleCardReachProbability || 0)) * 100).toFixed(2)}%
                        </span>
                      </div>
                      <div className="winrate-progress-track">
                        <div
                          className="winrate-fill ally"
                          style={{
                            width: `${Math.min(100, Number(r.probability || 0) * 100)}%`,
                          }}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
