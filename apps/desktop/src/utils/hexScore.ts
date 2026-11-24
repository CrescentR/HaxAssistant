export interface TierMeta {
  tier: "hang" | "top" | "elite" | "npc" | "trap";
  label: string;
  color: string;
  bg: string;
}

export function getTierMeta(score: number): TierMeta {
  if (score >= 80) {
    return {
      tier: "hang",
      label: "夯",
      color: "#c41e3a",
      bg: "rgba(196, 30, 58, 0.18)",
    };
  }
  if (score >= 68) {
    return {
      tier: "top",
      label: "顶级",
      color: "#e8751a",
      bg: "rgba(232, 117, 26, 0.18)",
    };
  }
  if (score >= 55) {
    return {
      tier: "elite",
      label: "人上人",
      color: "#b08a2d",
      bg: "rgba(176, 138, 45, 0.18)",
    };
  }
  if (score >= 42) {
    return {
      tier: "npc",
      label: "NPC",
      color: "#5a7a8a",
      bg: "rgba(90, 122, 138, 0.18)",
    };
  }
  return {
    tier: "trap",
    label: "拉完了",
    color: "#7a3aa8",
    bg: "rgba(122, 58, 168, 0.18)",
  };
}

const CDRAGON_BASE =
  "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default";
const DDRAGON_VERSION = "15.1.1";
const DDRAGON_BASE = `https://ddragon.leagueoflegends.com/cdn/${DDRAGON_VERSION}`;

export function getChampionIconUrl(heroId: string | number): string {
  const idStr = String(heroId || "").trim();
  if (!idStr || !/^\d+$/.test(idStr)) {
    return "/hexbox-logo.png";
  }
  return `${CDRAGON_BASE}/v1/champion-icons/${idStr}.png`;
}

export function getItemIconUrl(itemId: string | number): string {
  const idStr = String(itemId || "").trim();
  if (!idStr || !/^\d+$/.test(idStr)) {
    return "/hexbox-logo.png";
  }
  return `${DDRAGON_BASE}/img/item/${idStr}.png`;
}
