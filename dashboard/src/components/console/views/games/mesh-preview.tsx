// A hand-written WebGL2 viewer for one OXCL cooked collision mesh. The plan's
// budget rule says prefer a small hand-written renderer over pulling `three`
// into the bundle, so this is matrices and buffers, no scene graph: orbit by
// dragging, zoom by wheel, headlight shading from the face normal.

import { useEffect, useRef, useState } from "react"

export interface OxclMesh {
  vertices: [number, number, number][]
  indices: number[]
}

const VERTEX_SHADER = `#version 300 es
in vec3 position;
uniform mat4 mvp;
uniform mat4 model;
out vec3 world;
void main() {
  world = (model * vec4(position, 1.0)).xyz;
  gl_Position = mvp * vec4(position, 1.0);
}`

const FRAGMENT_SHADER = `#version 300 es
precision highp float;
in vec3 world;
out vec4 color;
uniform vec3 eye;
void main() {
  vec3 normal = normalize(cross(dFdx(world), dFdy(world)));
  float light = abs(dot(normal, normalize(eye - world)));
  vec3 tint = vec3(0.48, 0.72, 1.0);
  color = vec4(tint * (0.25 + 0.75 * light), 1.0);
}`

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type)!
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? "shader compile failed")
  }
  return shader
}

