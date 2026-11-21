// Build @cyg/core-ts: UMD drop-in (dist/cyg.js, parity with sdk/client/web)
// and ESM (dist/cyg-core.mjs) from the same source modules.
import { build } from "esbuild";
import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));

await mkdir(join(root, "dist"), { recursive: true });

// UMD banner/wrapper: mirror the legacy cyg.js shell so drop-in consumers
// (`window.CYG`, AMD, CommonJS) keep working untouched. The factory body is a
// bundled CJS module executed inside the factory and returning its exports.
const umdBanner =
    "(function (root, factory) {\n" +
    "    if (typeof define === 'function' && define.amd) {\n" +
    "        define([], factory);\n" +
    "    } else if (typeof module === 'object' && module.exports) {\n" +
    "        module.exports = factory();\n" +
    "    } else {\n" +
    "        root.CYG = factory();\n" +
    "    }\n" +
    "}(typeof self !== 'undefined' ? self : typeof globalThis !== 'undefined' ? globalThis : this, function () {\n" +
    "    const module = { exports: {} };\n" +
    "    const exports = module.exports;\n";
const umdFooter = "\n    return module.exports;\n}));\n";

const shared = {
    entryPoints: [join(root, "src/index.ts")],
    bundle: true,
    minify: false,
    sourcemap: true,
    logLevel: "info",
};

await build({
    // Flat drop-in entry (module.exports = legacy surface + typed additions),
    // so both `require()` and ESM `import CYG from` see CYG.Room directly.
    entryPoints: [join(root, "src/umd.js")],
    bundle: true,
    minify: false,
    sourcemap: true,
    logLevel: "info",
    format: "cjs",
    outfile: join(root, "dist/cyg-core.tmp.cjs"),
});

// Wrap the bundled CJS body in the UMD shell.
const cjsBody = await readFile(join(root, "dist/cyg-core.tmp.cjs"), "utf8");
await writeFile(join(root, "dist/cyg.cjs"), umdBanner + cjsBody + umdFooter);

await build({
    ...shared,
    format: "esm",
    outfile: join(root, "dist/cyg-core.mjs"),
});

await build({
    ...shared,
    format: "cjs",
    outfile: join(root, "dist/cyg-core.cjs"),
});

// Type declarations via tsc (esbuild strips types only).
execFileSync("npx", ["tsc", "--emitDeclarationOnly", "--declaration", "true"], {
    cwd: root,
    stdio: "inherit",
});

// Keep the legacy hand-written surface types alongside the generated ones.
await cp(
    join(root, "src/types/legacy.d.ts"),
    join(root, "dist/legacy.d.ts"),
);

console.log("==> built dist/cyg.cjs (UMD), dist/cyg-core.mjs (ESM), dist/*.d.ts");
