# 与原版 HexBox 1.1.0 的差距清单

> 整理时间：2026-11-25
> 对比对象：`HexBox_1.1.0_x64-setup.exe`（NSIS 安装包）与当前 `main` 分支的工作区
> 方法：用 7-Zip 解包安装器，查看目录结构和 `ocr/compiled-ocr-manifest.json`，再从主程序 `aram-assistant-app.exe` 的字符串里读出命令名、源码模块路径和设置项。没有运行原版程序，所以下面写的是"原版有这个功能的痕迹"，具体行为需要实现时再确认。

---

## 1. 本次已修复：OCR 报错 `ocr worker script not found`

### 现象

```text
OCR 调用失败：ocr worker script not found: R:\HexBox\ocr-sidecar/worker.py
```

### 原因

打包配置里没有声明任何资源文件，安装包里没有 `ocr-sidecar/`；程序却只在安装目录下找 `ocr-sidecar/worker.py`。

### 改动

| 文件 | 改动 |
| --- | --- |
| `apps/desktop/src-tauri/build.rs` | 构建时把 `data/*.json`、`models/*`、`ocr-sidecar/*.py` 复制到 `src-tauri/resources/`；源目录不存在时也会建一个空目录，保证打包不报错 |
| `apps/desktop/src-tauri/tauri.conf.json` | `bundle.resources` 把上面三个目录打进安装包 |
| `apps/desktop/src-tauri/src/main.rs` | 程序根目录依次尝试 exe 同级目录、资源目录、源码目录、当前目录；调试构建优先用源码目录 |
| `crates/app-core/src/lib.rs` | 脚本和模型在程序根目录、exe 同级目录、各自的 `resources/`、源码目录里依次查找 |
| `crates/app-core/src/ocr_worker.rs` | 找不到脚本或启动不了 Python 时，报错里写明处理办法；给子进程设置 `PYTHONIOENCODING=utf-8` |
| `.gitignore` | 忽略 `apps/desktop/src-tauri/resources/` |

可用的环境变量：

| 变量 | 作用 |
| --- | --- |
| `HEXBOX_OCR_SIDECAR_DIR` | 指定 `worker.py` 所在目录 |
| `ARAM_ASSISTANT_OCR_PYTHON` | 指定 Python 解释器 |
| `ARAM_ASSISTANT_OCR_MODEL_DIR` | 指定模型目录 |

Python 的回退命令在 Windows 上从 `python3` 改成了 `python`，因为 `python3` 在 Windows 上通常是应用商店的占位程序。

### 验证

- `cargo test --workspace` 全部通过，其中新增了一个测试：程序根目录下没有 `ocr-sidecar/` 时仍能找到 `worker.py`。
- macOS 上执行 `npm run tauri -- build --bundles app`，`HexBox.app/Contents/Resources/` 里有 `ocr-sidecar/worker.py`、模型和数据。
- Windows 上没有实测。

### 仍然存在的问题

- 目标机器上还要装 Python 3.10+ 和 `ocr-sidecar/requirements.txt` 里的依赖（见 2.1）。
- GitHub Actions 打出的包没有数据和 ONNX 模型，因为 `data/` 和 `models/*.onnx` 不在 Git 里。本地打包才是完整的。
- 已经装在 `R:\HexBox` 的旧版本要重新安装新包，或者把仓库的 `ocr-sidecar` 文件夹拷进去。

---

## 2. 尚未复现的功能

规模上，原版主程序里能读出约 130 个命令，当前有 24 个。下面按对使用的影响从大到小排列。

### 进度

