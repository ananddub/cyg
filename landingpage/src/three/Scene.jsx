import { useEffect, useRef } from 'react'
import * as THREE from 'three'

const OXIDE = new THREE.Color('#4ade80')
const EMBER = new THREE.Color('#fbbf24')
const FROST = new THREE.Color('#60a5fa')

/* even point distribution on a sphere */
function fibSphere(count, radius) {
  const pts = new Float32Array(count * 3)
  const golden = Math.PI * (3 - Math.sqrt(5))
  for (let i = 0; i < count; i++) {
    const y = 1 - (i / (count - 1)) * 2
    const r = Math.sqrt(1 - y * y)
    const theta = golden * i
    pts[i * 3] = Math.cos(theta) * r * radius
    pts[i * 3 + 1] = y * radius
    pts[i * 3 + 2] = Math.sin(theta) * r * radius
  }
  return pts
}

/* soft radial glow — fake bloom without the postprocessing stack */
function makeGlowTexture() {
  const c = document.createElement('canvas')
  c.width = c.height = 256
  const ctx = c.getContext('2d')
  const g = ctx.createRadialGradient(128, 128, 0, 128, 128, 128)
  g.addColorStop(0, 'rgba(140,255,180,0.9)')
  g.addColorStop(0.25, 'rgba(74,222,128,0.4)')
  g.addColorStop(0.6, 'rgba(74,222,128,0.09)')
  g.addColorStop(1, 'rgba(74,222,128,0)')
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 256, 256)
  return new THREE.CanvasTexture(c)
}

/* tiny round sprite so Points render as circles, not squares */
function makeDotTexture() {
  const c = document.createElement('canvas')
  c.width = c.height = 64
  const ctx = c.getContext('2d')
  const g = ctx.createRadialGradient(32, 32, 0, 32, 32, 32)
  g.addColorStop(0, 'rgba(255,255,255,1)')
  g.addColorStop(0.4, 'rgba(255,255,255,0.9)')
  g.addColorStop(1, 'rgba(255,255,255,0)')
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 64, 64)
  return new THREE.CanvasTexture(c)
}

