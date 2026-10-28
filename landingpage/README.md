# CYG — Cook Your Game · Landing Page

Marketing / landing site for **CYG (Cook Your Game)** — the single-binary game server platform.
Engine lives at [ananddub/cyg](https://github.com/ananddub/cyg); this repo is only the website and deploys on its own.

**Stack:** React 18 · Vite 6 · Tailwind CSS v4 · GSAP (ScrollTrigger) · vanilla Three.js (no R3F — glow via sprites, no postprocessing chain) · highlight.js · simple-icons

## Run

```bash
npm install
npm run dev        # http://localhost:5173
```

Build / preview:

```bash
npm run build      # → dist/
npm run preview
```

## Deploy

Static output in `dist/` — works on any static host:

| Host | Settings |
|---|---|
| Vercel | Import repo → framework auto-detects **Vite** |
| Netlify / Cloudflare Pages | Build `npm run build` · output `dist` |

## What's inside

| Path | What |
|---|---|
| `src/three/Scene.jsx` | WebGL hero (vanilla three.js in a React wrapper): glowing server core with heartbeat pulse, icosahedron cage, Fibonacci-sphere player particle shell, network arcs with travelling packets, orbit rings, star dust, mouse-parallax camera. Lazy-loaded + code-split; pauses off-screen / tab-hidden; respects `prefers-reduced-motion`. |
| `src/components/Hero.jsx` | Full-viewport hero over the 3D scene, GSAP entrance timeline, scroll parallax. |
| `src/components/Stats.jsx` | GSAP counters — **1-core measured**: 7,000+ CCU/core (WASM+Rapier2D @128 Hz), 0.08 ms p99 tick, 10k CCU/core ECS, 30+ `sys_*` imports. |
| `src/components/Features.jsx` | Bento grid: WASM rooms, dual-lane netcode (+ QUIC/WebTransport lane chip), server physics, BaaS tag cloud, console, SDK (real platform logos). |
| `src/components/Threed.jsx` + `src/three/ArenaScene.jsx` | "Full 3D worlds, served authoritative" — lit low-poly 3D diorama (PCF soft shadows) demonstrating real mechanics: **orb pickup → +1 score → respawn**, **lag-compensated raycast → visible crate hit**, **bot falls off edge → respawns**. With grounded copy (Rapier3D host physics, `ox-game cook` .glb → .oxcl, navmesh A* + AOI, lag-compensated raycasts) + floating `config.toml` card (TOML-highlighted). |
| `src/components/Architecture.jsx` | "One process" diagram — control plane (Axum) / game host loop / animated lock-free bus with packet dots. |
| `src/components/CodeSection.jsx` | Tabbed code viewer (`room.ts` / `client.js` / `schema.ts`) with highlight.js — uses the current `schema()` / `t.number()` / `t.map()` API. |
| `src/components/ConsolePreview.jsx` | The **real** web console — actual captures from the engine repo's `docs/screenshots/`, presented straight (border + deep shadow + soft brand glow) with view tabs (Live telemetry / WASM studio). No mock data. |
| `src/components/TechStack.jsx` | "Built on proven technology" logo wall — Rust, WebAssembly, WebRTC, AssemblyScript, PostgreSQL, Redis, Docker, Kubernetes (simple-icons) + Rapier2D/3D, QUIC, WebTransport, Axum, ECS, str0m, wasmtime sigils. |
| `src/components/Benchmarks.jsx` | Scroll-triggered bars, **all measured in a strict 1-core / 2 GB cgroup, published below measured**: ~1.5k combat · ~5k combined tick · ~7k Rapier2D · ~10k ECS · ~15k Rapier3D CCU/core — plus real capacity cards (~4k combat CCU per $10/mo VPS, 100×100 rooms×players on 2-core/2 GB, 0.76× tick budget, 93 MB peak RAM). |
| `src/components/Quickstart.jsx` | Auto-typing terminal loop (`ox-game init` → `doctor` → `run`) + 3-step guide. |
| `src/components/CTA.jsx` / `Footer.jsx` | Closing CTA + sitemap footer (with Rust trademark attribution). |

All copy and numbers are grounded in the engine repo — benchmark figures are the published
1-core / 2 GB cgroup values (rounded **below** measured), and example-folder names are never
referenced (`examples/` is not published).

Console images live in `public/` (copied from the engine repo's `docs/screenshots/` — re-copy
if the console UI changes).

## Design system

Tokens live in `src/index.css` under `@theme` (Tailwind v4), matching the console's zinc-dark
theme: `ink` `#09090b` / `panel` / `panel-2` surfaces, `line` borders, text roles `fg` /
`sub` / `mute`, brand accents `oxide` `#4ade80` / `ember` / `frost`. Shared component
classes (`.btn-primary`, `.btn-ghost`, `.btn-lg/sm`, `.card`, `.chip`, `.eyebrow`,
`.code-inline`) are defined once in `@layer components`. Accent text is solid brand color —
no gradient text, no glow shadows; motion is restrained (fades + the two 3D scenes).