| 项 | 状态 |
| --- | --- |
| 2.1 自带 OCR 可执行文件 | **已实现**（用 PyInstaller，不是 Nuitka）。训练样本模块、命令行回退、截图缓存清理、前台窗口检查没做 |
| 2.2 LCU WebSocket | **已实现事件监听**。WMIC 发现客户端、多客户端切换、事件录制没做 |
| 2.10 悬浮窗缩放与位置记忆 | **已实现**缩放、拖动、位置记忆与重置。外观预设、自适应高度、紧凑窗口等没做 |
| 2.7 出装顺序 | **已实现**出装顺序引擎和 `get_item_recommendations`。强化价值模式、出现率统计、手动改选等没做 |
| 2.9 赛后复盘 | **已实现**单局复盘和 `get_postgame_augment_review`。队伍分析、对局回执没做 |
| 其余各项 | 未开始 |

#### 2.1 的实现

- `scripts/build_ocr_runner.py` 用 PyInstaller 把 `ocr-sidecar/worker.py` 编译到仓库根目录的 `ocr/`（已加入 `.gitignore`）。
- `build.rs` 把 `ocr/` 打进安装包；程序启动时优先用 `ocr/hexbox-ocr[.exe]`，找不到才退回 Python 脚本。
- 环境变量：`HEXBOX_OCR_RUNNER` 指定可执行文件，`HEXBOX_OCR_USE_PYTHON=1` 强制用 Python。
- Windows 上启动 OCR 进程时不再弹出控制台窗口。
- CI 在打包前编译 OCR 程序。`macos-x64` 那一项除外：它在 Apple Silicon 机器上交叉编译，编出来的 OCR 程序会是 arm64，所以这个包仍然用 Python。
- 验证：macOS 上打包后，包内的 `ocr/hexbox-ocr` 能识别样例图；Windows 和 Linux 的编译只写进了 CI，没有跑过。

#### 2.2 的实现

- `crates/lcu-client/src/websocket.rs`：连接 `wss://127.0.0.1:<端口>/`，订阅 `OnJsonApiEvent`，只处理对局阶段、选人、对局确认三类事件。
- 主程序收到事件后立刻触发一次原有的状态刷新；1.5 秒轮询保留作兜底。
- 验证：帧解析有单元测试，另有一个测试连本地的测试服务器走完订阅和收事件。没有连真实客户端测过。

#### 2.7 的实现（出装顺序）

原版的算法读不出来，下面的规则是我自己定的，只是命令名和原版一致。

- `crates/reco-engine/src/item_order.rs`，命令 `get_item_recommendations`，参数都可省略，省略时取当前对局状态。
- 顺序：出门装 → 公式核心装；针对装插在中间（第 1 件核心后出鞋，第 2 件后出重伤，第 3 件后出第一件防装）。
- 按 `item_prices_v1.json` 的合成表计算每件还差多少金币，背包里已有的散件会被扣掉，同一个散件不会重复抵扣。
- 按当前金币给出"现在买什么"：买得起整件就买整件，否则优先买买得起的最贵散件。
- 背包总价值超过出门装时，出门装视为已完成；已穿其他成品鞋时，不再推荐第二双。
- 背包来自局内接口：`LiveGameSnapshot` 和悬浮窗状态新增了 `owned_item_ids`。
- 界面：流派参考悬浮窗显示整条出装顺序和还差的金币；阵亡提示显示现在该买什么。
- 验证：5 个单元测试（合成价格、散件购买、真实英雄的完整方案、已有装备、未知英雄）。没有在真实对局里看过。

#### 2.9 的实现（赛后复盘）

同样，评分规则是我自己定的。

- `crates/reco-engine/src/postgame.rs`，命令 `get_postgame_augment_review`，传入一条战绩。
- 分均输出、承伤、治疗对照 `hero_postgame_benchmarks_v1.json` 里同英雄的分位数，算出排名。不足 10 分钟的对局标为仅供参考。
- 每张强化按该英雄的数据给梯度；找出强化之间的联动和冲突；检查是否凑齐 `hero_augment_trios.json` 里的推荐组合；列出评分更高但没拿的强化。
- 对照公式核心装，统计完成了几件。
- 综合分：输出排名 45%、承伤与治疗排名中较高的一项 20%、强化平均 HexScore 35%，每组冲突扣 8 分。80 分以上为 S，65 为 A，50 为 B，35 为 C，其余为 D。
- 战绩条目新增了 `damage_taken`、`heal`、`items` 三个字段。
- 界面：「个人战绩」每局多了"复盘"按钮，点开显示评级、三项排名、强化评价和总结。
- 验证：5 个单元测试。界面没有实际点过；离线预览的示例战绩没有承伤和装备数据，复盘结果不完整。

