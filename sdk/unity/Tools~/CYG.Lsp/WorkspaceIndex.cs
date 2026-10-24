namespace CookYourGame.Lsp;

/// <summary>Lazy, mtime-cached index over .cyg files on disk for workspace symbols.</summary>
internal sealed class WorkspaceIndex
{
    private const int MaxFiles = 4000;
    private const int MaxResults = 500;

    private static readonly HashSet<string> ExcludedDirectories = new(StringComparer.OrdinalIgnoreCase)
    {
        ".git", ".vs", ".idea", ".artifacts", "node_modules", "Library", "Temp", "obj", "bin",
        "Build", "Builds", "Logs",
    };

    private readonly List<string> _folders = new();
    private readonly Dictionary<string, CacheEntry> _cache = new(StringComparer.Ordinal);

    public void SetFolders(IEnumerable<string> folders)
    {
        _folders.Clear();
        _cache.Clear();
        foreach (string folder in folders)
        {
            if (string.IsNullOrWhiteSpace(folder))
            {
                continue;
            }

            string full = Path.GetFullPath(folder);
            if (!_folders.Contains(full, StringComparer.Ordinal))
            {
                _folders.Add(full);
            }
        }
    }

    public List<(string Path, CygSymbol Symbol)> Search(string query)
    {
        var results = new List<(string Path, CygSymbol Symbol)>();
        int files = 0;
        foreach (string folder in _folders)
        {
            if (results.Count >= MaxResults || files >= MaxFiles)
            {
                break;
            }

            Walk(folder, query ?? string.Empty, results, ref files);
        }

        return results;
    }

    public void Invalidate(string uri)
    {
        string path;
        try
        {
            path = Path.GetFullPath(Uris.ToPath(uri));
        }
        catch
        {
            return;
        }

        _cache.Remove(path);
    }

    private void Walk(
        string folder,
        string query,
        List<(string Path, CygSymbol Symbol)> results,
        ref int files)
    {
        if (!Directory.Exists(folder))
        {
            return;
        }

        var pending = new Stack<string>();
        pending.Push(folder);
        while (pending.Count > 0 && files < MaxFiles && results.Count < MaxResults)
        {
            string directory = pending.Pop();
            try
            {
                foreach (string child in Directory.EnumerateDirectories(directory))
                {
                    string name = Path.GetFileName(child);
                    if (!ExcludedDirectories.Contains(name))
                    {
                        pending.Push(child);
                    }
                }

                foreach (string file in Directory.EnumerateFiles(directory, "*.cyg", SearchOption.TopDirectoryOnly))
                {
                    files++;
                    CygDocumentIndex? index = GetIndex(file);
                    if (index == null)
                    {
                        continue;
                    }

                    foreach (CygSymbol declaration in index.Declarations)
                    {
                        if (declaration.Kind == CygSymbolKind.Namespace)
                        {
                            continue;
                        }

                        if (query.Length == 0
                            || declaration.Name.Contains(query, StringComparison.OrdinalIgnoreCase))
                        {
                            results.Add((file, declaration));
                            if (results.Count >= MaxResults)
                            {
                                return;
                            }
                        }
                    }

                    if (files >= MaxFiles)
                    {
                        return;
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                // A locked or vanished directory must not take the server down.
            }
        }
    }

    private CygDocumentIndex? GetIndex(string path)
    {
        try
        {
            string full = Path.GetFullPath(path);
            var info = new FileInfo(full);
            if (!info.Exists)
            {
                _cache.Remove(full);
                return null;
            }

            if (_cache.TryGetValue(full, out CacheEntry entry)
                && entry.LastWriteUtc == info.LastWriteTimeUtc
                && entry.Length == info.Length)
            {
                return entry.Index;
            }

            CygDocumentIndex index = CygDocumentIndex.Build(File.ReadAllText(full), full);
            _cache[full] = new CacheEntry(info.LastWriteTimeUtc, info.Length, index);
            return index;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException)
        {
            return null;
        }
    }

    private readonly record struct CacheEntry(DateTime LastWriteUtc, long Length, CygDocumentIndex Index);
}
