# CYG — Cook Your Game · Landing Page

Marketing / landing site for CYG (Cook Your Game), the single-binary game server platform.

**Stack:** React 18 · Vite 6 · Tailwind CSS v4 · GSAP (ScrollTrigger) · vanilla Three.js (no R3F, fake bloom via glow sprites — keeps the 3D chunk at ~124 kB gzip) · highlight.js · simple-icons

## Run

```bash
cd landingpage
npm install
npm run dev        # http://localhost:5173
```

Build / preview:

```bash
npm run build      # → dist/
npm run preview
```

## What's inside

| Path | What |
|---|---|
| `src/three/Scene.jsx` | The WebGL hero (vanilla three.js in a React wrapper): glowing server core (heartbeat pulse + sprite glow), icosahedron cage, Fibonacci-sphere player particle shell, network arcs with travelling packets, custom-shader radar floor, star dust, mouse-parallax camera. Lazy-loaded + code-split; pauses when off-screen or tab hidden; respects `prefers-reduced-motion`. |
| `src/components/Hero.jsx` | Full-viewport hero over the 3D scene, GSAP entrance timeline. |
| `src/components/Stats.jsx` | GSAP-animated counters (9k CCU/core, 0.06 ms p99, 16k ECS, 30+ imports). |
| `src/components/Features.jsx` | Six-card grid: WASM rooms, dual-lane netcode, physics, BaaS, console, SDK. |
| `src/components/Architecture.jsx` | "One process" diagram — control plane / game host loop / animated lock-free bus. |
| `src/components/CodeSection.jsx` | Tabbed code viewer (room.ts / client.js / schema.ts) with highlight.js. |
| `src/components/ConsolePreview.jsx` | The **real** web console — actual captures from `docs/screenshots/` presented straight (border + deep shadow + soft brand glow), with minimal view tabs (Live telemetry / WASM studio). No mock data, no fake chrome. |
| `src/components/Threed.jsx` + `src/three/ArenaScene.jsx` | "Full 3D worlds, served authoritative" — live vanilla-three wireframe arena (circling tanks, floating crates, collectible orb, periodic raycast flashes) next to grounded copy: host-side Rapier3D, `ox-game cook` (.glb → .oxcl), navmesh A* + AOI, lag-compensated raycasts, and an illustrative 3D-arena `config.toml` sample (no repo example names — `examples/` is not published). |
| `src/components/TechStack.jsx` | "Built on proven technology" logo wall — Rust, WebAssembly, WebRTC, AssemblyScript, PostgreSQL, Redis, Docker, Kubernetes (simple-icons) + Rapier2D/3D, Axum, ECS, str0m, wasmtime sigils. |
| `src/components/Benchmarks.jsx` | Scroll-triggered animated benchmark bars (incl. Rapier3D row) + real capacity cards (~4k combat CCU per $10/mo VPS, 100×100 rooms×players on 2-core/2 GB, 0.76× tick budget, 93 MB peak RAM). |
| `src/components/Quickstart.jsx` | Auto-typing terminal loop + 3-step guide + copy-to-clipboard install line. |
| `src/components/CTA.jsx` / `Footer.jsx` | Closing call-to-action and sitemap footer. |

All copy and numbers are taken from the root `README.md` (measured benchmarks, Colyseus-shaped API, $10/mo VPS pitch) and kept in sync with its corrections — e.g. the WASM sandbox claim is "a trapping room fails alone", the schema sample uses the current `schema()` / `t.number()` / `t.map()` API, and the install line includes `OX_ALLOW_DEFAULT_SECRETS=1`.

Console images live in `public/` (copied from `docs/screenshots/` — re-copy if the console UI changes).

## Design system

Tokens live in `src/index.css` under `@theme` (Tailwind v4) with semantic text roles — `fg` (primary), `sub` (body, ≥8:1 contrast), `mute` (meta), surfaces `ink/panel/panel-2`, `line` borders, and brand accents `oxide` `#4ade80` / `ember` / `frost`. Shared component classes (`.btn-primary`, `.btn-ghost`, `.btn-lg/sm`, `.card`, `.chip`, `.kicker`, `.code-inline`) are defined once in `@layer components` and used across sections. Hero text sits on a strengthened vignette with text shadows so it stays readable over the WebGL scene.
