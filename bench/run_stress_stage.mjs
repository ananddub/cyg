import { execSync, spawn } from 'node:child_process';
import fs from 'node:fs';

const argv = process.argv.slice(2);
const arg = (name, def) => {
    const i = argv.indexOf(`--${name}`);
    return i >= 0 ? argv[i + 1] : def;
};

const ENGINE = arg('engine', 'cyg');
const CLIENTS = Number(arg('clients', 100));
const ROOMS = Number(arg('rooms', 10));
const SECONDS = Number(arg('seconds', 10));
const HZ = Number(arg('hz', 20));
const CONTAINER = ENGINE === 'cyg' ? 'cyg-bench' : 'colyseus-bench';

try {
    const running = execSync(`docker inspect -f '{{.State.Running}}' ${CONTAINER}`, { encoding: 'utf8' }).trim();
    if (running !== 'true') {
        console.error(`Container ${CONTAINER} is not running!`);
        process.exit(1);
    }
} catch (e) {
    console.error(`Container check failed: ${e.message}`);
    process.exit(1);
}

function parseMemToBytes(str) {
    const match = str.match(/([0-9.]+)\s*([a-zA-Z]+)/);
    if (!match) return 0;
    const val = parseFloat(match[1]);
    const unit = match[2].toLowerCase();
    if (unit.startsWith('k')) return val * 1024;
    if (unit.startsWith('m')) return val * 1024 * 1024;
    if (unit.startsWith('g')) return val * 1024 * 1024 * 1024;
    return val;
}

const statsFile = `/tmp/stats_${CONTAINER}.log`;
if (fs.existsSync(statsFile)) fs.unlinkSync(statsFile);

// Start background bash sampler
const samplerScript = `
while true; do
  docker stats ${CONTAINER} --no-stream --format "{{.CPUPerc}}#{{.MemUsage}}" >> ${statsFile} 2>/dev/null
  sleep 0.5
done
`;
const samplerProc = spawn('bash', ['-c', samplerScript], { detached: true, stdio: 'ignore' });

const reportFile = `/tmp/bench_report_${ENGINE}_${CLIENTS}.json`;
if (fs.existsSync(reportFile)) fs.unlinkSync(reportFile);

let cmd = '';
if (ENGINE === 'cyg') {
    const descriptor = arg('descriptor');
    if (!descriptor) {
        console.error('--descriptor <path to cyg.schema.bin of the game project> is required for the cyg engine');
        process.exit(1);
    }
    cmd = `ulimit -n 65536 && node --max-old-space-size=4096 bench/loadtest.mjs --url http://127.0.0.1:8124 --room ${arg('room', 'my_room')} --rooms ${ROOMS} --clients ${CLIENTS} --seconds ${SECONDS} --hz ${HZ} --descriptor ${descriptor} --json > ${reportFile}`;
} else {
    cmd = `ulimit -n 65536 && cd /mnt/SSD/Devloper/pannel/colyseus && NODE_OPTIONS="--max-old-space-size=4096" node_modules/.bin/tsx packages/example/bench_client.mjs --clients ${CLIENTS} --seconds ${SECONDS} --hz ${HZ} --room bench --rooms ${ROOMS} > ${reportFile}`;
}

const t0 = performance.now();
let exitCode = 0;
let errorMsg = null;
try {
    execSync(cmd, { stdio: ['ignore', 'ignore', 'pipe'], timeout: 300000 });
} catch (e) {
    exitCode = e.status || 1;
    errorMsg = (e.stderr ? e.stderr.toString() : '') || e.message;
}
const elapsed = (performance.now() - t0) / 1000;

try { process.kill(-samplerProc.pid, 'SIGKILL'); } catch (_) { try { samplerProc.kill('SIGKILL'); } catch (_) {} }

let maxCpu = 0;
let maxMemBytes = 0;
let maxMemStr = '0MiB';

if (fs.existsSync(statsFile)) {
    const lines = fs.readFileSync(statsFile, 'utf8').split('\n');
    for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed || !trimmed.includes('#')) continue;
        const [cpuPart, memPart] = trimmed.split('#');
        const cpu = parseFloat(cpuPart.replace('%', '')) || 0;
        if (cpu > maxCpu) maxCpu = cpu;
        const used = memPart.split('/')[0].trim();
        const bytes = parseMemToBytes(used);
        if (bytes > maxMemBytes) {
            maxMemBytes = bytes;
            maxMemStr = used;
        }
    }
    fs.unlinkSync(statsFile);
}

let parsed = null;
if (fs.existsSync(reportFile)) {
    try {
        const content = fs.readFileSync(reportFile, 'utf8');
        parsed = JSON.parse(content);
    } catch (e) {
        errorMsg = (errorMsg ? errorMsg + ' | ' : '') + `JSON parse error: ${e.message}`;
    }
}

const summary = {
    engine: ENGINE,
    container: CONTAINER,
    clients: CLIENTS,
    rooms: ROOMS,
    exitCode,
    elapsedSeconds: +elapsed.toFixed(1),
    peakCpuPercent: +maxCpu.toFixed(1),
    peakMem: maxMemStr,
    peakMemBytes: Math.round(maxMemBytes),
    report: parsed ? {
        clients: parsed.clients,
        joinMs: parsed.joinMs,
        firstStateMs: parsed.firstSnapshotMs || parsed.firstStateMs,
        updates: parsed.updates,
        traffic: parsed.traffic,
        inputs: parsed.inputs,
        errors: parsed.errors || []
    } : { error: errorMsg }
};

console.log(JSON.stringify(summary, null, 2));
