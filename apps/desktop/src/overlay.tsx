import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { loadDatabase, type LoadedHexBoxDatabase } from "./api/dataService";
import { OverlayApp, type OverlayKind } from "./overlays/OverlayApp";
import "./styles/replica.css";

function resolveOverlayKind(): OverlayKind {
  const params = new URLSearchParams(window.location.search);
  const raw = params.get("overlay") || "augment";
  const allowed: OverlayKind[] = [
    "augment",
    "item",
    "champ-select",
    "player-scores",
    "death-hint",
  ];
  return allowed.includes(raw as OverlayKind) ? (raw as OverlayKind) : "augment";
}

const OverlayEntry: React.FC = () => {
  const [db, setDb] = useState<LoadedHexBoxDatabase | null>(null);
  const kind = resolveOverlayKind();

  useEffect(() => {
    loadDatabase().then(setDb).catch(() => {});
  }, []);

  if (!db) {
    return <div className="overlay-card">正在初始化悬浮窗数据...</div>;
  }

  return <OverlayApp kind={kind} db={db} />;
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <OverlayEntry />
  </React.StrictMode>
);