#### 2.10 的实现

- 新命令：`zoom_overlay`、`reset_overlay_zoom`、`reset_overlay_position`、`start_overlay_drag`。
- 缩放范围 75%–130%，每步 5%；窗口大小跟着缩放变化（之前只有内容缩放，窗口不变）。
- 拖动悬浮窗标题栏可以移动窗口，位置存在 `data/overlay_layout.json`，下次启动恢复；保存的位置不在任何显示器上时不恢复。
- 鼠标移到悬浮窗上会出现缩放和重置位置的按钮；设置页有"全部重置"。
- 验证：缩放计算和位置存取有单元测试，应用能正常启动。拖动和按钮没有在界面上实际点过。

### 2.1 OCR 运行方式

| | 原版 | 现在 |
| --- | --- | --- |
| 形态 | Nuitka 4.2 编译的 `ocr/hexbox-ocr.exe`，自带 Python 3.12 和全部依赖 | Python 脚本，需要用户自己装 Python |
| 依赖版本 | RapidFuzz 3.14.6、mss 10.2.0、numpy 2.5.2、onnxruntime 1.29.0、pillow 12.3.0 | `requirements.txt` 只写了下限 |
| 模块 | `worker`、`service`、`augment_pipeline`、`card_locator`、`preset_ocr`，以及 `training_*` 四个训练样本模块 | 前四个已有；`preset_ocr` 的功能在 `hexbox_ocr.py` 里；训练样本模块没有 |
| 回退 | Worker 失败时退回单次命令行调用（`preset --preset-key --performance-mode --screen-image --capture-screen` 等参数） | Worker 失败后只重启一次 |
| 其他 | 截图缓存清理（`cleanup_ocr_cache`）、前台窗口不是游戏时拒绝识别（`--allow-non-lol-foreground`） | 没有 |

这是和原版使用体验差距最大的一项，建议最先做：在 CI 里用 Nuitka 或 PyInstaller 把 `ocr-sidecar` 编译成可执行文件，放进安装包的 `ocr/` 目录。

### 2.2 客户端事件

| 原版 | 现在 |
| --- | --- |
| LCU WebSocket 订阅（`lcu-client/src/websocket.rs`、`champ_select_feed.rs`），事件名如 `OnJsonApiEvent_lol-champ-select_v1_session` | 只有定时轮询 |
| 用 WMIC 查 `LeagueClientUx.exe` 的命令行来发现客户端，并读 `RiotClientInstalls.json` | 只查 lockfile |
| 多客户端列表与切换（`list_lcu_clients`、`select_lcu_client`） | 没有 |
| LCU 事件录制（`start/stop/clear/get_lcu_event_recording`） | 没有 |

### 2.3 战绩与玩家查询

| 原版 | 现在 |
| --- | --- |
| SGP 接口（`lcu-client/src/sgp.rs`） | `sgp_enabled` 只是设置项，没有实现 |
| 搜索玩家、玩家档案（`search_player`、`get_player_profile`，`player_query.rs`） | 没有 |
| 对局名单、历史 HexScore（`get_game_roster`、`get_history_hex_scores`） | 只有本人的 LCU 战绩列表 |
| 局内玩家情报（`get_game_players`、`refresh_game_player_intel`、`get_intel_cache_status`） | 只有玩家战力条 |

### 2.4 玩家标记与遭遇记录

| 原版 | 现在 |
| --- | --- |
| 标记存在后端（`list/save/delete_player_marker`） | 只存在前端本地存储 |
| 遭遇记录（`encounter_recorder.rs`，`get_encounter_summary`、`delete/prune_player_encounters`） | 没有 |
| 玩家库导入导出（`list/export/import_player_library`） | 没有 |

