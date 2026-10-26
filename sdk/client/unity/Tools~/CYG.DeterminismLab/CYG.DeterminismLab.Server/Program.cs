using System.Collections.Concurrent;
using CYG.DeterminismLab.Server;

const uint GoldenCrc32 = 0xE9D6E0FF;
const ulong GoldenFnv1a64 = 0x76E67011297DCBA1UL;
const int GoldenIterations = 10000;
const uint GoldenSeed = 1337;

var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();

app.UseBlazorFrameworkFiles();
app.UseDefaultFiles();
app.UseStaticFiles();

var reports = new ConcurrentDictionary<string, DeviceReport>(StringComparer.Ordinal);

app.MapGet("/api/health", () => Results.Ok(new { ok = true }));

app.MapGet("/api/reports", (HttpContext context) =>
{
    context.Response.Headers.CacheControl = "no-store";

    DeviceReport[] current = reports.Values
        .OrderBy(r => r.LastSeenUtc)
        .ToArray();

    return Results.Ok(new
    {
        expected = new
        {
            crc = $"0x{GoldenCrc32:X8}",
            fnv = $"0x{GoldenFnv1a64:X16}",
            iterations = GoldenIterations,
            seed = GoldenSeed
        },
        reports = current,
        summary = new
        {
            count = current.Length,
            matching = current.Count(r => r.Ok),
            mismatched = current.Count(r => !r.Ok),
            distinctCrc = current.Select(r => r.Crc32).Distinct().Count(),
            distinctFnv = current.Select(r => r.Fnv1a64).Distinct().Count(),
            allMatch = current.Length > 0 && current.All(r => r.Ok)
        }
    });
});

app.MapPost("/api/reports", (HttpRequest request, ReportRequest body) =>
{
    string deviceId = Clean(body.DeviceId, 128);
    if (deviceId.Length == 0)
    {
        return Results.BadRequest(new { error = "deviceId is required" });
    }

    if (body.Iterations != GoldenIterations || body.Seed != GoldenSeed)
    {
        return Results.BadRequest(new
        {
            error = $"only the canonical vector ({GoldenIterations} iterations, seed {GoldenSeed}) is accepted"
        });
    }

    string label = Clean(body.Label, 64);
    if (label.Length == 0)
    {
        label = "Unnamed device";
    }

    bool ok = body.Crc32 == GoldenCrc32 && body.Fnv1a64 == GoldenFnv1a64;
    DateTimeOffset now = DateTimeOffset.UtcNow;
    reports.TryGetValue(deviceId, out DeviceReport? existing);

    DeviceReport report = new(
        deviceId,
        label,
        Clean(body.Runtime, 120),
        Clean(body.Platform, 120),
        Clean(body.Language, 32),
        Clean(request.Headers.UserAgent.ToString(), 300),
        request.HttpContext.Connection.RemoteIpAddress?.ToString() ?? "",
        body.Crc32,
        body.Fnv1a64,
        $"0x{body.Crc32:X8}",
        $"0x{body.Fnv1a64:X16}",
        body.Iterations,
        body.Seed,
        ok,
        Math.Clamp(body.ElapsedMs, 0, 600_000),
        existing?.FirstSeenUtc ?? now,
        now);

    reports[deviceId] = report;

    app.Logger.LogInformation(
        "report device={Label} crc={Crc} fnv={Fnv} ok={Ok} browser={UserAgent}",
        report.Label,
        report.CrcHex,
        report.FnvHex,
        report.Ok,
        report.UserAgent);

    return Results.Ok(report);
});

app.MapFallbackToFile("index.html");
app.Run();

static string Clean(string? value, int maxLength)
{
    if (string.IsNullOrWhiteSpace(value))
    {
        return string.Empty;
    }

    string trimmed = value.Trim();
    return trimmed.Length <= maxLength ? trimmed : trimmed[..maxLength];
}
