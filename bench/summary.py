import json, sys
for stage in sys.argv[1:]:
    try:
        d = json.load(open(f"/tmp/mt_cyg_{stage}.json"))
    except Exception:
        print(f"{stage:<6} (no report)")
        continue
    raw = d["perProcRaw"]
    upd = sum(r.get("updates", {}).get("perClientPerSecond", 0) for r in raw) / max(1, len(raw))
    c = d["container"]
    print(f"{stage:<6} {d['joined']:>8} {d['errors']:>8} {c['maxCpuPct']:>7.1f}% {c['maxMemMiB']:>8}MiB oom={c['oomState']:>5} upd/s={upd:.1f}")
