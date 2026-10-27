import { useEffect, useRef } from 'react'
import * as THREE from 'three'

/* soft radial glow for the orb */
function makeGlowTexture() {
  const c = document.createElement('canvas')
  c.width = c.height = 128
  const ctx = c.getContext('2d')
  const g = ctx.createRadialGradient(64, 64, 0, 64, 64, 64)
  g.addColorStop(0, 'rgba(140,255,180,0.9)')
  g.addColorStop(0.3, 'rgba(74,222,128,0.35)')
  g.addColorStop(1, 'rgba(74,222,128,0)')
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 128, 128)
  return new THREE.CanvasTexture(c)
}

/* "+1" score popup texture */
function makeTextTexture(text) {
  const c = document.createElement('canvas')
  c.width = 128
  c.height = 64
  const ctx = c.getContext('2d')
  ctx.font = 'bold 44px system-ui, sans-serif'
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  ctx.shadowColor = 'rgba(74,222,128,0.9)'
  ctx.shadowBlur = 14
  ctx.fillStyle = '#a7f3d0'
  ctx.fillText(text, 64, 32)
  return new THREE.CanvasTexture(c)
}

const ZINC = 0x3f3f46
const OXIDE_HEX = 0x4ade80
const EMBER_HEX = 0xfbbf24
const FROST_HEX = 0x60a5fa

function std(color, opts = {}) {
  return new THREE.MeshStandardMaterial({ color, roughness: 0.82, metalness: 0.12, ...opts })
}

/**
 * Live 3D arena — lit low-poly diorama demonstrating the section's claims:
 *   orb pickup → +1 score popup → respawn        (authoritative scoring)
 *   lag-compensated raycast → visible crate hit   (hit registration)
 *   bot walks off the edge → falls → respawns     (fall = respawn rule)
 * Vanilla three.js, lazy-loaded, paused off-screen, respects reduced-motion.
 */
