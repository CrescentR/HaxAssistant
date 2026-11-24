# HaxAssistant (HexBox) 损坏文件清单与重建开发指南

> 生成时间：2026-11-24  
> 适用分支：`main`（当前完好基线提交 `6cce12e`）  
> 文档目的：汇总当前工作区中所有已损坏/缺失的文件，并基于仓库内**仍然完好的源码与数据**反推出各模块的接口契约、依赖版本与推荐重建顺序，便于直接继续开发。

---

## 1. 损坏总览

当前工作区 `/home/crescent/code/Rea` 遭受过文件系统级数据损坏（多个源码与模型文件被 Windows MSVC Rust `.pdb`/`.rlib` 调试符号及随机二进制片段覆盖）。

| 模块 | 完好文件 | 损坏 / 缺失文件 | 影响评估 |
| --- | --- | --- | --- |
| **前端桌面端 (`apps/desktop/`)** | `package-lock.json`, `dataService.ts`, `HexdataDataView.tsx`, `OngoingWorkspace.tsx`, `wght-D2EsC2yI.css`, `src-tauri/main.rs`, `src-tauri/tauri.conf.json` | 工程配置 5 个、核心入口/类型/工具 5 个、视图 4 个、样式 10 个、`dist/` 全部 | **核心业务逻辑与两张主视图仍在**，补齐配置、类型、入口与样式后可最快跑起前端 |
| **OCR Sidecar (`ocr-sidecar/`)** | `hexbox_ocr.py`, `service.py`, `card_locator.py`, `augment_pipeline.py`, `requirements.txt`, `sample_augment_1080p.png` | `worker.py` (1 个) | **主干识别/定位/匹配代码全完好**，仅需按既定接口重写 `worker.py` |
| **Rust 后端 (`Cargo.toml`, `crates/`)** | 仅 `apps/desktop/src-tauri/src/main.rs` 完好 | 根 `Cargo.toml`/`Cargo.lock`、`reco-engine`、`data-runtime`、`data-updater` 全损坏；`shared-types`、`lcu-client`、`app-core` 为空目录 | 需按 Workspace 结构重新搭建 |
| **数据与同步脚本 (`scripts/`, `data/`)** | 66 个 JSON（含 `augments_full.json`, `hero_formula_items.json`, `augment_mechanic_pairs_v1.json`, `augment_stage_value_v1.json`, `standard-heroes/*.json` 等） | `scripts/sync_hexdata.py` 损坏；`data/` 下 27 个 JSON 损坏（含 `champions.json`, `hero_augment_stats.json` 等） | 需重建或重新拉取 8 个核心 JSON 与 19 个英雄装备 JSON |
| **ONNX 模型 (`models/`)** | `own-card-detector.json` 与 3 个 `*_keys.txt` 字典完好 | 4 个 `.onnx` 模型全部损坏 | 需重新获取或替换模型权重（PaddleOCR 可从官方重新导出，`own-card-detector.onnx` 需找回原文件或降级为预设坐标） |
| **静态资源 (`apps/desktop/public/`)** | PNG 1158 个、WOFF2 46 个完好 | PNG 1441 个、WOFF2 169 个损坏 | 英雄/装备图标可改走在线/本地 DataDragon，字体可改用系统字体或 CDN |

---

## 2. 建议重建开发顺序（P0 → P3）

1. **P0 — 让前端开发服务器先跑起来（最快看到界面）**
   - 重建 `apps/desktop/package.json`、`tsconfig.json`、`vite.config.ts`、`index.html`、`overlay.html`
   - 重建 `apps/desktop/src/types/hexbox.ts`、`apps/desktop/src/utils/hexScore.ts`
   - 重建 `apps/desktop/src/main.tsx`、`src/overlay.tsx`、`src/overlays/OverlayApp.tsx` 与 `src/styles/replica.css`
   - 准备最小可用的 `data/champions.json` 与 `data/hero_augment_stats.json`（先让 `OngoingWorkspace` 与 `HexdataDataView` 渲染出数据）
2. **P1 — 补全其余 4 个前端页面**
   - `RecommendView.tsx`、`SettingsHubView.tsx`、`PersonalHistoryView.tsx`、`KaiheiView.tsx`
