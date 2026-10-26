namespace CYG.DeterminismLab.Server;

public sealed class ReportRequest
{
    public string? DeviceId { get; set; }
    public string? Label { get; set; }
    public string? Runtime { get; set; }
    public string? Platform { get; set; }
    public string? Language { get; set; }
    public uint Crc32 { get; set; }
    public ulong Fnv1a64 { get; set; }
    public int Iterations { get; set; }
    public uint Seed { get; set; }
    public bool Ok { get; set; }
    public long ElapsedMs { get; set; }
}

public sealed record DeviceReport(
    string DeviceId,
    string Label,
    string Runtime,
    string Platform,
    string Language,
    string UserAgent,
    string RemoteAddress,
    uint Crc32,
    ulong Fnv1a64,
    string CrcHex,
    string FnvHex,
    int Iterations,
    uint Seed,
    bool Ok,
    long ElapsedMs,
    DateTimeOffset FirstSeenUtc,
    DateTimeOffset LastSeenUtc);