export default function ArenaScene() {
  const mountRef = useRef(null)

  useEffect(() => {
    const mount = mountRef.current
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.75))
    renderer.setSize(mount.clientWidth, mount.clientHeight)
    renderer.shadowMap.enabled = true
    renderer.shadowMap.type = THREE.PCFSoftShadowMap
    mount.appendChild(renderer.domElement)

    const scene = new THREE.Scene()
    scene.fog = new THREE.Fog(0x09090b, 18, 46)
    const camera = new THREE.PerspectiveCamera(42, mount.clientWidth / mount.clientHeight, 0.1, 100)

    /* ── lights ── */
    scene.add(new THREE.AmbientLight(0x8b93a5, 0.55))

    const key = new THREE.DirectionalLight(0xf5f7ff, 1.6)
    key.position.set(9, 15, 7)
    key.castShadow = true
    key.shadow.mapSize.set(1024, 1024)
    key.shadow.camera.left = -14
    key.shadow.camera.right = 14
    key.shadow.camera.top = 14
    key.shadow.camera.bottom = -14
    key.shadow.camera.near = 1
    key.shadow.camera.far = 42
    key.shadow.bias = -0.002
    scene.add(key)

    const fill = new THREE.DirectionalLight(0x60a5fa, 0.25)
    fill.position.set(-8, 6, -9)
    scene.add(fill)

    /* ── ground: outer void grid + solid arena platform ── */
    const grid = new THREE.GridHelper(40, 40, 0x232328, 0x141416)
    grid.position.y = -0.55
    scene.add(grid)

    const platform = new THREE.Mesh(
      new THREE.CylinderGeometry(10, 10.4, 0.6, 72),
      std(0x17171a),
    )
    platform.position.y = -0.3
    platform.receiveShadow = true
    scene.add(platform)

    /* glowing rim ring on the platform edge */
    const rim = new THREE.Mesh(
      new THREE.TorusGeometry(10, 0.035, 8, 96),
      new THREE.MeshBasicMaterial({ color: OXIDE_HEX, transparent: true, opacity: 0.5 }),
    )
    rim.rotation.x = Math.PI / 2
    rim.position.y = 0.01
    scene.add(rim)

    /* ── crates (raycast targets) — grounded, cast shadows ──
       placed inside r<4.1 so both tank circles (5.6 / 8.2) stay clear */
    const crates = []
    const crateSpecs = [
      [-3.0, -2.0, 1.4], [3.4, -1.4, 1.0], [1.6, 3.2, 1.2],
      [-2.2, 2.6, 0.8], [0.4, -3.4, 0.9], [-0.8, 0.8, 0.7],
    ]
    for (const [x, z, s] of crateSpecs) {
      const m = new THREE.Mesh(new THREE.BoxGeometry(s, s, s), std(ZINC))
      m.position.set(x, s / 2, z)
      m.rotation.y = Math.random() * Math.PI
      m.castShadow = true
      m.receiveShadow = true
      scene.add(m)
      crates.push({ mesh: m, hitT: 0 })
    }

    /* ── collectible orb: emissive core + glow sprite + point light ── */
    const glowTex = makeGlowTexture()
    const orb = new THREE.Group()
    const orbCore = new THREE.Mesh(
      new THREE.SphereGeometry(0.3, 24, 24),
      new THREE.MeshStandardMaterial({
        color: 0xbaffd9, emissive: OXIDE_HEX, emissiveIntensity: 1.6, roughness: 0.3,
      }),
    )
    orb.add(orbCore)
    const orbGlow = new THREE.Sprite(new THREE.SpriteMaterial({
      map: glowTex, transparent: true, opacity: 0.75,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    orbGlow.scale.setScalar(2.4)
    orb.add(orbGlow)
    const orbLight = new THREE.PointLight(OXIDE_HEX, 10, 9, 2)
    orbLight.position.y = 0.4
    orb.add(orbLight)
    orb.position.set(0, 1.5, 0)
    scene.add(orb)

    /* "+1" popup */
    const popup = new THREE.Sprite(new THREE.SpriteMaterial({
      map: makeTextTexture('+1'), transparent: true, opacity: 0, depthWrite: false,
    }))
    popup.scale.set(1.6, 0.8, 1)
    scene.add(popup)

    /* ── tanks ── */
    function makeTank(color) {
      const t = new THREE.Group()
      const bodyMat = std(color)
      const body = new THREE.Mesh(new THREE.BoxGeometry(1.6, 0.5, 1.05), bodyMat)
      body.position.y = 0.32
      body.castShadow = true
      t.add(body)
      // track skirts
      for (const sx of [-0.52, 0.52]) {
        const skirt = new THREE.Mesh(new THREE.BoxGeometry(0.35, 0.3, 1.15), std(0x27272a))
        skirt.position.set(sx * 1.5, 0.16, 0)
        skirt.castShadow = true
        t.add(skirt)
      }
      const turret = new THREE.Mesh(new THREE.CylinderGeometry(0.42, 0.5, 0.32, 16), bodyMat)
      turret.position.y = 0.72
      turret.castShadow = true
      t.add(turret)
      const barrel = new THREE.Mesh(new THREE.CylinderGeometry(0.06, 0.06, 1.25, 10), std(0x27272a))
      barrel.rotation.x = Math.PI / 2
      barrel.position.set(0, 0.72, 0.95)
      t.add(barrel)
      scene.add(t)
      return t
    }
    const tanks = [
      { mesh: makeTank(0x2ea86b), r: 5.6, speed: 0.22, phase: 0 },
      { mesh: makeTank(ZINC), r: 8.2, speed: -0.34, phase: 2.1 },
    ]

    /* ── wandering bot: falls off, respawns ── */
    const bot = new THREE.Mesh(new THREE.BoxGeometry(0.7, 0.7, 0.7), std(FROST_HEX))
    bot.castShadow = true
    scene.add(bot)
    const botState = { dir: Math.random() * Math.PI * 2, falling: false, vy: 0 }
    function respawnBot() {
      const a = Math.random() * Math.PI * 2
      bot.position.set(Math.cos(a) * 3.5, 0.35, Math.sin(a) * 3.5)
      bot.rotation.set(0, 0, 0)
      bot.visible = true
      botState.dir = Math.random() * Math.PI * 2
      botState.falling = false
      botState.vy = 0
    }
    respawnBot()

    /* ── raycast flash: tank → crate, impact sprite + emissive hit ── */
    const rayGeo = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()])
    const rayMat = new THREE.LineBasicMaterial({
      color: EMBER_HEX, transparent: true, opacity: 0,
      blending: THREE.AdditiveBlending, depthWrite: false,
    })
    const ray = new THREE.Line(rayGeo, rayMat)
    scene.add(ray)
    const impact = new THREE.Sprite(new THREE.SpriteMaterial({
      map: glowTex, color: EMBER_HEX, transparent: true, opacity: 0,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    impact.scale.setScalar(1.6)
    scene.add(impact)
    let rayTimer = 1.2
    let rayLife = 0

    /* orb pickup state */
    let orbRespawn = 0
    let popupLife = 0

    /* interaction + loop */
    const pointer = { x: 0, y: 0 }
    const onPointer = (e) => {
      pointer.x = (e.clientX / window.innerWidth) * 2 - 1
      pointer.y = -((e.clientY / window.innerHeight) * 2 - 1)
    }
    window.addEventListener('pointermove', onPointer, { passive: true })

    const clock = new THREE.Clock()
    let raf = 0
    let visible = true

    const renderFrame = () => {
      const t = clock.getElapsedTime()
      const dt = Math.min(clock.getDelta() || 0.016, 0.05)

      /* crates: hit flash decays */
      for (const c of crates) {
        if (c.hitT > 0) {
          c.hitT -= dt
          const k = Math.max(0, c.hitT / 0.5)
          c.mesh.material.emissive.setHex(EMBER_HEX)
          c.mesh.material.emissiveIntensity = k * 1.2
          if (c.hitT <= 0) {
            c.mesh.material.emissiveIntensity = 0
          }
        }
      }

      /* tanks circle & face travel direction */
      for (const tk of tanks) {
        const a = tk.phase + t * tk.speed
        tk.mesh.position.set(Math.cos(a) * tk.r, 0, Math.sin(a) * tk.r)
        tk.mesh.rotation.y = -a - Math.sign(tk.speed) * Math.PI / 2
      }

      /* bot wanders — steers around crates and tanks, falls off the edge, respawns */
      if (!botState.falling) {
        // avoid obstacles: if something is too close ahead, steer away from it
        const avoid = (ox, oz, rad) => {
          const dx = bot.position.x - ox
          const dz = bot.position.z - oz
          const d = Math.hypot(dx, dz)
          if (d < rad && d > 0.001) botState.dir = Math.atan2(dz, dx)
        }
        for (const c of crates) avoid(c.mesh.position.x, c.mesh.position.z, 1.5)
        for (const tk of tanks) avoid(tk.mesh.position.x, tk.mesh.position.z, 1.8)

        bot.position.x += Math.cos(botState.dir) * dt * 1.6
        bot.position.z += Math.sin(botState.dir) * dt * 1.6
        bot.rotation.y = -botState.dir
        const r = Math.hypot(bot.position.x, bot.position.z)
        if (r > 9.7) {
          botState.falling = true
          botState.vy = 0
        }
      } else {
        botState.vy -= 14 * dt
        bot.position.y += botState.vy * dt
        bot.rotation.x += dt * 3
        if (bot.position.y < -6) respawnBot()
      }

      /* orb: bob, spin, pickup check, respawn */
      if (orb.visible) {
        orb.position.y = 1.5 + Math.sin(t * 1.4) * 0.22
        orb.rotation.y = t * 0.8
        for (const tk of tanks) {
          if (tk.mesh.position.distanceTo(orb.position) < 1.45) {
            orb.visible = false
            orbRespawn = 1.4
            popup.position.copy(orb.position).add(new THREE.Vector3(0, 0.5, 0))
            popup.material.opacity = 1
            popupLife = 1
            break
          }
        }
      } else if (orbRespawn > 0) {
        orbRespawn -= dt
        if (orbRespawn <= 0) {
          // spawn on the tank paths' band (never inside a crate)
          const a = Math.random() * Math.PI * 2
          const r = 4.5 + Math.random() * 3.9
          orb.position.set(Math.cos(a) * r, 1.5, Math.sin(a) * r)
          orb.visible = true
        }
      }

      /* "+1" popup rises & fades */
      if (popupLife > 0) {
        popupLife -= dt * 0.9
        popup.position.y += dt * 1.1
        popup.material.opacity = Math.max(0, popupLife)
      }

      /* periodic raycast: a tank shoots a crate */
      rayTimer -= dt
      if (rayTimer <= 0 && rayLife <= 0) {
        rayTimer = 1.8 + Math.random() * 1.6
        rayLife = 0.55
        const from = tanks[Math.floor(Math.random() * tanks.length)].mesh.position
        const target = crates[Math.floor(Math.random() * crates.length)]
        target.hitT = 0.5
        const to = target.mesh.position
        impact.position.copy(to)
        const pos = rayGeo.attributes.position.array
        pos[0] = from.x; pos[1] = from.y + 0.75; pos[2] = from.z
        pos[3] = to.x; pos[4] = to.y; pos[5] = to.z
        rayGeo.attributes.position.needsUpdate = true
      }
      if (rayLife > 0) {
        rayLife -= dt
        rayMat.opacity = Math.max(0, rayLife / 0.55) * 0.9
        impact.material.opacity = Math.max(0, rayLife / 0.55) * 0.8
      }

      /* slow orbit + parallax — lower, more cinematic angle */
      const ang = t * 0.045
      const tx = Math.sin(ang) * 15 + pointer.x * 1.6
      const tz = Math.cos(ang) * 15
      const ty = 7.5 + pointer.y * 1.1
      camera.position.x += (tx - camera.position.x) * 0.03
      camera.position.y += (ty - camera.position.y) * 0.03
      camera.position.z += (tz - camera.position.z) * 0.03
      camera.lookAt(0, 0.7, 0)

      renderer.render(scene, camera)
    }

    const tick = () => {
      raf = requestAnimationFrame(tick)
      if (visible) renderFrame()
    }

    if (reduced) {
      camera.position.set(13, 8, 13)
      camera.lookAt(0, 0.7, 0)
      renderer.render(scene, camera)
    } else {
      tick()
    }

    const io = new IntersectionObserver(([e]) => { visible = e.isIntersecting }, { threshold: 0 })
    io.observe(mount)
    const onVis = () => { visible = document.visibilityState === 'visible' }
    document.addEventListener('visibilitychange', onVis)

    const ro = new ResizeObserver(() => {
      const w = mount.clientWidth
      const h = mount.clientHeight
      camera.aspect = w / h
      camera.updateProjectionMatrix()
      renderer.setSize(w, h)
      if (reduced) renderer.render(scene, camera)
    })
    ro.observe(mount)

    return () => {
      cancelAnimationFrame(raf)
      io.disconnect()
      ro.disconnect()
      document.removeEventListener('visibilitychange', onVis)
      window.removeEventListener('pointermove', onPointer)
      scene.traverse((o) => {
        o.geometry?.dispose?.()
        const mats = Array.isArray(o.material) ? o.material : [o.material]
        mats.forEach((m) => { m?.map?.dispose?.(); m?.dispose?.() })
      })
      glowTex.dispose()
      renderer.dispose()
      mount.removeChild(renderer.domElement)
    }
  }, [])

  return <div ref={mountRef} className="h-full w-full" aria-hidden="true" />
}
