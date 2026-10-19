// The WASM Studio & Deployments view, exported the way `administration/index.ts`
// exports its area modules. The two panels are also re-exported so the Games
// detail tabs render the same upload + history the studio page does.

export { WasmUploadView } from "./index.tsx"
export { DeploymentHistory, WasmUploadPanel } from "./studio"