3. **P2 — 恢复 OCR Sidecar 闭环**
   - 按接口重写 `ocr-sidecar/worker.py`
   - 替换损坏的 `models/*.onnx`（至少替换 `ch_PP-OCRv4_rec_int8.onnx` 或 `PP-OCRv5_mobile_rec.onnx`；若暂时无 `own-card-detector.onnx`，先支持预设坐标模式 `augment_presets.json`）
4. **P3 — 重建 Rust Workspace、LCU 联动与数据同步**
   - 重建根 `Cargo.toml` 及 `crates/*`
   - 重建 `scripts/sync_hexdata.py` 恢复 27 个损坏的数据文件
   - 将 `apps/desktop/src-tauri/src/main.rs` 升级为完整 Tauri v2 Command/Window 宿主

---

## 3. 分模块损坏清单与重建规格

### 3.1 前端工程配置与 HTML 入口 (`apps/desktop/`)

#### 损坏文件清单
- `apps/desktop/package.json`
- `apps/desktop/tsconfig.json`
- `apps/desktop/vite.config.ts`
- `apps/desktop/index.html`
- `apps/desktop/overlay.html`
- `apps/desktop/dist/*`（构建产物，无需恢复，`npm run build` 自动生成）

#### 从完好文件反推的重建规格
1. **`package.json`**（从完好的 `apps/desktop/package-lock.json` 与 `src-tauri/tauri.conf.json` 精确还原）：
   - `name`: `"hexbox-desktop-replica"`，`version`: `"1.1.0"`
   - `scripts`: 至少包含 `"dev": "vite"`, `"build": "tsc && vite build"`, `"preview": "vite preview"`（`tauri.conf.json` 中配置了 `beforeDevCommand: "npm run dev"`, `devUrl: "http://localhost:1420"`, `beforeBuildCommand: "npm run build"`, `frontendDist: "../dist"`）
   - `dependencies`: `lucide-react@^0.460.0`, `react@^18.3.1`, `react-dom@^18.3.1`
   - `devDependencies`: `@types/node@^22.0.0`, `@types/react@^18.3.12`, `@types/react-dom@^18.3.1`, `@vitejs/plugin-react@^4.3.3`, `typescript@^5.6.3`, `vite@^5.4.11`
2. **`vite.config.ts`**（从 `tauri.conf.json`、`dataService.ts`、`OngoingWorkspace.tsx` 反推）：
   - 监听端口 `1420`（`strictPort: true`）
   - 多入口打包：`index.html`（主窗口）与 `overlay.html`（悬浮窗）
   - 开发期静态中间件：将 `/data/*` 映射到仓库根目录 `../../data/`（`dataService.ts` 通过 `fetch("/data/xxx.json")` 加载数据）
   - 开发期 OCR API 中间件：处理 `GET /api/ocr/run?preset=...&image=...`（见 `OngoingWorkspace.tsx:104-117`），调用 `ocr-sidecar/hexbox_ocr.py preset --preset <preset> --image <image>` 并返回 JSON
3. **`index.html` / `overlay.html`**：
   - `index.html` 挂载 `#root` 并引入 `/src/main.tsx`
   - `overlay.html` 挂载 `#root` 并引入 `/src/overlay.tsx`（`tauri.conf.json` 通过 `?overlay=augment|item|champ-select|player-scores|death-hint` 区分 5 种透明悬浮窗）

---

### 3.2 前端核心类型与工具 (`apps/desktop/src/types/` & `utils/`)

#### 损坏文件清单
- `apps/desktop/src/types/hexbox.ts`
- `apps/desktop/src/utils/hexScore.ts`

#### `src/types/hexbox.ts` 必需导出类型（从 `dataService.ts` 完整反推）
- `AppSettings`：
  ```ts
  export interface AppSettings {
    appearance_mode: "dark" | "light";
    augment_hotkey: string;
    item_overlay_hotkey: string;
    auto_show_overlay: boolean;
    auto_show_formula_item_overlay: boolean;
    player_scores_overlay_enabled: boolean;
    death_shop_hint_enabled: boolean;
    bench_quick_grab_enabled: boolean;
    bench_autograb_enabled: boolean;
    bench_preferred_champion_ids: number[];
    auto_ocr_enabled: boolean;
    ocr_performance_mode: "eco" | "balanced" | "performance";
    selected_preset_key: string;
    show_augment_stats: boolean;
    sgp_enabled: boolean;
    ready_check_auto_accept: boolean;
    overlay_opacity: number;
    overlay_zoom: number;
  }
  ```
