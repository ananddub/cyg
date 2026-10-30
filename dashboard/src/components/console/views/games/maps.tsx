// A game's map assets: the upload panel (cooked mesh / arena JSON / navmesh)
// and the version table with activate and delete. Rendered by the per-game
// detail tabs as the "Maps" tab.
//
// The upload is `POST /v2/console/games/{id}/maps/upload` as multipart form
// data (`file`, `format`, optional `name`) — like the wasm studio, the one raw
// request the panel needs, same bearer token, refusals surfaced as toasts.

import { useCallback, useEffect, useRef, useState, type ChangeEvent, type DragEvent } from "react"
import { createPortal } from "react-dom"
import {
  Eye,
  Loader2,
  Map as MapIcon,
  Maximize2,
  Minimize2,
  Move,
  Rocket,
  RotateCcw,
  Search,
  Upload,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Modal } from "@/components/console/modal"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { queryError, useConsoleQuery } from "@/hooks/use-console"
import { dash, formatTime } from "@/lib/format"
import { useSession } from "@/lib/session"

import { gameMapsPath } from "./routes"
import { OxclMeshPreview } from "./mesh-preview-lazy"

interface MapPreviewResponse {
  name: string
  version: number
  format: string
  sha256: string
  mesh?: { vertices: [number, number, number][]; indices: number[] } | null
  note?: string | null
}

/** The stored-bytes 3D preview: the server parses the OXCL mesh, the browser
 * renders it through the lazily-loaded WebGL viewer. A format without a
 * preview answers with a note, which is exactly what this shows. */
function MeshPreviewModal({
  gameId,
  map,
  onClose,
}: {
  gameId: string
  map: GameMapInfo
  onClose: () => void
}) {
  const [preview, setPreview] = useState<MapPreviewResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const token = useSession()?.token ?? ""

  useEffect(() => {
    let cancelled = false
    fetch(`${gameMapsPath(gameId)}/${map.version}/preview`, {
      headers: { accept: "application/json", authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(8000),
    })
      .then(async (response) => {
        if (!response.ok) throw new Error(`Preview failed (${response.status})`)
        return (await response.json()) as MapPreviewResponse
      })
      .then((body) => {
        if (!cancelled) setPreview(body)
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause))
      })
    return () => {
      cancelled = true
    }
  }, [gameId, map.version, token])

  return (
    <Modal
      open
      title={`🗺️ ${map.name || "Map"} v${map.version} (${FORMAT_LABELS[map.format] ?? map.format})`}
      description={preview?.note ?? undefined}
      onClose={onClose}
      footer={
        <Button variant="outline" onClick={onClose}>
          Close
        </Button>
      }
    >
      {error !== null ? (
        <p className="px-4 py-8 text-center text-sm text-red-400">{error}</p>
      ) : preview === null ? (
        <p className="px-4 py-8 text-center text-sm text-muted-foreground">Loading…</p>
      ) : preview.mesh !== null && preview.mesh !== undefined ? (
        <OxclMeshPreview mesh={preview.mesh} />
      ) : (
        <p className="px-4 py-8 text-center text-sm text-muted-foreground">
          {preview.note ?? "This format has no preview."}
        </p>
      )}
    </Modal>
  )
}

/** One stored `game_maps` row, without its bytes. */
export interface GameMapInfo {
  id: string
  version: number
  name: string
  format: "oxcl" | "map2d" | "map3d" | "navmesh"
  sha256: string
  is_active: boolean
  size_bytes: number
  created_at: string
}

const FORMAT_LABELS: Record<GameMapInfo["format"], string> = {
  oxcl: "3D mesh (OXCL)",
  map2d: "2D arena (JSON)",
  map3d: "3D arena (JSON)",
  navmesh: "Navmesh",
}

/** `format` per extension, mirroring the server's inference so the field the
 * upload sends always matches what the file actually is. */
const EXT_FORMATS: Record<string, GameMapInfo["format"]> = {
  oxcl: "oxcl",
  json: "map2d",
  nav2d: "navmesh",
  mesh: "navmesh",
}

function fmtBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / (1024 * 1024)).toFixed(2)} MiB`
}

function inferFormat(name: string): GameMapInfo["format"] | null {
  const ext = name.split(".").pop()?.toLowerCase() ?? ""
  return EXT_FORMATS[ext] ?? null
}

export interface ArenaObstacle {
  id?: number | string
  name?: string
  kind?: string
  x: number
  y: number
  z?: number
  w?: number
  h?: number
  d?: number
}

export interface ArenaData {
  name?: string
  width: number
  height: number
  depth?: number
  obstacles: ArenaObstacle[]
}

export function ArenaCanvasPreview({
  data,
  onUploadConfirm,
  formatLabel,
}: {
  data: ArenaData
  onUploadConfirm?: () => void
  formatLabel?: string
}) {
  const normalCanvasRef = useRef<HTMLCanvasElement>(null)
  const fullCanvasRef = useRef<HTMLCanvasElement>(null)

  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const [isDragging, setIsDragging] = useState(false)
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 })
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null)
  const [mouseWorld, setMouseWorld] = useState<{ x: number; y: number } | null>(null)
  const [searchQuery, setSearchQuery] = useState("")
  const [showInspector, setShowInspector] = useState(true)

  const drawScene = useCallback(
    (canvas: HTMLCanvasElement | null) => {
      if (!canvas) return
      const ctx = canvas.getContext("2d")
      if (!ctx) return

      const rect = canvas.getBoundingClientRect()
      if (rect.width === 0 || rect.height === 0) return

      const dpr = window.devicePixelRatio || 1
      const displayW = Math.round(rect.width)
      const displayH = Math.round(rect.height)

      if (canvas.width !== displayW * dpr || canvas.height !== displayH * dpr) {
        canvas.width = displayW * dpr
        canvas.height = displayH * dpr
      }

      ctx.save()
      ctx.scale(dpr, dpr)

      const W = displayW
      const H = displayH

      // Deep space grid background
      ctx.fillStyle = "#070b12"
      ctx.fillRect(0, 0, W, H)

      const arenaW = data.width || 800
      const arenaH = data.height || data.depth || 500
      const pad = 40

      const baseScale = Math.min((W - pad * 2) / arenaW, (H - pad * 2) / arenaH)
      const scale = baseScale * zoom

      const centerCanvasX = W / 2 + pan.x
      const centerCanvasY = H / 2 + pan.y

      const hasNegative = data.obstacles.some((o) => o.x < 0 || (o.z !== undefined && o.z < 0))

      const toScreenX = (x: number) => {
        if (hasNegative) {
          return centerCanvasX + x * scale
        }
        return centerCanvasX + (x - arenaW / 2) * scale
      }
      const toScreenY = (y: number) => {
        if (hasNegative) {
          return centerCanvasY + y * scale
        }
        return centerCanvasY + (y - arenaH / 2) * scale
      }

      const arenaLeft = toScreenX(hasNegative ? -arenaW / 2 : 0)
      const arenaTop = toScreenY(hasNegative ? -arenaH / 2 : 0)
      const arenaRight = toScreenX(hasNegative ? arenaW / 2 : arenaW)
      const arenaBottom = toScreenY(hasNegative ? arenaH / 2 : arenaH)
      const arenaRenderW = arenaRight - arenaLeft
      const arenaRenderH = arenaBottom - arenaTop

      // Arena Floor Fill
      ctx.fillStyle = "#0c131f"
      ctx.fillRect(arenaLeft, arenaTop, arenaRenderW, arenaRenderH)

      // Fine Grid
      ctx.strokeStyle = "rgba(255, 255, 255, 0.04)"
      ctx.lineWidth = 1
      const gridStep = Math.max(20, Math.round(arenaW / 16))
      const startX = hasNegative ? -arenaW / 2 : 0
      const endX = hasNegative ? arenaW / 2 : arenaW
      for (let x = startX; x <= endX; x += gridStep) {
        const sx = toScreenX(x)
        ctx.beginPath()
        ctx.moveTo(sx, arenaTop)
        ctx.lineTo(sx, arenaBottom)
        ctx.stroke()
      }
      const startY = hasNegative ? -arenaH / 2 : 0
      const endY = hasNegative ? arenaH / 2 : arenaH
      for (let y = startY; y <= endY; y += gridStep) {
        const sy = toScreenY(y)
        ctx.beginPath()
        ctx.moveTo(arenaLeft, sy)
        ctx.lineTo(arenaRight, sy)
        ctx.stroke()
      }

      // Arena Boundary with cyan glow
      ctx.strokeStyle = "#38bdf8"
      ctx.lineWidth = Math.max(2, 2 * Math.min(zoom, 1.8))
      ctx.strokeRect(arenaLeft, arenaTop, arenaRenderW, arenaRenderH)

      // Center crosshair (dashed)
      const cx = toScreenX(0)
      const cy = toScreenY(0)
      ctx.strokeStyle = "rgba(56, 189, 248, 0.3)"
      ctx.lineWidth = 1
      ctx.setLineDash([4, 4])
      ctx.beginPath()
      ctx.moveTo(cx, arenaTop)
      ctx.lineTo(cx, arenaBottom)
      ctx.moveTo(arenaLeft, cy)
      ctx.lineTo(arenaRight, cy)
      ctx.stroke()
      ctx.setLineDash([])

      // Center (0,0) mark
      ctx.fillStyle = "#38bdf8"
      ctx.beginPath()
      ctx.arc(cx, cy, 3, 0, Math.PI * 2)
      ctx.fill()
      ctx.font = "bold 9px system-ui, sans-serif"
      ctx.fillStyle = "rgba(56, 189, 248, 0.75)"
      ctx.fillText("(0,0)", cx + 5, cy - 5)

      // Obstacles
      const palette = [
        { fill: "rgba(56, 189, 248, 0.35)", stroke: "#38bdf8" },
        { fill: "rgba(245, 158, 11, 0.35)", stroke: "#f59e0b" },
        { fill: "rgba(16, 185, 129, 0.35)", stroke: "#10b981" },
        { fill: "rgba(236, 72, 153, 0.35)", stroke: "#ec4899" },
        { fill: "rgba(139, 92, 246, 0.35)", stroke: "#8b5cf6" },
        { fill: "rgba(6, 182, 212, 0.35)", stroke: "#06b6d4" },
      ]

      data.obstacles.forEach((ob, idx) => {
        const ow = (ob.w || 20) * scale
        const oh = (ob.d || ob.h || 20) * scale
        const posY = ob.z !== undefined ? ob.z : ob.y
        const sx = toScreenX(ob.x) - ow / 2
        const sy = toScreenY(posY) - oh / 2

        const isHovered = hoveredIndex === idx
        const color = palette[idx % palette.length]

        if (isHovered) {
          ctx.fillStyle = "rgba(255, 255, 255, 0.45)"
          ctx.strokeStyle = "#ffffff"
          ctx.lineWidth = 3
          ctx.shadowColor = color.stroke
          ctx.shadowBlur = 14
        } else {
          ctx.fillStyle = color.fill
          ctx.strokeStyle = color.stroke
          ctx.lineWidth = 1.5
          ctx.shadowColor = "transparent"
          ctx.shadowBlur = 0
        }

        ctx.fillRect(sx, sy, Math.max(3, ow), Math.max(3, oh))
        ctx.strokeRect(sx, sy, Math.max(3, ow), Math.max(3, oh))
        ctx.shadowBlur = 0

        // Label
        if (zoom >= 0.45 || isHovered) {
          const label = ob.name || `#${ob.id ?? idx + 1}`
          ctx.fillStyle = isHovered ? "#ffffff" : "#f1f5f9"
          ctx.font = isHovered ? "bold 11px system-ui, sans-serif" : "bold 9px system-ui, sans-serif"
          ctx.fillText(label, sx + 2, sy - 3)
        }
      })

      ctx.restore()
    },
    [data, zoom, pan, hoveredIndex]
  )

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return
    setIsDragging(true)
    setDragStart({ x: e.clientX - pan.x, y: e.clientY - pan.y })
  }

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (isDragging) {
      setPan({
        x: e.clientX - dragStart.x,
        y: e.clientY - dragStart.y,
      })
      return
    }

    const canvas = e.currentTarget
    const rect = canvas.getBoundingClientRect()
    const mouseX = e.clientX - rect.left
    const mouseY = e.clientY - rect.top

    const W = rect.width
    const H = rect.height
    const arenaW = data.width || 800
    const arenaH = data.height || data.depth || 500
    const pad = 40
    const baseScale = Math.min((W - pad * 2) / arenaW, (H - pad * 2) / arenaH)
    const scale = baseScale * zoom
    const centerCanvasX = W / 2 + pan.x
    const centerCanvasY = H / 2 + pan.y
    const hasNegative = data.obstacles.some((o) => o.x < 0 || (o.z !== undefined && o.z < 0))

    const worldX = hasNegative
      ? (mouseX - centerCanvasX) / scale
      : (mouseX - centerCanvasX) / scale + arenaW / 2
    const worldY = hasNegative
      ? (mouseY - centerCanvasY) / scale
      : (mouseY - centerCanvasY) / scale + arenaH / 2

    setMouseWorld({ x: Math.round(worldX), y: Math.round(worldY) })

    let foundIdx: number | null = null
    for (let i = data.obstacles.length - 1; i >= 0; i--) {
      const ob = data.obstacles[i]
      const ow = ob.w || 20
      const oh = ob.d || ob.h || 20
      const posY = ob.z !== undefined ? ob.z : ob.y
      if (
        worldX >= ob.x - ow / 2 &&
        worldX <= ob.x + ow / 2 &&
        worldY >= posY - oh / 2 &&
        worldY <= posY + oh / 2
      ) {
        foundIdx = i
        break
      }
    }
    setHoveredIndex(foundIdx)
  }

  const handleMouseUp = () => {
    setIsDragging(false)
  }

  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault()
    const canvas = e.currentTarget
    const rect = canvas.getBoundingClientRect()
    const mouseX = e.clientX - rect.left
    const mouseY = e.clientY - rect.top

    const zoomFactor = e.deltaY < 0 ? 1.15 : 0.85
    const newZoom = Math.min(10, Math.max(0.15, zoom * zoomFactor))

    const W = rect.width
    const H = rect.height
    const arenaW = data.width || 800
    const arenaH = data.height || data.depth || 500
    const pad = 40
    const baseScale = Math.min((W - pad * 2) / arenaW, (H - pad * 2) / arenaH)
    const oldScale = baseScale * zoom
    const newScale = baseScale * newZoom

    const centerCanvasX = W / 2 + pan.x
    const centerCanvasY = H / 2 + pan.y

    const newCenterCanvasX = mouseX - (mouseX - centerCanvasX) * (newScale / oldScale)
    const newCenterCanvasY = mouseY - (mouseY - centerCanvasY) * (newScale / oldScale)

    setPan({
      x: newCenterCanvasX - W / 2,
      y: newCenterCanvasY - H / 2,
    })
    setZoom(newZoom)
  }

  const resetView = () => {
    setZoom(1)
    setPan({ x: 0, y: 0 })
    setHoveredIndex(null)
  }

  const focusOnObstacle = (ob: ArenaObstacle, idx: number) => {
    setHoveredIndex(idx)
    const canvas = isFullscreen ? fullCanvasRef.current : normalCanvasRef.current
    if (!canvas) return
    const rect = canvas.getBoundingClientRect()
    const arenaW = data.width || 800
    const arenaH = data.height || data.depth || 500
    const pad = 40
    const baseScale = Math.min((rect.width - pad * 2) / arenaW, (rect.height - pad * 2) / arenaH)
    const focusZoom = Math.max(2, zoom)
    const newScale = baseScale * focusZoom

    const hasNegative = data.obstacles.some((o) => o.x < 0 || (o.z !== undefined && o.z < 0))
    const posY = ob.z !== undefined ? ob.z : ob.y
    const targetX = hasNegative ? ob.x : ob.x - arenaW / 2
    const targetY = hasNegative ? posY : posY - arenaH / 2

    setZoom(focusZoom)
    setPan({
      x: -targetX * newScale,
      y: -targetY * newScale,
    })
  }

  // Escape key in capture phase exits fullscreen cleanly without closing parent modal
  useEffect(() => {
    if (!isFullscreen) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation()
        setIsFullscreen(false)
        resetView()
      }
    }
    window.addEventListener("keydown", onKeyDown, true)
    return () => window.removeEventListener("keydown", onKeyDown, true)
  }, [isFullscreen])

  // Redraw when view changes
  useEffect(() => {
    if (isFullscreen) {
      drawScene(fullCanvasRef.current)
    } else {
      drawScene(normalCanvasRef.current)
    }
  }, [drawScene, isFullscreen])

  // Resize listener
  useEffect(() => {
    const handleResize = () => {
      if (isFullscreen) {
        drawScene(fullCanvasRef.current)
      } else {
        drawScene(normalCanvasRef.current)
      }
    }
    window.addEventListener("resize", handleResize)
    return () => window.removeEventListener("resize", handleResize)
  }, [drawScene, isFullscreen])

  const hoveredObstacle = hoveredIndex !== null ? data.obstacles[hoveredIndex] : null

  const filteredObstacles = data.obstacles.filter((ob, idx) => {
    if (!searchQuery.trim()) return true
    const q = searchQuery.toLowerCase()
    const name = (ob.name || `#${ob.id ?? idx + 1}`).toLowerCase()
    const kind = (ob.kind || "").toLowerCase()
    return name.includes(q) || kind.includes(q)
  })

  return (
    <>
      {/* Normal Mode View (inside modal dialog) */}
      <div className="space-y-4">
        <div className="relative overflow-hidden rounded-lg border border-border/70 bg-[#070b12] shadow-inner group">
          <canvas
            ref={normalCanvasRef}
            className="w-full h-[330px] block cursor-grab active:cursor-grabbing"
            onMouseDown={handleMouseDown}
            onMouseMove={handleMouseMove}
            onMouseUp={handleMouseUp}
            onMouseLeave={handleMouseUp}
            onWheel={handleWheel}
          />

          {/* Top-Right Controls */}
          <div className="absolute top-2.5 right-2.5 flex items-center gap-1.5 bg-black/80 backdrop-blur-md p-1 rounded-lg border border-white/10 shadow-lg z-10">
            <Button
              size="icon-xs"
              variant="ghost"
              onClick={() => setZoom((z) => Math.max(0.15, z * 0.8))}
              title="Zoom Out (-)"
            >
              <ZoomOut className="size-3.5" />
            </Button>
            <span className="font-mono text-[11px] px-1.5 min-w-10 text-center font-bold text-primary">
              {Math.round(zoom * 100)}%
            </span>
            <Button
              size="icon-xs"
              variant="ghost"
              onClick={() => setZoom((z) => Math.min(10, z * 1.25))}
              title="Zoom In (+)"
            >
              <ZoomIn className="size-3.5" />
            </Button>
            <Button size="icon-xs" variant="ghost" onClick={resetView} title="Fit View">
              <RotateCcw className="size-3.5" />
            </Button>
            <div className="h-3 w-px bg-white/20 mx-0.5" />
            <Button
              size="xs"
              variant="outline"
              onClick={() => {
                setIsFullscreen(true)
                resetView()
              }}
              title="Open Fullscreen"
              className="bg-primary/20 text-primary border-primary/40 hover:bg-primary/30 h-6 px-2 text-[11px] font-semibold"
            >
              <Maximize2 className="size-3 mr-1" />
              Full Screen
            </Button>
          </div>

          {/* Bottom Bar Info */}
          <div className="absolute bottom-2.5 left-2.5 flex items-center gap-2 bg-black/80 px-2.5 py-1 rounded text-[11px] text-muted-foreground font-mono backdrop-blur-xs border border-white/10">
            <span className="text-primary font-semibold">{data.width} × {data.height || data.depth}</span>
            <span>·</span>
            <span>{data.obstacles.length} colliders</span>
            <span className="text-muted-foreground/60 hidden sm:inline">· Drag &amp; scroll to move/zoom</span>
          </div>

          {/* Hover Card in Normal Mode */}
          {hoveredObstacle && (
            <div className="absolute top-2.5 left-2.5 z-10 bg-black/90 border border-primary/40 rounded px-2.5 py-1.5 text-[11px] font-mono text-muted-foreground shadow-lg backdrop-blur-md pointer-events-none">
              <span className="text-primary font-semibold">
                {hoveredObstacle.name || `#${hoveredObstacle.id ?? (hoveredIndex! + 1)}`}
              </span>
              : pos [{hoveredObstacle.x}, {hoveredObstacle.z !== undefined ? hoveredObstacle.z : hoveredObstacle.y}]
            </div>
          )}
        </div>

        {/* Normal Mode Colliders List with Click to Focus */}
        <div className="rounded-lg border border-border/60 bg-muted/20 p-3">
          <div className="flex items-center justify-between pb-2 border-b border-border/40">
            <span className="text-xs font-semibold text-foreground">
              Static Colliders ({data.obstacles.length}) · Click to Focus
            </span>
            {formatLabel && (
              <Badge variant="outline" className="font-mono text-[10px]">
                {formatLabel}
              </Badge>
            )}
          </div>

          <div className="mt-2 max-h-40 overflow-y-auto space-y-1.5 pr-1">
            {data.obstacles.length === 0 ? (
              <p className="text-xs text-muted-foreground py-2 text-center">No obstacles in this arena.</p>
            ) : (
              data.obstacles.map((ob, idx) => {
                const isSelected = hoveredIndex === idx
                return (
                  <div
                    key={idx}
                    onClick={() => focusOnObstacle(ob, idx)}
                    className={`flex items-center justify-between text-xs py-1.5 px-2 rounded border font-mono text-[11px] cursor-pointer transition-colors ${
                      isSelected
                        ? "bg-primary/20 border-primary text-foreground"
                        : "bg-background/50 border-border/40 hover:bg-muted/40 text-muted-foreground"
                    }`}
                  >
                    <div className="flex items-center gap-1.5">
                      <span className="text-primary font-medium">{ob.name || `#${ob.id ?? idx + 1}`}</span>
                      {ob.kind && <span className="text-[10px] text-muted-foreground">({ob.kind})</span>}
                    </div>
                    <span className="text-muted-foreground text-[10px]">
                      pos: [{ob.x}, {ob.z !== undefined ? ob.z : ob.y}] · size: [{ob.w ?? 20} × {ob.d || ob.h || 20}]
                    </span>
                  </div>
                )
              })
            )}
          </div>
        </div>
      </div>

      {/* Fullscreen Mode (Portal directly to document.body) */}
      {isFullscreen &&
        createPortal(
          <div className="fixed inset-0 z-[100] flex flex-col bg-[#070b12] text-foreground select-none overflow-hidden animate-in fade-in duration-150">
            {/* Fullscreen Header */}
            <header className="flex items-center justify-between border-b border-border/70 bg-card/90 backdrop-blur-md px-6 py-2.5 z-20">
              <div className="flex items-center gap-3">
                <h2 className="text-base font-bold text-foreground flex items-center gap-2">
                  🗺️ Arena Map Preview: <span className="text-primary font-mono">{data.name || "Map"}</span>
                </h2>
                <Badge variant="outline" className="font-mono text-xs">
                  {data.width} × {data.height || data.depth}
                </Badge>
                <Badge variant="secondary" className="font-mono text-xs">
                  {data.obstacles.length} colliders
                </Badge>
                {formatLabel && (
                  <Badge variant="outline" className="font-mono text-xs text-primary border-primary/40">
                    {formatLabel}
                  </Badge>
                )}
              </div>

              {/* Floating Navigation Controls */}
              <div className="flex items-center gap-2">
                <div className="flex items-center gap-1 bg-background/80 border border-border/70 rounded-lg p-1 shadow-md">
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    onClick={() => setZoom((z) => Math.max(0.15, z * 0.8))}
                    title="Zoom Out (-)"
                  >
                    <ZoomOut className="size-4" />
                  </Button>
                  <span className="font-mono text-xs px-2 min-w-14 text-center font-bold text-primary">
                    {Math.round(zoom * 100)}%
                  </span>
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    onClick={() => setZoom((z) => Math.min(10, z * 1.25))}
                    title="Zoom In (+)"
                  >
                    <ZoomIn className="size-4" />
                  </Button>
                  <div className="h-4 w-px bg-border/60 mx-1" />
                  <Button size="xs" variant="ghost" onClick={resetView} title="Fit Arena in View">
                    <RotateCcw className="size-3.5 mr-1" />
                    Fit View
                  </Button>
                </div>

                <Button
                  size="xs"
                  variant={showInspector ? "secondary" : "outline"}
                  onClick={() => setShowInspector(!showInspector)}
                  title="Toggle Colliders List"
                >
                  Colliders ({data.obstacles.length})
                </Button>

                <Button
                  size="xs"
                  variant="destructive"
                  onClick={() => {
                    setIsFullscreen(false)
                    resetView()
                  }}
                  title="Exit Fullscreen (ESC)"
                >
                  <Minimize2 className="size-3.5 mr-1.5" />
                  Back to Dialog (ESC)
                </Button>

                {onUploadConfirm && (
                  <Button
                    size="xs"
                    onClick={() => {
                      setIsFullscreen(false)
                      onUploadConfirm()
                    }}
                    className="bg-primary text-primary-foreground font-semibold"
                  >
                    <Rocket className="size-3.5 mr-1.5" />
                    Confirm &amp; Activate
                  </Button>
                )}
              </div>
            </header>

            {/* Fullscreen Body */}
            <div className="relative flex-1 w-full h-full overflow-hidden">
              <canvas
                ref={fullCanvasRef}
                className="w-full h-full block cursor-grab active:cursor-grabbing"
                onMouseDown={handleMouseDown}
                onMouseMove={handleMouseMove}
                onMouseUp={handleMouseUp}
                onMouseLeave={handleMouseUp}
                onWheel={handleWheel}
              />

              {/* Hovered Collider Floating Details */}
              {hoveredObstacle && (
                <div className="absolute top-4 left-4 z-20 bg-background/95 border border-primary/50 rounded-lg p-3 shadow-2xl backdrop-blur-md font-mono text-xs max-w-sm pointer-events-none animate-in fade-in duration-100">
                  <div className="flex items-center justify-between gap-2 border-b border-border/60 pb-1.5 mb-1.5">
                    <span className="font-bold text-primary text-sm">
                      {hoveredObstacle.name || `#${hoveredObstacle.id ?? (hoveredIndex! + 1)}`}
                    </span>
                    {hoveredObstacle.kind && (
                      <Badge variant="outline" className="text-[10px] px-1.5 py-0">
                        {hoveredObstacle.kind}
                      </Badge>
                    )}
                  </div>
                  <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
                    <span>Position:</span>
                    <span className="text-foreground text-right">
                      [{hoveredObstacle.x}, {hoveredObstacle.z !== undefined ? hoveredObstacle.z : hoveredObstacle.y}]
                    </span>
                    <span>Size (W×D/H):</span>
                    <span className="text-foreground text-right">
                      [{hoveredObstacle.w ?? 20} × {hoveredObstacle.d || hoveredObstacle.h || 20}]
                    </span>
                    {hoveredObstacle.z !== undefined && (
                      <>
                        <span>Elevation (Y):</span>
                        <span className="text-foreground text-right">{hoveredObstacle.y ?? 0}</span>
                      </>
                    )}
                  </div>
                </div>
              )}

              {/* Bottom Help and Status Bar */}
              <div className="absolute bottom-4 left-6 z-20 flex items-center gap-4 text-xs text-muted-foreground font-mono bg-black/80 backdrop-blur-md px-3.5 py-2 rounded-lg border border-white/10 shadow-xl">
                <span className="flex items-center gap-1.5 text-foreground">
                  <Move className="size-3.5 text-primary" /> Drag to move
                </span>
                <span>·</span>
                <span>Scroll to zoom in/out</span>
                <span>·</span>
                <span>Click collider to focus</span>
                {mouseWorld && (
                  <>
                    <span>·</span>
                    <span className="text-primary">Cursor: [{mouseWorld.x}, {mouseWorld.y}]</span>
                  </>
                )}
              </div>

              {/* Collapsible Colliders Inspector Sidebar */}
              {showInspector && (
                <aside className="absolute top-4 right-4 bottom-4 w-80 z-20 bg-card/95 border border-border/80 rounded-xl shadow-2xl backdrop-blur-md flex flex-col overflow-hidden animate-in slide-in-from-right-4 duration-200">
                  <div className="p-3 border-b border-border/60 flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-foreground">Colliders</span>
                      <Badge variant="secondary" className="font-mono text-[10px]">
                        {data.obstacles.length}
                      </Badge>
                    </div>
                    <Button size="icon-xs" variant="ghost" onClick={() => setShowInspector(false)} title="Close">
                      <X className="size-3" />
                    </Button>
                  </div>

                  <div className="p-2 border-b border-border/40">
                    <div className="relative">
                      <Search className="size-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
                      <input
                        type="text"
                        placeholder="Filter colliders..."
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full bg-background/60 border border-border/60 rounded-md pl-8 pr-2.5 py-1 text-xs text-foreground placeholder:text-muted-foreground outline-none focus:border-primary font-mono"
                      />
                    </div>
                  </div>

                  <div className="flex-1 overflow-y-auto p-2 space-y-1">
                    {filteredObstacles.length === 0 ? (
                      <p className="text-xs text-muted-foreground text-center py-4">No matching colliders.</p>
                    ) : (
                      filteredObstacles.map((ob, idx) => {
                        const originalIdx = data.obstacles.indexOf(ob)
                        const isSelected = hoveredIndex === originalIdx
                        return (
                          <button
                            key={idx}
                            onClick={() => focusOnObstacle(ob, originalIdx)}
                            className={`w-full text-left p-2 rounded-lg border transition-all text-xs font-mono flex flex-col gap-0.5 ${
                              isSelected
                                ? "bg-primary/20 border-primary text-foreground shadow-sm"
                                : "bg-background/40 border-border/40 hover:bg-muted/40 text-muted-foreground hover:text-foreground"
                            }`}
                          >
                            <div className="flex items-center justify-between">
                              <span className="font-semibold text-primary">
                                {ob.name || `#${ob.id ?? originalIdx + 1}`}
                              </span>
                              {ob.kind && (
                                <span className="text-[10px] text-muted-foreground">({ob.kind})</span>
                              )}
                            </div>
                            <div className="text-[10px] text-muted-foreground flex items-center justify-between">
                              <span>pos: [{ob.x}, {ob.z !== undefined ? ob.z : ob.y}]</span>
                              <span>size: [{ob.w ?? 20}×{ob.d || ob.h || 20}]</span>
                            </div>
                          </button>
                        )
                      })
                    )}
                  </div>
                </aside>
              )}
            </div>
          </div>,
          document.body
        )}
    </>
  )
}