### 2.5 开黑

| 原版 | 现在 |
| --- | --- |
| 开黑大厅页面（前端资源 `kaiheiHall-*.js`） | 「开黑与标记」页只有标记 |
| 邀请玩家进房间（`invite_kaihei_player_to_lobby`，`lobby_invite.rs`） | 没有 |

开黑大厅连的是原版自己的服务器，复现时只有"邀请进房间"这部分能在本地做。

### 2.6 召唤师技能与符文

| 原版 | 现在 |
| --- | --- |
| 读取并自动设置召唤师技能（`get_summoner_spell_snapshot`、`apply_summoner_spells`，设置项 `summoner_flash_slot`） | 没有 |
| 无限火力推荐和符文页（`get_arurf_recommendation`、`apply_arurf_rune_page`） | 没有 |
| 装备方案同步到客户端（权限项 `itemSetSyncAllowed`） | 没有 |

### 2.7 推荐与出装

| 原版 | 现在 |
| --- | --- |
| 出装顺序引擎（`reco-engine/src/item_order.rs`、`item_plan.rs`，`get_item_recommendations`） | 已实现，规则自定（见"进度"） |
| 强化价值模式（`augment_value.rs`，设置项 `augment_value_mode`） | 只有一种评分方式 |
| 强化出现率统计（`augment_offer_stats.rs`） | 没有 |
| 手动改选强化（`set_manual_augment_selection`、`set_manual_augment_override`） | 没有 |
| BP 草稿快照（`get_bp_draft_snapshot`）、阵容对位上下文（`get_lineup_matchup_context`） | 只有 5v5 阵容评估 |
| 英雄预取（`hero_prefetch.rs`） | 没有 |
| `destiny_weapon.rs`（用途未确认） | 没有 |

### 2.8 板凳席

| 原版 | 现在 |
| --- | --- |
| 抢英雄和取消（`grab_bench_champion`、`cancel_bench_grab`，`bench_autograb_worker.rs`） | 有 `bench_swap` 和自动抢，没有取消 |
| 自动接受带延迟设置（`ready_check_accept_delay_ms`） | 有自动接受，没有延迟 |

### 2.9 赛后

| 原版 | 现在 |
| --- | --- |
| 赛后强化复盘（`postgame_review.rs`，`get_postgame_augment_review`） | 已实现，规则自定（见"进度"） |
| 赛后队伍分析（`postgame_team.rs`） | 没有 |
| 对局回执（`get_match_receipts`、`read_match_receipt`） | 没有 |

### 2.10 悬浮窗

| 原版 | 现在 |
| --- | --- |
| 缩放与重置（`zoom_overlay`、`reset_overlay_zoom`、`zoom_formula_item_overlay`） | 没有 |
| 位置记忆与重置（`reset_overlay_position`、`get_overlay_layout`） | 没有 |
| 外观预设与字体缩放（`get/set_overlay_look`、`save_font_scale`） | 没有 |
| 按内容自适应高度（`fit_overlay_content_height`、`fit_formula_item_overlay_height`） | 固定尺寸 |
| 流派参考窗口可调整大小、可获取键盘焦点 | 没有 |
| 紧凑对局窗口（`toggle_compact_match_window`） | 没有 |
| 本局内关闭玩家战力条（`mute_player_scores_overlay_for_session`） | 只有总开关 |
| 设置页里的悬浮窗预览（`save_overlay_preview_settings` 等） | 没有 |

### 2.11 窗口与系统

