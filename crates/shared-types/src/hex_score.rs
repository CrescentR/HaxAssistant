use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TierMeta {
    pub tier: String,
    pub label: String,
    pub color: String,
    pub bg: String,
}

/// Five-tier HexScore classification matching `apps/desktop/src/utils/hexScore.ts`.
pub fn get_tier_meta(score: f64) -> TierMeta {
    if score >= 80.0 {
        TierMeta {
            tier: "hang".to_string(),
            label: "夯".to_string(),
            color: "#c41e3a".to_string(),
            bg: "rgba(196, 30, 58, 0.18)".to_string(),
        }
    } else if score >= 68.0 {
        TierMeta {
            tier: "top".to_string(),
            label: "顶级".to_string(),
            color: "#e8751a".to_string(),
            bg: "rgba(232, 117, 26, 0.18)".to_string(),
        }
    } else if score >= 55.0 {
        TierMeta {
            tier: "elite".to_string(),
            label: "人上人".to_string(),
            color: "#b08a2d".to_string(),
            bg: "rgba(176, 138, 45, 0.18)".to_string(),
        }
    } else if score >= 42.0 {
        TierMeta {
            tier: "npc".to_string(),
            label: "NPC".to_string(),
            color: "#5a7a8a".to_string(),
            bg: "rgba(90, 122, 138, 0.18)".to_string(),
        }
    } else {
        TierMeta {
            tier: "trap".to_string(),
            label: "拉完了".to_string(),
            color: "#7a3aa8".to_string(),
            bg: "rgba(122, 58, 168, 0.18)".to_string(),
        }
    }
}