- `ChampionSummary`：含 `id: string`, `hero: string`, `roles: string[]`, `search_terms?: string[]`, `win_rate: number`, `pick_rate: number`, `games: number`, `hex_score: number`, `hex_tier?: string`, `hex_label: string`, `hex_tier_color: string`, `kda?: number`, `avg_damage?: number`, `avg_damage_taken?: number`, `avg_heal_shield?: number`, `avg_cc_time?: number`
- `AugmentCatalogEntry`：与 `data/augments_full.json` 字段一致（`id`, `augment_id`, `augment_name`, `normalized_name`, `english_name`, `description`, `rarity`, `tier`, `games`, `win_rate`, `pick_rate`, `hex_score`, `hex_tier`, `hex_label`, `hex_tier_color`, `icon_url`）
- `HeroAugmentStatRow`：含 `augment_id`, `augment_name`, `rarity`, `hex_score`, `win_rate`, `pick_rate`, `delta_win_rate`, `games`, `top_items?: Array<{ item_id: number | string; item_name: string; win_rate: number; average_index: number }>`
- `AugmentMechanicPairs`：含 `schemaVersion`, `reviewedPatch`, `note`, `links: Array<{ aName: string; bName: string; family: string; reason: string }>`, `conflicts: Array<{ aName: string; rule: string; reason: string; bs: Array<{ id: string | number; name: string }> }>`
- `AugmentRecommendationCard`：见 `dataService.ts:193-213` 返回对象（`slot`, `hero_name`, `augment_id`, `augment_name`, `rarity`, `hex_score`, `hex_tier`, `hex_label`, `hex_tier_color`, `win_rate`, `pick_rate`, `delta_win_rate`, `sample_size`, `stage_shift`, `reason`, `synergy_note`, `conflict_note`, `top_items`, `is_best`）
- `EnemyCounterAnalysis`：见 `dataService.ts:237-247, 366-376`（`state: "waiting" | "ready"`, `lean: "physical" | "magic" | "balanced"`, `physicalShare`, `magicShare`, `trueShare`, `boots`, `antiHeal`, `defensiveItems`, `healers`）
- `FormulaItemBuild`：与 `data/hero_formula_items.json` 中 `items[]` 一致（`heroId`, `heroName`, `label`, `starterItems: Array<{ id: string; name: string }>`, `coreItems: Array<{ id: string; name: string }>`）
- `HeroItemBuildPack`：单英雄装备构建包（可根据重建后的 `hero_item_builds.json` 结构定义）
- `PlayerMarker`：见 `dataService.ts:516-544`（`puuid`, `summonerName`, `tag: "carry" | "friend" | "toxic" | string`, `tagLabel`, `note`, `encounters`, `updatedAt`）

#### `src/utils/hexScore.ts` 必需导出函数（从 `dataService.ts` / `OngoingWorkspace.tsx` / `augments_full.json` 反推）
1. `getTierMeta(score: number)`：返回 `{ tier: string; label: string; color: string; bg: string }`
   - 从 `data/augments_full.json` 提取到的精确五档分级与色值：
     | 分数区间 | `tier` | `label` | `color` |
     | --- | --- | --- | --- |
     | `>= 80` | `hang` | `夯` | `#c41e3a` |
     | `68 ~ 79.9` | `top` | `顶级` | `#e8751a` |
     | `55 ~ 67.9` | `elite` | `人上人` | `#b08a2d` |
     | `42 ~ 54.9` | `npc` | `NPC` | `#5a7a8a` |
     | `< 42` | `trap` | `拉完了` | `#7a3aa8` |
   - 注意：`OngoingWorkspace.tsx:742` 使用了 `tier.bg` 与 `tier.color`，可设 `bg` 为对应色值的低透明背景（如 `rgba(..., 0.18)`）。