| 原版 | 现在 |
| --- | --- |
| 游戏无边框窗口修复（`set_league_window_mode_borderless`，`window_mode.rs`） | 没有 |
| 游戏设置锁定（`lock/unlock_game_settings`、`update_fixed_game_settings`） | 没有 |
| 关闭按钮行为选择（`choose_close_button_action`，设置项 `close_button_action`） | 直接退出 |
| 主窗口尺寸记忆（`main_window_geometry.rs`） | 没有 |
| 模拟预览窗口（`simulation_window.rs`，`open_simulation_preview`） | 没有 |
| 提权处理（`elevation.rs`） | 没有 |
| WebView 崩溃恢复（`runtime_recovery.rs`） | 没有 |

### 2.12 更新与数据

| 原版 | 现在 |
| --- | --- |
| 应用自动更新（`app_update.rs`，`check_app_update`） | 没有 |
| 远程数据同步（`commands/data_sync.rs`） | 只有本地 manifest 校验；`scripts/sync_hexdata.py` 支持 `--source-url`，但应用内没有入口 |
| `data-runtime` 里有 `skill_cards.rs`、`hero_benchmarks.rs`、`item_prices.rs`、`augment_mechanic_pairs.rs` 四个模块 | `skill_cards.json` 和 `hero_postgame_benchmarks_v1.json` 文件在，但后端没有读取 |
| 韩服 OCR 模型、客户端语言识别（`get_client_locale`） | 韩服模型文件在，后端没有按语言切换 |

### 2.13 设置项

原版设置里能读出、当前 `AppSettings` 里没有的字段：

`ready_check_accept_delay_ms`、`hex_score_display`、`public_overlay_mode`、`preferred_hero_name`、`summoner_flash_slot`、`close_button_action`、`overlay_font_scale`、`kaihei_tab_enabled`、`augment_stats_tier`、`augment_value_mode`、`overlay_compact`。

---

## 3. 不建议复现的部分

这些功能连的是原版作者的服务器，或者只服务于原版的商业运营：

- 账号、设备身份、授权与试用（`commands/account.rs`、`identity.rs`，接口如 `/v1/license/...`、`/v1/trial/ultra/...`）
- 功能权限开关（`set_account_capabilities`、`get_rollout_status`、`persist_remote_config`）
- 遥测（`native_telemetry.rs`、`game_health_telemetry.rs`、`recognition_telemetry.rs`、`/v1/account/analytics/events`）
- 诊断上传（`support_diagnostics.rs`、`upload_support_diagnostics`）
- 训练样本上传（`commands/training_sample/*`）
- 安装来源归因（`consume_installer_attribution`）

本地的诊断导出（`export_diagnostics`）可以按需保留一个精简版。

---

## 4. 建议顺序

1. **自带 OCR 可执行文件**（2.1）：去掉 Python 依赖。
2. **LCU WebSocket**（2.2）：选人和对局状态变化能立刻响应，也是 2.6、2.8 的基础。
3. **悬浮窗缩放和位置记忆**（2.10）：不同分辨率下才好用。
4. **出装顺序和赛后复盘**（2.7、2.9）：数据文件已经在，主要是后端逻辑。
5. **SGP 战绩与玩家查询、遭遇记录**（2.3、2.4）。
6. **召唤师技能、符文、窗口模式修复**（2.6、2.11）。
7. **自动更新和远程数据同步**（2.12）：需要先定好自己的发布地址。

---

## 附录：原版安装目录结构

```text
HexBox/
├── aram-assistant-app.exe      # 主程序（Tauri，约 55 MB）
├── uninstall.exe
├── data/                       # 19 个 JSON
├── models/                     # 4 个 ONNX + 3 个字典 + own-card-detector.json
└── ocr/
    ├── hexbox-ocr.exe          # Nuitka 编译的 OCR 程序（约 26 MB）
    ├── compiled-ocr-manifest.json
    ├── python312.dll 及各 .pyd / .dll
    └── numpy/  onnxruntime/  PIL/  rapidfuzz/  licenses/
```

当前 `data/` 比原版多 5 个文件，都是 `scripts/prepare_dev_data.py` 生成的：`augment_mechanic_pairs_v1.json`、`augment_rarity.json`、`augments_full.json`、`hero_synergies_counters.json`、`postmatch.json`。
