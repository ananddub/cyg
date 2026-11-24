#!/usr/bin/env node
/**
 * Multi-process loadtest spawner for high-CCU ladders (single Node process
 * cannot consume 10k clients). Splits CLIENTS across PROCS identical
 * loadtest/bench_client children, samples the bench container's CPU/RSS via
 * docker stats, and aggregates one JSON report.
 *
 *   node bench/spawn_mt.mjs --engine cyg --container cyg-bench \
 *       --procs 10 --clients 500 --rooms 640 --seconds 30 --hz 20 --stage 5k
 */
import { spawn, execSync } from 'node:child_process';
import fs from 'node:fs';

const argv = process.argv.slice(2);
const arg = (name, def) => {
    const i = argv.indexOf(`--${name}`);
    return i >= 0 ? argv[i + 1] : def;
};
const ENGINE = arg('engine', 'cyg');
const CONTAINER = arg('container', ENGINE === 'cyg' ? 'cyg-bench' : 'colyseus-bench');
const PROCS = Number(arg('procs', 10));
const PER_PROC = Number(arg('clients', 500));
const ROOMS = Number(arg('rooms', 640));
const SECONDS = Number(arg('seconds', 30));
const HZ = Number(arg('hz', 20));
const RAMP = Number(arg('ramp', 250));
const STAGGER = Number(arg('stagger', 0));
// CYG endpoint overrides so the ladder can target any example game, not just
// the no-op bench room (e.g. jump-box-2d at :7796 with real Rapier2D physics).
const CYG_URL = arg('cyg-url', 'http://127.0.0.1:8297');
const CYG_ROOM = arg('cyg-room', 'free');
const CYG_DESC = arg('cyg-descriptor', 'bench-game-10k/out/cyg.schema.bin');
// Join deadline per client. Real-physics rooms need a longer one under
// saturation; a genuine hang still fails, just later.
const JOIN_TIMEOUT_MS = arg('join-timeout', '15000');
const STAGE = arg('stage', `${PROCS * PER_PROC}`);
const OUT = arg('out', `/tmp/mt_${ENGINE}_${STAGE}.json`);

const statsFile = `/tmp/stats_${CONTAINER}_${STAGE}.log`;
if (fs.existsSync(statsFile)) fs.unlinkSync(statsFile);
const sampler = spawn('bash', ['-c', `
while true; do
  docker stats ${CONTAINER} --no-stream --format "{{.CPUPerc}}#{{.MemUsage}}" >> ${statsFile} 2>/dev/null
  sleep 0.5
done`], { detached: true, stdio: 'ignore' });

const childCmds = [];
for (let p = 0; p < PROCS; p++) {
    childCmds.push(ENGINE === 'cyg'
        ? { cmd: 'node', args: ['bench/loadtest.mjs', '--url', CYG_URL, '--room', CYG_ROOM, '--rooms', String(ROOMS), '--clients', String(PER_PROC), '--seconds', String(SECONDS), '--hz', String(HZ), '--ramp', String(RAMP), '--timeout', String(JOIN_TIMEOUT_MS), '--descriptor', CYG_DESC, '--json'], cwd: '/mnt/SSD/Devloper/pannel/openoxide-gaming' }
        : { cmd: 'node_modules/.bin/tsx', args: ['packages/example/bench_client.mjs', '--clients', String(PER_PROC), '--seconds', String(SECONDS), '--hz', String(HZ), '--room', 'bench', '--rooms', String(ROOMS)], cwd: '/mnt/SSD/Devloper/pannel/colyseus' });
}