export default function Scene() {
  const mountRef = useRef(null)

  useEffect(() => {
    const mount = mountRef.current
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const mobile = window.innerWidth < 768

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.75))
    renderer.setSize(mount.clientWidth, mount.clientHeight)
    mount.appendChild(renderer.domElement)

    const scene = new THREE.Scene()
    scene.fog = new THREE.Fog(0x09090b, 9, 26)
    const camera = new THREE.PerspectiveCamera(42, mount.clientWidth / mount.clientHeight, 0.1, 100)
    camera.position.set(0, 1.7, 8.6)

    const root = new THREE.Group()
    root.position.y = 0.9
    scene.add(root)

    /* core: glow sprite + bright heart + wireframe cage */
    const glowTex = makeGlowTexture()
    const glow = new THREE.Sprite(new THREE.SpriteMaterial({
      map: glowTex, transparent: true, opacity: 0.7,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    glow.scale.setScalar(4.4)
    root.add(glow)

    const core = new THREE.Mesh(
      new THREE.SphereGeometry(0.4, 32, 32),
      new THREE.MeshBasicMaterial({ color: 0xeafff4 }),
    )
    root.add(core)

    // bright inner halo so the heart never looks dead
    const halo = new THREE.Sprite(new THREE.SpriteMaterial({
      map: glowTex, transparent: true, opacity: 0.9,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    halo.scale.setScalar(1.6)
    root.add(halo)

    const wire = new THREE.Mesh(
      new THREE.IcosahedronGeometry(1.35, 1),
      new THREE.MeshBasicMaterial({ color: 0x4ade80, wireframe: true, transparent: true, opacity: 0.26 }),
    )
    root.add(wire)

    const ring1 = new THREE.Mesh(
      new THREE.TorusGeometry(2.1, 0.006, 8, 128),
      new THREE.MeshBasicMaterial({ color: 0x4ade80, transparent: true, opacity: 0.35 }),
    )
    ring1.rotation.set(Math.PI / 2.4, 0.3, 0)
    root.add(ring1)

    const ring2 = new THREE.Mesh(
      new THREE.TorusGeometry(2.65, 0.005, 8, 128),
      new THREE.MeshBasicMaterial({ color: 0x60a5fa, transparent: true, opacity: 0.25 }),
    )
    ring2.rotation.set(Math.PI / 1.8, -0.5, 0.4)
    root.add(ring2)

    /* particle shell — the "players" */
    const N = mobile ? 600 : 1000
    const R = 3.4
    const shellPos = fibSphere(N, R)
    const shellCol = new Float32Array(N * 3)
    const c = new THREE.Color()
    for (let i = 0; i < N; i++) {
      const roll = Math.random()
      if (roll > 0.97) c.copy(EMBER)
      else if (roll > 0.92) c.copy(FROST)
      else c.copy(OXIDE).multiplyScalar(0.4 + Math.random() * 0.35)
      shellCol[i * 3] = c.r
      shellCol[i * 3 + 1] = c.g
      shellCol[i * 3 + 2] = c.b
    }
    const shellGeo = new THREE.BufferGeometry()
    shellGeo.setAttribute('position', new THREE.BufferAttribute(shellPos, 3))
    shellGeo.setAttribute('color', new THREE.BufferAttribute(shellCol, 3))
    const dotTex = makeDotTexture()
    const shell = new THREE.Points(shellGeo, new THREE.PointsMaterial({
      size: 0.06, map: dotTex, vertexColors: true, transparent: true, opacity: 0.75,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    root.add(shell)

    /* network arcs + travelling packets */
    const arcCount = mobile ? 8 : 12
    const pairPts = fibSphere(arcCount * 2, R)
    const curves = []
    for (let i = 0; i < arcCount; i++) {
      const a = new THREE.Vector3(pairPts[i * 6], pairPts[i * 6 + 1], pairPts[i * 6 + 2])
      const b = new THREE.Vector3(pairPts[i * 6 + 3], pairPts[i * 6 + 4], pairPts[i * 6 + 5])
      if (a.distanceTo(b) < R * 0.9) continue
      const mid = a.clone().add(b).multiplyScalar(0.5).normalize().multiplyScalar(R * (1.25 + Math.random() * 0.35))
      const curve = new THREE.QuadraticBezierCurve3(a, mid, b)
      curves.push(curve)
      const geo = new THREE.BufferGeometry().setFromPoints(curve.getPoints(32))
      const line = new THREE.Line(geo, new THREE.LineBasicMaterial({
        color: Math.random() > 0.75 ? 0x60a5fa : 0x4ade80,
        transparent: true, opacity: 0.14,
        blending: THREE.AdditiveBlending, depthWrite: false,
      }))
      root.add(line)
    }

    const pPos = new Float32Array(curves.length * 3)
    const pGeo = new THREE.BufferGeometry()
    pGeo.setAttribute('position', new THREE.BufferAttribute(pPos, 3))
    const packets = new THREE.Points(pGeo, new THREE.PointsMaterial({
      size: 0.14, map: dotTex, color: 0xfbbf24, transparent: true, opacity: 0.95,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }))
    root.add(packets)
    const pMeta = curves.map((_, i) => ({ curve: i, t: Math.random(), speed: 0.25 + Math.random() * 0.45 }))

    /* background dust */
    const dustN = mobile ? 150 : 300
    const dustPos = new Float32Array(dustN * 3)
    for (let i = 0; i < dustN; i++) {
      dustPos[i * 3] = (Math.random() - 0.5) * 60
      dustPos[i * 3 + 1] = (Math.random() - 0.5) * 30
      dustPos[i * 3 + 2] = -8 - Math.random() * 25
    }
    const dustGeo = new THREE.BufferGeometry()
    dustGeo.setAttribute('position', new THREE.BufferAttribute(dustPos, 3))
    const dust = new THREE.Points(dustGeo, new THREE.PointsMaterial({
      size: 0.08, map: dotTex, color: 0x3f3f46, transparent: true, opacity: 0.55, depthWrite: false,
    }))
    scene.add(dust)

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
      const dt = Math.min(clock.getDelta ? 0.016 : 0.016, 0.05)

      const beat = Math.exp(-((t % 0.9) / 0.3))
      core.scale.setScalar(1 + beat * 0.14)
      glow.material.opacity = 0.55 + beat * 0.3

      wire.rotation.y = t * 0.12
      wire.rotation.x = Math.sin(t * 0.09) * 0.18
      shell.rotation.y = t * 0.035
      ring1.rotation.z = t * 0.1
      ring2.rotation.z = -t * 0.08

      const arr = pGeo.attributes.position.array
      for (let i = 0; i < pMeta.length; i++) {
        const p = pMeta[i]
        p.t = (p.t + dt * p.speed) % 1
        const v = curves[p.curve].getPoint(p.t)
        arr[i * 3] = v.x
        arr[i * 3 + 1] = v.y
        arr[i * 3 + 2] = v.z
      }
      pGeo.attributes.position.needsUpdate = true

      const tx = Math.sin(t * 0.07) * 1.1 + pointer.x * 1.3
      const ty = 1.7 + Math.sin(t * 0.11) * 0.25 + pointer.y * 0.7
      camera.position.x += (tx - camera.position.x) * 0.03
      camera.position.y += (ty - camera.position.y) * 0.03
      camera.lookAt(0, 0.7, 0)

      renderer.render(scene, camera)
    }

    const tick = () => {
      raf = requestAnimationFrame(tick)
      if (visible) renderFrame()
    }

    if (reduced) {
      renderer.render(scene, camera)
    } else {
      tick()
    }

    /* pause when hero off-screen / tab hidden */
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
      dotTex.dispose()
      renderer.dispose()
      mount.removeChild(renderer.domElement)
    }
  }, [])

  return <div ref={mountRef} className="absolute inset-0" aria-hidden="true" />
}