/** The drop zone + format picker. A `.json` upload needs its dialect picked
 * (2D arena vs 3D arena); the other extensions carry their format alone. */
export function MapsUploadPanel({ gameId }: { gameId: string }) {
  const token = useSession()?.token ?? ""
  const [file, setFile] = useState<File | null>(null)
  const [format, setFormat] = useState<GameMapInfo["format"]>("map2d")
  const [dragActive, setDragActive] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const [previewData, setPreviewData] = useState<ArenaData | null>(null)
  const [previewOpen, setPreviewOpen] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)

  const maps = useConsoleQuery<GameMapInfo[]>(`game-maps-${gameId}`, gameMapsPath(gameId), {}, gameId !== "")

  async function accept(file: File) {
    setFile(file)
    const inferred = inferFormat(file.name)
    if (inferred !== null) setFormat(inferred)

    // Auto-parse JSON map files to show instant visual preview
    if (file.name.toLowerCase().endsWith(".json")) {
      try {
        const text = await file.text()
        const parsed = JSON.parse(text)
        if (parsed.arena_width || parsed.obstacles) {
          const is3d = parsed.arena_depth !== undefined || (parsed.obstacles?.[0]?.z !== undefined)
          if (is3d) setFormat("map3d")
          setPreviewData({
            name: file.name,
            width: Number(parsed.arena_width) || 800,
            height: Number(parsed.arena_height) || Number(parsed.arena_depth) || 500,
            depth: parsed.arena_depth ? Number(parsed.arena_depth) : undefined,
            obstacles: Array.isArray(parsed.obstacles) ? parsed.obstacles : [],
          })
          setPreviewOpen(true) // Open preview popup dialog immediately!
        } else {
          setPreviewData(null)
        }
      } catch {
        setPreviewData(null)
      }
    } else {
      setPreviewData(null)
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    setDragActive(false)
    const dropped = event.dataTransfer.files[0]
    if (dropped !== undefined) void accept(dropped)
  }

  function onPick(event: ChangeEvent<HTMLInputElement>) {
    const chosen = event.target.files?.[0]
    if (chosen !== undefined) void accept(chosen)
  }

  /** The one raw request the panel makes: multipart with upload progress. */
  function upload() {
    if (file === null || gameId === "" || token === "") return
    const form = new FormData()
    form.append("file", file)
    form.append("format", format)

    const xhr = new XMLHttpRequest()
    xhr.open("POST", `${gameMapsPath(gameId)}/upload`)
    xhr.setRequestHeader("authorization", `Bearer ${token}`)
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) setProgress(Math.round((event.loaded / event.total) * 100))
    }
    xhr.onload = () => {
      setProgress(null)
      let body: { map?: GameMapInfo; error?: { message?: string } } = {}
      try {
        body = JSON.parse(xhr.responseText) as typeof body
      } catch {
        // A non-JSON refusal still needs a line on screen.
      }
      const message = body.error?.message ?? `Upload refused (${xhr.status}).`
      if (xhr.status >= 200 && xhr.status < 300 && body.map !== undefined) {
        toast.success(`Map v${body.map.version} uploaded and active.`)
        setFile(null)
        if (fileInput.current !== null) fileInput.current.value = ""
        void maps.refetch()
      } else {
        toast.error(message)
      }
    }
    xhr.onerror = () => {
      setProgress(null)
      toast.error("The upload failed before the server answered.")
    }
    setProgress(0)
    xhr.send(form)
  }

  const formatOptions = Object.entries(FORMAT_LABELS) as [GameMapInfo["format"], string][]

  return (
    <Panel
      title="Upload map"
      description="A cooked .oxcl mesh, an arena JSON (2D/3D), or a baked navmesh — the server loads the active version at boot."
    >
      <div className="space-y-3 px-4 py-3">
        <div
          role="button"
          tabIndex={0}
          onClick={() => fileInput.current?.click()}
          onKeyDown={(event) => event.key === "Enter" && fileInput.current?.click()}
          onDragOver={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDragLeave={() => setDragActive(false)}
          onDrop={onDrop}
          className={`flex flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed px-4 py-8 text-center transition-colors ${
            dragActive
              ? "cursor-pointer border-primary bg-primary/5"
              : "cursor-pointer border-border hover:border-primary/50"
          }`}
        >
          <Upload className="size-5 text-muted-foreground" />
          <span className="text-sm font-medium text-foreground">
            {file === null ? "Drop the map here, or click to choose" : file.name}
          </span>
          <span className="text-xs text-muted-foreground">
            {file === null
              ? ".oxcl · .json (2D/3D arena) · .nav2d/.mesh (navmesh) — up to 32 MiB"
              : `${fmtBytes(file.size)} · ${FORMAT_LABELS[format]}`}
          </span>
        </div>
        <input
          ref={fileInput}
          type="file"
          accept=".oxcl,.json,.nav2d,.mesh"
          className="hidden"
          onChange={onPick}
        />

        <div className="flex flex-wrap items-center gap-3">
          <span className="text-xs font-medium text-muted-foreground">Format</span>
          <div className="flex flex-wrap gap-1.5">
            {formatOptions.map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => setFormat(value)}
                className={`rounded-md border px-2 py-1 text-xs transition-colors ${
                  format === value
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border text-muted-foreground hover:border-primary/40 hover:text-foreground"
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        {progress !== null && (
          <div className="space-y-1">
            <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
              <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
            </div>
            <span className="font-mono text-xs text-muted-foreground">{progress}%</span>
          </div>
        )}

        <div className="flex items-center justify-between pt-1">
          {previewData ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setPreviewOpen(true)}
              className="gap-1.5"
            >
              <Eye className="size-4 text-primary" />
              Preview map popup
            </Button>
          ) : (
            <div />
          )}

          <Button onClick={upload} disabled={file === null || gameId === "" || progress !== null}>
            {progress === null ? <Rocket /> : <Loader2 className="animate-spin" />}
            Upload &amp; activate
          </Button>
        </div>
      </div>

      <Modal
        open={previewOpen}
        title={previewData ? `🗺️ Arena Map Preview: ${file?.name ?? "Map"}` : "Map Preview"}
        description={
          previewData
            ? `${previewData.width} × ${previewData.height || previewData.depth} · ${previewData.obstacles.length} colliders (${FORMAT_LABELS[format]})`
            : undefined
        }
        onClose={() => setPreviewOpen(false)}
        footer={
          <>
            <Button variant="outline" onClick={() => setPreviewOpen(false)}>
              Close
            </Button>
            <Button
              onClick={() => {
                setPreviewOpen(false)
                upload()
              }}
              disabled={file === null || gameId === "" || progress !== null}
            >
              <Rocket className="size-4" />
              Confirm &amp; activate map
            </Button>
          </>
        }
      >
        {previewData && (
          <ArenaCanvasPreview
            data={previewData}
            formatLabel={FORMAT_LABELS[format]}
            onUploadConfirm={() => {
              setPreviewOpen(false)
              upload()
            }}
          />
        )}
      </Modal>
    </Panel>
  )
}

/** The stored versions and their activate / delete controls. */
export function MapsPanel({ gameId }: { gameId: string }) {
  const [activating, setActivating] = useState<GameMapInfo | null>(null)
  const [removing, setRemoving] = useState<GameMapInfo | null>(null)
  const [previewing, setPreviewing] = useState<GameMapInfo | null>(null)

  const maps = useConsoleQuery<GameMapInfo[]>(`game-maps-${gameId}`, gameMapsPath(gameId), {}, gameId !== "")

  const columns: Column<GameMapInfo>[] = [
    {
      header: "Version",
      cell: (row) => (
        <span className="flex items-center gap-2">
          <span className="font-medium text-foreground">v{row.version}</span>
          {row.is_active && <Badge>Active</Badge>}
        </span>
      ),
    },
    { header: "Name", cell: (row) => <span className="text-xs text-foreground">{dash(row.name)}</span> },
    {
      header: "Format",
      cell: (row) => (
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <MapIcon className="size-3.5" />
          {FORMAT_LABELS[row.format] ?? row.format}
        </span>
      ),
    },
    { header: "Size", cell: (row) => <Mono>{fmtBytes(row.size_bytes)}</Mono> },
    {
      header: "SHA-256",
      cell: (row) => (
        <span title={row.sha256}>
          <Mono>{`${row.sha256.slice(0, 12)}…`}</Mono>
        </span>
      ),
    },
    { header: "Uploaded", cell: (row) => <Mono>{formatTime(row.created_at)}</Mono> },
    {
      header: "",
      cell: (row) => (
        <div className="flex justify-end gap-1.5" onClick={(event) => event.stopPropagation()}>
          {!row.is_active && (
            <Button size="xs" variant="outline" onClick={() => setActivating(row)}>
              Activate v{row.version}
            </Button>
          )}
          <Button size="xs" variant="outline" onClick={() => setPreviewing(row)}>
            <Eye className="size-3.5" />
            3D
          </Button>
          {!row.is_active && (
            <Button size="xs" variant="ghost" onClick={() => setRemoving(row)}>
              Delete
            </Button>
          )}
        </div>
      ),
    },
  ]

  return (
    <>
      <Panel fill>
        <DataTable
          columns={columns}
          rows={maps.data ?? []}
          loading={gameId !== "" && maps.isPending}
          error={maps.data === undefined ? queryError(maps.error) : null}
          rowKey={(row) => String(row.version)}
          empty="No maps uploaded for this game yet — the server falls back to its project folder's local map files."
          fill
        />
      </Panel>

      <ConfirmAction
        open={activating !== null}
        onClose={() => setActivating(null)}
        title={activating === null ? "" : `Activate v${activating.version}`}
        message={
          activating === null
            ? ""
            : `New rooms spawn on ${activating.name} (${FORMAT_LABELS[activating.format]}). The version the server holds now stays stored, and a restart re-loads this choice either way.`
        }
        confirmLabel="Activate"
        method="POST"
        path={`${gameMapsPath(gameId)}/${activating?.version ?? 0}/activate`}
        success={`v${activating?.version ?? ""} is the active map.`}
      />

      {previewing !== null && (
        <MeshPreviewModal gameId={gameId} map={previewing} onClose={() => setPreviewing(null)} />
      )}

      <ConfirmAction
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={removing === null ? "" : `Delete v${removing.version}`}
        message="The version and its stored map are removed. The active version cannot be deleted, and this cannot be undone."
        confirmLabel="Delete version"
        destructive
        method="DELETE"
        path={removing === null ? gameMapsPath(gameId) : `${gameMapsPath(gameId)}/${removing.version}`}
        success={`v${removing?.version ?? ""} deleted.`}
      />
    </>
  )
}