const t0 = performance.now();
const fs2 = await import('node:fs');
const children = await Promise.all(childCmds.map((c, i) => new Promise((resolve) => {
    const launch = () => {
        const outFile = `/tmp/mt_child_${STAGE}_${i}.json`;
        const fh = fs2.openSync(outFile, 'w');
        const ch = spawn(c.cmd, c.args, {
            cwd: c.cwd,
            stdio: ['ignore', fh, 'pipe'],
            env: { ...process.env, NODE_OPTIONS: '--max-old-space-size=4096' },
        });
        fs2.closeSync(fh);
        let err = '';
        ch.stderr.on('data', (d) => (err += d));
        ch.on('error', (e) => { err += String(e); });
        resolve({ ch, out: () => { try { return fs2.readFileSync(outFile, 'utf8'); } catch (_) { return ''; } }, err: () => err, exit: new Promise((r) => ch.on('close', (code) => r(code))) });
    };
    setTimeout(launch, i * STAGGER * 1000);
})));

const codes = await Promise.all(children.map((c) => c.exit));
const elapsed = (performance.now() - t0) / 1000;
try { process.kill(-sampler.pid, 'SIGKILL'); } catch (_) { try { sampler.kill('SIGKILL'); } catch (_) {} }

let oom = false;
try {
    oom = execSync(`docker inspect -f '{{.State.OOMKilled}} {{.State.ExitCode}}' ${CONTAINER}`, { encoding: 'utf8' }).trim();
} catch (_) {}

// aggregate
const reports = children.map((c, i) => {
    const text = c.out();
    const start = text.indexOf('{');
    try { return JSON.parse(text.slice(start)); } catch (e) {
        console.error(`proc ${i} parse failed: ${e.message} len=${text.length} tail=${JSON.stringify(text.slice(-150))}`);
        const err = c.err();
        if (err) console.error(`proc ${i} stderr: ${err.slice(-400)}`);
        return null;
    }
});
const ok = reports.filter(Boolean);
const sum = (f) => ok.reduce((a, r) => a + f(r), 0);
const joined = sum((r) => r.clients?.joined ?? 0);
const fatal = PROCS * PER_PROC - joined;
const errs = sum((r) => r.errors?.count ?? r.errors ?? 0);
let maxCpu = 0, maxMem = 0;
if (fs.existsSync(statsFile)) {
    for (const line of fs.readFileSync(statsFile, 'utf8').split('\n')) {
        const [cpu, mem] = line.trim().split('#');
        if (!mem) continue;
        maxCpu = Math.max(maxCpu, parseFloat(cpu));
        const m = mem.match(/([0-9.]+)\s*(MiB|GiB)/);
        if (m) maxMem = Math.max(maxMem, parseFloat(m[1]) * (m[2] === 'GiB' ? 1024 : 1));
    }
}
const base = ok[0] || {};
const report = {
    engine: ENGINE, stage: STAGE, cygUrl: ENGINE === 'cyg' ? CYG_URL : undefined, procs: PROCS, perProc: PER_PROC,
    exitCodes: codes, exitOk: codes.every((c) => c === 0),
    joined, fatal, errors: errs,
    elapsedSec: +elapsed.toFixed(1),
    container: { maxCpuPct: maxCpu, maxMemMiB: Math.round(maxMem), oomState: oom },
    joinP50: base.joinMs?.p50 ?? base.joinMs?.p50, joinP95: base.joinMs?.p95,
    updatesPerClientPerSec: base.updates?.perClientPerSecond,
    bytesInPerClient: base.traffic?.bytesInPerClient, bytesOutPerClient: base.traffic?.bytesOutPerClient,
    perProcRaw: ok,
};
fs.writeFileSync(OUT, JSON.stringify(report, null, 1));
console.log(`STAGE ${STAGE} ${ENGINE}: joined=${joined}/${PROCS * PER_PROC} errors=${errs} exit=${codes} cpu=${maxCpu}% mem=${Math.round(maxMem)}MiB oom=${oom} elapsed=${elapsed.toFixed(0)}s -> ${OUT}`);
if (fatal > 0 || !codes.every((c) => c === 0)) {
    for (const c of children) { const e = c.err(); if (e) console.error(e.slice(-500)); }
}
process.exit(fatal > 0 ? 1 : 0);