2. `getChampionIconUrl(heroId: string | number): string`：返回英雄头像路径或 DataDragon / CommunityDragon URL（组件内已有 `onError` 回退到 `/hexbox-logo.png`）
3. `getItemIconUrl(itemId: string | number): string`：返回装备图标路径或 CDN URL

---

### 3.3 前端入口、视图、悬浮窗与样式 (`apps/desktop/src/`)

#### 损坏文件清单
- **入口与悬浮窗**：
  - `apps/desktop/src/main.tsx`
  - `apps/desktop/src/overlay.tsx`
  - `apps/desktop/src/overlays/OverlayApp.tsx`
- **业务视图（4 个损坏，2 个完好）**：
  - `apps/desktop/src/views/RecommendView.tsx`（单英雄海克斯/出装详情页；`OngoingWorkspace` 与 `HexdataDataView` 均通过回调跳转至此页）
  - `apps/desktop/src/views/KaiheiView.tsx`（开黑/玩家标记与车队管理页；对应 `dataService.ts` 中的 `loadPlayerMarkers` / `savePlayerMarkers`）
  - `apps/desktop/src/views/PersonalHistoryView.tsx`（个人战绩/对局历史与复盘统计）
  - `apps/desktop/src/views/SettingsHubView.tsx`（设置中心；对应 `dataService.ts` 中的 `loadSettings` / `saveSettings` / `DEFAULT_SETTINGS`）
- **样式文件（10 个损坏，仅 `wght-D2EsC2yI.css` 完好）**：
  - `apps/desktop/src/styles/replica.css`（主自定义样式，涵盖 `.workspace-container`, `.panel-card`, `.phase-tabs`, `.hex-table`, `.augment-3cards-grid`, `.verdict-seal-box`, `.winrate-progress-track`, `.roster-dual-columns` 等）
  - 以下 9 个带哈希名的 CSS 属于早期提取的资源文件，重建时建议**合并精简到 `replica.css` 或模块化 CSS 中，不必逐一恢复原哈希文件名**：
    - `main-BqJylhn9.css` (1.8MB)
    - `overlay-BhPaJb4r.css` (298KB)
    - `AugmentOverlayView-Cii5DVyw.css`
    - `ChampionSelectOverlayView-9TOqR3LO.css`
    - `HexdataDataView-DQOKDmAb.css`
    - `OneClickDiagnostics-pwyWaDK1.css`
    - `OngoingWorkspace-XD6r_Rpr.css`
    - `VerdictSeal-Do2gh23P.css`
    - `remoteSupportDiagnostics-JiO3A950.css`

#### 悬浮窗路由约定（来自 `src-tauri/tauri.conf.json`）
`OverlayApp.tsx` 读取 `window.location.search` 的 `overlay` 参数，渲染以下 5 种视图：
- `augment`：局内强化三选一悬浮窗（980×340）
- `item`：实时流派与克制出装参考（460×280）
- `champ-select`：选人阵容胜率与替补席推荐（420×540）
- `player-scores`：10 人玩家战力条（640×240）
- `death-hint`：阵亡购物提示（380×140）

---

### 3.4 OCR Sidecar (`ocr-sidecar/`)

#### 损坏文件清单
- 仅 `ocr-sidecar/worker.py`（1 个）

#### 重建规格（从完好的 `hexbox_ocr.py`、`augment_pipeline.py`、`card_locator.py`、`service.py` 和 `OngoingWorkspace.tsx` 精确反推）
1. **类 `PersistentOcrRunner`**（被 `hexbox_ocr.py:80-84` 直接导入调用）：
   - 构造：`PersistentOcrRunner(performance_mode: str = "balanced", lang: str = "ch", backend: str = "onnx")`
   - 内部持有懒加载的 `PpOcrOnnxRecognizer`（来自 `service.py`）、`CardLocator`（来自 `card_locator.py`，若 `own-card-detector.onnx` 不可用则回退到 `augment_pipeline.get_preset()` 坐标）与 `AutoWatcher`（来自 `augment_pipeline.py`）
   - 方法：`run_pipeline(preset_key: str = "1920x1080@100", screen_image_path: Optional[str] = None) -> Dict[str, Any]`
     - 步骤：加载截图（或调用 `capture_screen_image`）→ 卡片定位（`CardLocator.locate` 或 `get_preset`）→ 裁剪 3 个标题框（`crop_rect_to_image`）→ 批量推理（`recognizer.recognize_image_batch`）→ 字典模糊匹配（`match_ocr_texts`）→ 计时统计
     - 返回 JSON 需兼容 `OngoingWorkspace.tsx:108-117`：
       ```json
       {
         "status": "ok",
         "result": [
           { "slot": 1, "raw_text": "...", "match_name": "...", "match_type": "exact|alias|fuzzy|unmatched", "confidence": 0.95, "stable": true }
         ],
         "ocr_matches": [ /* 同 result，供 AutoWatcher.observe 使用 */ ],
         "timings": { "capture_ms": 0, "locate_ms": 0, "ocr_ms": 30.9, "total_ms": 35.1 }
       }
       ```
