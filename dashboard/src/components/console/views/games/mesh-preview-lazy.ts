// The WebGL chunk is loaded only when someone actually opens a 3D preview, so
// the console's main bundle never carries the renderer.

import { lazy } from "react"

export const OxclMeshPreview = lazy(() =>
  import("./mesh-preview").then((module) => ({ default: module.OxclMeshPreview })),
)