export function OxclMeshPreview({ mesh }: { mesh: OxclMesh }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (canvas === null) return
    const gl = canvas.getContext("webgl2", { antialias: true })
    if (gl === null) {
      setError("This browser exposes no WebGL2; the mesh stays numeric.")
      return
    }

    let program: WebGLProgram
    try {
      program = gl.createProgram()!
      gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX_SHADER))
      gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT_SHADER))
      gl.linkProgram(program)
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
        throw new Error(gl.getProgramInfoLog(program) ?? "link failed")
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
      return
    }

    const positions = new Float32Array(mesh.vertices.flat())
    const indices = new Uint32Array(mesh.indices)
    const vao = gl.createVertexArray()!
    gl.bindVertexArray(vao)
    const vbo = gl.createBuffer()!
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo)
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW)
    const position = gl.getAttribLocation(program, "position")
    gl.enableVertexAttribArray(position)
    gl.vertexAttribPointer(position, 3, gl.FLOAT, false, 0, 0)
    const ibo = gl.createBuffer()!
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ibo)
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW)

    gl.enable(gl.DEPTH_TEST)
    gl.clearColor(0.06, 0.07, 0.1, 1.0)

    // Center and scale the mesh into a unit ball around the origin.
    let minX = Infinity
    let minY = Infinity
    let minZ = Infinity
    let maxX = -Infinity
    let maxY = -Infinity
    let maxZ = -Infinity
    for (const [x, y, z] of mesh.vertices) {
      minX = Math.min(minX, x)
      minY = Math.min(minY, y)
      minZ = Math.min(minZ, z)
      maxX = Math.max(maxX, x)
      maxY = Math.max(maxY, y)
      maxZ = Math.max(maxZ, z)
    }
    const cx = (minX + maxX) / 2
    const cy = (minY + maxY) / 2
    const cz = (minZ + maxZ) / 2
    const radius = Math.max(maxX - minX, maxY - minY, maxZ - minZ, 0.001) / 2

    const state = { yaw: 0.6, pitch: 0.5, distance: radius * 4 }
    const mvpLocation = gl.getUniformLocation(program, "mvp")
    const modelLocation = gl.getUniformLocation(program, "model")
    const eyeLocation = gl.getUniformLocation(program, "eye")

    function perspective(fov: number, aspect: number, near: number, far: number): Float32Array {
      const f = 1 / Math.tan(fov / 2)
      const nf = 1 / (near - far)
      return new Float32Array([
        f / aspect, 0, 0, 0,
        0, f, 0, 0,
        0, 0, (far + near) * nf, -1,
        0, 0, 2 * far * near * nf, 0,
      ])
    }

    function frame() {
      const rect = canvas!.getBoundingClientRect()
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      const width = Math.max(1, Math.floor(rect.width * dpr))
      const height = Math.max(1, Math.floor(rect.height * dpr))
      if (canvas!.width !== width || canvas!.height !== height) {
        canvas!.width = width
        canvas!.height = height
      }
      gl!.viewport(0, 0, width, height)
      gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT)
      const cosP = Math.cos(state.pitch)
      const eye = [
        cx + state.distance * cosP * Math.sin(state.yaw),
        cy + state.distance * Math.sin(state.pitch),
        cz + state.distance * cosP * Math.cos(state.yaw),
      ]
      const proj = perspective(Math.PI / 4, width / height, radius * 0.01, radius * 40)
      // A look-at matrix, written out because the alternative is a helper library.
      let zx = cx - eye[0]
      let zy = cy - eye[1]
      let zz = cz - eye[2]
      const zl = Math.hypot(zx, zy, zz) || 1
      zx /= zl
      zy /= zl
      zz /= zl
      let xx = zz * 0 - 1 * zy
      let xy = 0 * zx - 0 * zz
      let xz = 1 * zy - 0 * zx
      const xl = Math.hypot(xx, xy, xz) || 1
      xx /= xl
      xy /= xl
      xz /= xl
      const yx = zy * xz - zz * xy
      const yy = zz * xx - zx * xz
      const yz = zx * xy - zy * xx
      const view = new Float32Array([
        xx, yx, zx, 0,
        xy, yy, zy, 0,
        xz, yz, zz, 0,
        -(xx * eye[0] + xy * eye[1] + xz * eye[2]),
        -(yx * eye[0] + yy * eye[1] + yz * eye[2]),
        -(zx * eye[0] + zy * eye[1] + zz * eye[2]),
        1,
      ])
      const viewProj = multiply(proj, view)
      const model = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1])
      gl!.useProgram(program!)
      gl!.bindVertexArray(vao!)
      gl!.uniformMatrix4fv(mvpLocation!, false, viewProj)
      gl!.uniformMatrix4fv(modelLocation!, false, model)
      gl!.uniform3f(eyeLocation!, eye[0], eye[1], eye[2])
      gl!.drawElements(gl!.TRIANGLES, indices.length, gl!.UNSIGNED_INT, 0)
      animation = requestAnimationFrame(frame)
    }

    function multiply(a: Float32Array, b: Float32Array): Float32Array {
      const out = new Float32Array(16)
      for (let row = 0; row < 4; row += 1) {
        for (let col = 0; col < 4; col += 1) {
          let sum = 0
          for (let k = 0; k < 4; k += 1) {
            sum += a[k * 4 + col] * b[row * 4 + k]
          }
          out[row * 4 + col] = sum
        }
      }
      return out
    }

    let dragging = false
    let lastX = 0
    let lastY = 0
    const onDown = (event: PointerEvent) => {
      dragging = true
      lastX = event.clientX
      lastY = event.clientY
      canvas!.setPointerCapture(event.pointerId)
    }
    const onMove = (event: PointerEvent) => {
      if (!dragging) return
      state.yaw += (event.clientX - lastX) * 0.01
      state.pitch = Math.max(-1.5, Math.min(1.5, state.pitch + (event.clientY - lastY) * 0.01))
      lastX = event.clientX
      lastY = event.clientY
    }
    const onUp = () => {
      dragging = false
    }
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      state.distance = Math.max(radius * 1.2, state.distance * (1 + event.deltaY * 0.001))
    }
    canvas.addEventListener("pointerdown", onDown)
    canvas.addEventListener("pointermove", onMove)
    canvas.addEventListener("pointerup", onUp)
    canvas.addEventListener("wheel", onWheel, { passive: false })

    let animation = requestAnimationFrame(frame)
    return () => {
      cancelAnimationFrame(animation)
      canvas.removeEventListener("pointerdown", onDown)
      canvas.removeEventListener("pointermove", onMove)
      canvas.removeEventListener("pointerup", onUp)
      canvas.removeEventListener("wheel", onWheel)
      gl.deleteBuffer(vbo)
      gl.deleteBuffer(ibo)
      gl.deleteVertexArray(vao)
      gl.deleteProgram(program)
    }
  }, [mesh])

  if (error !== null) {
    return <p className="px-4 py-8 text-center text-sm text-muted-foreground">{error}</p>
  }
  return (
    <div className="relative">
      <canvas ref={canvasRef} className="h-80 w-full cursor-grab rounded-md bg-slate-950" />
      <p className="absolute right-2 bottom-2 text-xs text-muted-foreground">
        {mesh.vertices.length.toLocaleString()} vertices · drag to orbit · wheel to zoom
      </p>
    </div>
  )
}