2. **函数 `main()`**（被 `hexbox_ocr.py:106-108` 调用）：
   - 常驻 Stdin/Stdout 按行 JSON-RPC 循环：逐行读取 JSON 请求（如 `{"id": 1, "method": "run_pipeline" | "watch_tick" | "ping" | "shutdown", "params": {...}}`），将结果单行 JSON 写回 stdout 并 `flush()`。

---

### 3.5 Rust Workspace 与后端 Crates (`Cargo.toml`, `crates/`)

#### 损坏与缺失清单
- **根目录（损坏）**：`Cargo.toml`、`Cargo.lock`
- **已存在但源码全部损坏的 Crates**：
  - `crates/reco-engine/`：`Cargo.toml`, `src/lib.rs`, `src/conditional_items.rs`, `src/lineup.rs`
  - `crates/data-runtime/`：`Cargo.toml`, `src/lib.rs`
  - `crates/data-updater/`：`Cargo.toml`, `src/lib.rs`
- **空目录 Crates（源码缺失）**：
  - `crates/shared-types/`
  - `crates/lcu-client/`
  - `crates/app-core/`
- **缺少 `Cargo.toml` 的 Tauri 宿主**：
  - `apps/desktop/src-tauri/Cargo.toml`（当前仅有完好的 `src/main.rs` 和 `tauri.conf.json`）

#### 各 Crate 职责与对接契约（参考 `apps/desktop/src-tauri/src/main.rs` 与 `dataService.ts`）
- `crates/shared-types`：前后端共享的 Serde 数据结构（英雄、强化、推荐卡片、设置、LCU 状态）
- `crates/data-runtime`：加载 `data/` 下的 `runtime_meta.json`, `champions.json`, `augments_full.json` 等为内存 `Dataset`
- `crates/data-updater`：校验 `data/manifest.json` 哈希并触发数据更新
- `crates/reco-engine`：
  - `lib.rs`：海克斯三选一评分（基础 HexScore + Stage Shift + 机制联动加分 + 废卡冲突降权，算法与 `dataService.ts:evaluateAugmentOffer` 对齐）
  - `conditional_items.rs`：敌方伤害占比与动态克制装备（算法与 `dataService.ts:analyzeEnemyCounter` 对齐）
  - `lineup.rs`：5v5 阵容胜率预测与替补席换人增益（算法与 `dataService.ts:evaluate5v5Lineup` 对齐）
- `crates/lcu-client`：读取 LeagueClient `lockfile` 连接本地 LCU HTTPS API（Gameflow 状态轮询、选人阶段 `POST /lol-champ-select/v1/session/bench/swap/{id}` 30ms 秒抢、自动接受对局、Live Client Data）
- `crates/app-core`：聚合上述模块，提供 `HexBoxCore::initialize(&root)`（被 `apps/desktop/src-tauri/src/main.rs:15` 调用，暴露 `core.dataset`）

---

### 3.6 数据同步脚本与损坏数据 (`scripts/` & `data/`)

