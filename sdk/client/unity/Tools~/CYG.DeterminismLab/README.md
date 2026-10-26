# CYG Determinism Lab

A small browser lab for the fixed-point simulation. The page compiles the same
`CYGDeterminismProbe` used by the Unity tests into WebAssembly, runs the golden
vector in the browser, and reports the result to a small ASP.NET host. Every
device that opens the page shows up in one table, so a hash mismatch is visible
immediately instead of being assumed away.

Expected values:

```
CRC-32   0xE9D6E0FF
FNV-1a64 0x76E67011297DCBA1
```

## Run

Publish first. Running from source in the Production environment does not
serve the client's static web assets, so `/` returns 404:

```bash
cd sdk/unity/Tools~/CYG.DeterminismLab
dotnet publish CYG.DeterminismLab.Server -c Release -o CYG.DeterminismLab.Server/bin/publish
cd CYG.DeterminismLab.Server/bin/publish
ASPNETCORE_URLS=http://0.0.0.0:5199 dotnet CYG.DeterminismLab.Server.dll
```

For a quick local loop, `ASPNETCORE_ENVIRONMENT=Development dotnet run --project
CYG.DeterminismLab.Server -c Release --urls http://0.0.0.0:5199` also works.

Open `http://localhost:5199` on the host, then open
`http://<host-lan-ip>:5199` on phones/tablets/other laptops on the same
network. Each browser runs the probe and posts its result; the table refreshes
every two seconds.

Reports live in memory and are keyed by a browser-local id (`localStorage`), so
re-running a browser replaces that browser's row. Restarting the server clears
the list. The label in the input box is saved per browser.

## What it does and does not prove

- It runs the exact `sdk/unity/Runtime/Sim` sources through the .NET WebAssembly
  runtime, so it adds a wasm32 runtime and browser JIT to the parity matrix.
- Same CRC + FNV on every row means those browser runtimes produced
  bit-identical fixed-point values.
- It is not a substitute for a Unity IL2CPP/ARM device run. For those, use
  `CYGDeterminismSelfCheck` in a player build and compare its log line with
  these values.

The client project links the simulation sources directly, so changing the
probe or the fixed-point core automatically changes what this lab tests.
