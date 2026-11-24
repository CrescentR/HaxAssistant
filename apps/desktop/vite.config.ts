import { execFile } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "../..");
const DATA_ROOT = path.join(REPO_ROOT, "data");
const OCR_ROOT = path.join(REPO_ROOT, "ocr-sidecar");

function resolvePythonBin(): string {
  const candidates = [
    path.join(OCR_ROOT, ".venv", "bin", "python"),
    path.join(REPO_ROOT, ".venv", "bin", "python"),
    path.join(OCR_ROOT, ".venv", "Scripts", "python.exe"),
    "python3",
  ];
  for (const cand of candidates) {
    if (cand === "python3" || fs.existsSync(cand)) return cand;
  }
  return "python3";
}

function hexboxDevMiddleware(): Plugin {
  return {
    name: "hexbox-dev-middleware",
    closeBundle() {
      // Copy offline JSON datasets into dist/data so standalone Tauri builds serve /data/* cleanly
      if (!fs.existsSync(DATA_ROOT)) return;
      const distDataDir = path.join(__dirname, "dist", "data");
      fs.mkdirSync(distDataDir, { recursive: true });
      for (const entry of fs.readdirSync(DATA_ROOT)) {
        if (!entry.endsWith(".json")) continue;
        fs.copyFileSync(path.join(DATA_ROOT, entry), path.join(distDataDir, entry));
      }
    },
    configureServer(server) {
      // Serve /data/* from repository root data/
      server.middlewares.use("/data", (req, res, next) => {
        const reqPath = (req.url || "").split("?")[0].replace(/^\/+/, "");
        if (!reqPath) return next();
        const filePath = path.resolve(DATA_ROOT, reqPath);
        if (!filePath.startsWith(DATA_ROOT) || !fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
          res.statusCode = 404;
          res.setHeader("Content-Type", "application/json; charset=utf-8");
          res.end(JSON.stringify({ error: "not_found", path: reqPath }));
          return;
        }
        res.setHeader("Content-Type", "application/json; charset=utf-8");
        fs.createReadStream(filePath).pipe(res);
      });

      // Dev OCR endpoint: GET /api/ocr/run?preset=...&image=...
      server.middlewares.use("/api/ocr/run", (req, res) => {
        const urlObj = new URL(req.url || "/", "http://localhost:1420");
        const preset = urlObj.searchParams.get("preset") || "1920x1080@100";
        let imagePath = urlObj.searchParams.get("image") || "";
        const sampleImage = path.join(OCR_ROOT, "sample_augment_1080p.png");

        if (!imagePath || !fs.existsSync(imagePath)) {
          imagePath = sampleImage;
        }

        const scriptPath = path.join(OCR_ROOT, "hexbox_ocr.py");
        const pythonBin = resolvePythonBin();
        const args = [scriptPath, "preset", "--preset", preset, "--image", imagePath];

        execFile(
          pythonBin,
          args,
          {
            cwd: REPO_ROOT,
            timeout: 30000,
            maxBuffer: 10 * 1024 * 1024,
            env: {
              ...process.env,
              ARAM_ASSISTANT_DATA_DIR: DATA_ROOT,
              ARAM_ASSISTANT_OCR_MODEL_DIR: path.join(REPO_ROOT, "models"),
            },
          },
          (err, stdout, stderr) => {
            res.setHeader("Content-Type", "application/json; charset=utf-8");
            if (err) {
              res.statusCode = 500;
              res.end(
                JSON.stringify({
                  status: "error",
                  error: err.message,
                  stderr: stderr?.slice(0, 1000),
                })
              );
              return;
            }
            res.statusCode = 200;
            res.end(stdout.trim() || JSON.stringify({ status: "error", error: "empty_output" }));
          }
        );
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), hexboxDevMiddleware()],
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        overlay: path.resolve(__dirname, "overlay.html"),
      },
    },
  },
});