#### 损坏文件清单
1. **同步脚本**：`scripts/sync_hexdata.py`
2. **`data/` 顶层损坏 JSON（8 个）**：
   - `data/champions.json`（**P0 必需**：前端英雄列表、选人/战力/克制核心依赖）
   - `data/hero_augment_stats.json`（**P0 必需**：单英雄×海克斯胜率表，原大小 ~24MB）
   - `data/hero_item_builds.json`（单英雄装备构建，原大小 ~1.3MB）
   - `data/hero_synergies_counters.json`（队友协同与敌我克制关系，原大小 ~520KB）
   - `data/hero_augment_trios.json`（海克斯三件套组合，原大小 ~262KB）
   - `data/basic_catalog.json`
   - `data/postmatch.json`（各英雄场均伤害/承伤/治疗/控制，用于克制出装计算）
   - `data/standard_heroes_summary.json`
3. **`data/hero-augment-items/` 损坏 JSON（19 个）**：
   - `2.json`, `4.json`, `5.json`, `7.json`, `8.json`, `9.json`, `10.json`, `11.json`, `14.json`, `15.json`, `17.json`, `19.json`, `21.json`, `22.json`, `23.json`, `24.json`, `26.json`, `27.json`, `29.json`
   - 注：同目录的 `index.json` 与其余 11 个英雄文件（`1`, `3`, `6`, `12`, `13`, `16`, `18`, `20`, `25`, `28` 等）完好，可作为精确 Schema 参照。
   - 另注：`data/standard-heroes/` 下的 30 个 JSON 全部完好，内含 `decisionAugments` / `items` / `trios` 等字段，若暂时无法重新爬取远端数据，可先用这些完好文件 + `augments_full.json` + `hero_formula_items.json` 生成本地开发用的降级版 `champions.json` 与 `hero_augment_stats.json`。

---

### 3.7 ONNX 模型与静态资源 (`models/` & `apps/desktop/public/`)

#### 损坏的 ONNX 模型（全部 4 个）
| 文件 | 状态 | 校验 / 处理建议 |
| --- | --- | --- |
| `models/ch_PP-OCRv4_rec_int8.onnx` | 已损坏 | 可直接从 PaddleOCR 官方重新获取 PP-OCRv4 中文识别 ONNX（配套字典 `ppocr_keys_v1.txt` 完好） |
| `models/PP-OCRv5_mobile_rec.onnx` | 已损坏 | 可从 PaddleOCR 官方重新获取 PP-OCRv5 mobile 识别 ONNX（配套字典 `PP-OCRv5_mobile_rec_keys.txt` 完好） |
| `models/korean_PP-OCRv5_mobile_rec.onnx` | 已损坏 | 仅韩服 OCR 需要，可按需从 PaddleOCR 官方补回 |
| `models/own-card-detector.onnx` | 已损坏 | `own-card-detector.json` 记录期望 SHA256 为 `8618d830...`，磁盘实际为 `894c86fa...`。若无原备份，建议在 `worker.py` 中优先支持基于 `data/augment_presets.json` 固定分辨率比例裁剪（零模型依赖），后续有需要再重新训练或接入轻量模板匹配 |

#### 损坏的前端静态资源 (`apps/desktop/public/`)
- PNG 图片：1158 个完好，**1441 个损坏**
- WOFF2 字体：46 个完好，**169 个损坏**
- **处理建议**：清理损坏的图片与字体；英雄/装备/召唤师技能图标统一在 `hexScore.ts` 中配置 Riot DataDragon CDN（或重新批量下载），字体在 CSS 中直接回退到系统无衬线中文字体。

---

## 4. 已推送到 GitHub 的完好基线清单 (`6cce12e`)

以下 15 个文件已验证完好并推送到 `origin/main`（后续开发可直接在此基础上新建分支或增量提交）：

- `.gitignore`
- `ocr-sidecar/hexbox_ocr.py`
- `ocr-sidecar/service.py`
- `ocr-sidecar/card_locator.py`
- `ocr-sidecar/augment_pipeline.py`
- `ocr-sidecar/requirements.txt`
- `ocr-sidecar/sample_augment_1080p.png`
- `models/own-card-detector.json`
- `apps/desktop/package-lock.json`
- `apps/desktop/src/api/dataService.ts`
- `apps/desktop/src/views/OngoingWorkspace.tsx`
- `apps/desktop/src/views/HexdataDataView.tsx`
- `apps/desktop/src/styles/wght-D2EsC2yI.css`
- `apps/desktop/src-tauri/tauri.conf.json`
- `apps/desktop/src-tauri/src/main.rs`
