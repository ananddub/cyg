namespace CYG.DeterminismLab.Client;

public sealed class LocalReport
{
    public string Runtime { get; set; } = "";
    public string Architecture { get; set; } = "";
    public string Platform { get; set; } = "";
    public string Language { get; set; } = "";
    public string CrcHex { get; set; } = "";
    public string FnvHex { get; set; } = "";
    public uint Crc32 { get; set; }
    public ulong Fnv1a64 { get; set; }
    public bool Ok { get; set; }
    public long ElapsedMs { get; set; }
}

public sealed class ReportRequest
{
    public string DeviceId { get; set; } = "";
    public string Label { get; set; } = "";
    public string Runtime { get; set; } = "";
    public string Platform { get; set; } = "";
    public string Language { get; set; } = "";
    public uint Crc32 { get; set; }
    public ulong Fnv1a64 { get; set; }
    public int Iterations { get; set; }
    public uint Seed { get; set; }
    public bool Ok { get; set; }
    public long ElapsedMs { get; set; }
}

public sealed class DeviceReport
{
    public string DeviceId { get; set; } = "";
    public string Label { get; set; } = "";
    public string Runtime { get; set; } = "";
    public string Platform { get; set; } = "";
    public string Language { get; set; } = "";
    public string UserAgent { get; set; } = "";
    public string RemoteAddress { get; set; } = "";
    public uint Crc32 { get; set; }
    public ulong Fnv1a64 { get; set; }
    public string CrcHex { get; set; } = "";
    public string FnvHex { get; set; } = "";
    public int Iterations { get; set; }
    public uint Seed { get; set; }
    public bool Ok { get; set; }
    public long ElapsedMs { get; set; }
    public DateTimeOffset FirstSeenUtc { get; set; }
    public DateTimeOffset LastSeenUtc { get; set; }
}

public sealed class ExpectedValues
{
    public string Crc { get; set; } = "";
    public string Fnv { get; set; } = "";
    public int Iterations { get; set; }
    public uint Seed { get; set; }
}

public sealed class ReportSummary
{
    public int Count { get; set; }
    public int Matching { get; set; }
    public int Mismatched { get; set; }
    public int DistinctCrc { get; set; }
    public int DistinctFnv { get; set; }
    public bool AllMatch { get; set; }
}

public sealed class ReportsResponse
{
    public ExpectedValues? Expected { get; set; }
    public List<DeviceReport> Reports { get; set; } = new();
    public ReportSummary? Summary { get; set; }
}
