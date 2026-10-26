using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using CookYourGame.Dsl;
using CookYourGame.Dsl.Core;
using CookYourGame.Dsl.Emit;

namespace CookYourGame.Dsl.Cli
{
    internal static class Program
    {
        private const int ExitSuccess = 0;
        private const int ExitFailure = 1;
        private const int ExitUsage = 2;

        private static int Main(string[] args)
        {
            if (!TryParseArguments(args, out CliOptions options, out string? error))
            {
                Console.Error.WriteLine("error: " + error);
                Console.Error.WriteLine();
                PrintUsage(Console.Error);
                return ExitUsage;
            }

            if (options.Help)
            {
                PrintUsage(Console.Out);
                return ExitSuccess;
            }

            string[] inputs;
            try
            {
                inputs = ResolveInputs(options.Inputs);
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException || ex is ArgumentException)
            {
                Console.Error.WriteLine("error: " + ex.Message);
                return ExitFailure;
            }

            if (inputs.Length == 0)
            {
                Console.Error.WriteLine("error: no .cyg files were found in the supplied input paths.");
                return ExitFailure;
            }

            var frontend = new CygFrontend();
            var emitter = new CygCSharpEmitter();
            bool failed = false;
            var writtenPaths = new HashSet<string>(PathComparer);

            foreach (string input in inputs)
            {
                if (!ProcessFile(input, options, frontend, emitter, writtenPaths))
                {
                    failed = true;
                }
            }

            return failed ? ExitFailure : ExitSuccess;
        }

        private static bool ProcessFile(
            string input,
            CliOptions options,
            CygFrontend frontend,
            CygCSharpEmitter emitter,
            HashSet<string> writtenPaths)
        {
            string source;
            try
            {
                source = File.ReadAllText(input);
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException)
            {
                Console.Error.WriteLine($"{input} error: {ex.Message}");
                return false;
            }

            CygModule module;
            try
            {
                module = frontend.Parse(source, input);
            }
            catch (NotImplementedException ex)
            {
                Console.Error.WriteLine($"{input} error: the .cyg frontend is not implemented yet ({ex.Message})");
                return false;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"{input} error: unhandled frontend failure: {ex.GetType().Name}: {ex.Message}");
                return false;
            }

            PrintDiagnostics(input, module.Diagnostics);
            if (module.HasErrors)
            {
                return false;
            }

            if (options.Check)
            {
                return true;
            }

            string outputDirectory = options.OutputDirectory
                ?? Path.Combine(Path.GetDirectoryName(input) ?? ".", "Generated");

            CygEmitResult result;
            try
            {
                result = emitter.Emit(module, new CygEmitOptions
                {
                    Namespace = options.Namespace,
                    OutputDirectory = outputDirectory,
                    IncludeHeader = true
                });
            }
            catch (NotImplementedException ex)
            {
                Console.Error.WriteLine($"{input} error: the C# emitter is not implemented yet ({ex.Message})");
                return false;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"{input} error: unhandled emitter failure: {ex.GetType().Name}: {ex.Message}");
                return false;
            }

            PrintDiagnostics(input, result.Diagnostics);
            if (!result.Success)
            {
                return false;
            }

            if (result.Files.Count == 0)
            {
                Console.Error.WriteLine($"{input} error: the emitter produced no files.");
                return false;
            }

            try
            {
                Directory.CreateDirectory(outputDirectory);
            }
            catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException)
            {
                Console.Error.WriteLine($"{input} error: cannot create output directory '{outputDirectory}': {ex.Message}");
                return false;
            }

            string outputRoot = Path.GetFullPath(outputDirectory);
            bool ok = true;
            var batchPaths = new HashSet<string>(PathComparer);
            foreach (CygEmittedFile file in result.Files)
            {
                string relative = file.RelativePath ?? string.Empty;
                if (string.IsNullOrWhiteSpace(relative) || Path.IsPathRooted(relative))
                {
                    Console.Error.WriteLine($"{input} error: emitter returned an invalid relative path '{relative}'.");
                    ok = false;
                    continue;
                }

                string target;
                try
                {
                    target = Path.GetFullPath(Path.Combine(outputRoot, relative));
                }
                catch (Exception ex) when (ex is ArgumentException || ex is NotSupportedException || ex is PathTooLongException)
                {
                    Console.Error.WriteLine($"{input} error: invalid output path '{relative}': {ex.Message}");
                    ok = false;
                    continue;
                }

                if (!IsUnderDirectory(target, outputRoot))
                {
                    Console.Error.WriteLine($"{input} error: output path '{relative}' escapes the output directory.");
                    ok = false;
                    continue;
                }

                if (!batchPaths.Add(target) || !writtenPaths.Add(target))
                {
                    Console.Error.WriteLine($"{input} error: more than one input would write '{target}'.");
                    ok = false;
                    continue;
                }

                try
                {
                    string? directory = Path.GetDirectoryName(target);
                    if (!string.IsNullOrEmpty(directory))
                    {
                        Directory.CreateDirectory(directory);
                    }

                    File.WriteAllText(target, file.Content ?? string.Empty, new UTF8Encoding(false));
                    Console.Out.WriteLine("wrote " + target);
                }
                catch (Exception ex) when (ex is IOException || ex is UnauthorizedAccessException)
                {
                    Console.Error.WriteLine($"{input} error: cannot write '{target}': {ex.Message}");
                    ok = false;
                }
            }

            return ok;
        }

        private static string[] ResolveInputs(IReadOnlyList<string> inputs)
        {
            var files = new List<string>();
            var seen = new HashSet<string>(PathComparer);

            foreach (string raw in inputs)
            {
                string path = Path.GetFullPath(raw);
                if (Directory.Exists(path))
                {
                    string[] discovered = Directory
                        .EnumerateFiles(path, "*", SearchOption.AllDirectories)
                        .Where(p => string.Equals(Path.GetExtension(p), ".cyg", StringComparison.OrdinalIgnoreCase))
                        .OrderBy(p => p, StringComparer.Ordinal)
                        .ToArray();

                    foreach (string file in discovered)
                    {
                        string full = Path.GetFullPath(file);
                        if (seen.Add(full))
                        {
                            files.Add(full);
                        }
                    }

                    continue;
                }

                if (!File.Exists(path))
                {
                    throw new IOException($"input not found: {raw}");
                }

                if (!string.Equals(Path.GetExtension(path), ".cyg", StringComparison.OrdinalIgnoreCase))
                {
                    throw new IOException($"input is not a .cyg file: {raw}");
                }

                if (seen.Add(path))
                {
                    files.Add(path);
                }
            }

            files.Sort(StringComparer.Ordinal);
            return files.ToArray();
        }

        private static void PrintDiagnostics(string input, IReadOnlyList<CygDiagnostic> diagnostics)
        {
            foreach (CygDiagnostic diagnostic in diagnostics)
            {
                string file = string.IsNullOrEmpty(diagnostic.Span.File) ? input : diagnostic.Span.File;
                int line = diagnostic.Span.Line > 0 ? diagnostic.Span.Line : 0;
                int column = diagnostic.Span.Column > 0 ? diagnostic.Span.Column : 0;
                string severity = diagnostic.Severity.ToString().ToLowerInvariant();
                string text = $"{file}:{line}:{column} {severity} {diagnostic.Id}: {diagnostic.Message}";
                if (diagnostic.Severity == CygSeverity.Error)
                {
                    Console.Error.WriteLine(text);
                }
                else
                {
                    Console.Out.WriteLine(text);
                }
            }
        }

        private static bool IsUnderDirectory(string path, string directory)
        {
            string root = Path.TrimEndingDirectorySeparator(directory);
            if (string.Equals(path, root, PathComparison))
            {
                return true;
            }

            string prefix = root + Path.DirectorySeparatorChar;
            return path.StartsWith(prefix, PathComparison);
        }

        private static bool TryParseArguments(string[] args, out CliOptions options, out string? error)
        {
            options = new CliOptions();
            error = null;

            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                switch (arg)
                {
                    case "--help":
                    case "-h":
                        options.Help = true;
                        return true;
                    case "--check":
                        options.Check = true;
                        break;
                    case "--out":
                        if (!TryReadValue(args, ref i, arg, out string? outValue, out error))
                        {
                            return false;
                        }
                        options.OutputDirectory = outValue;
                        break;
                    case "--namespace":
                        if (!TryReadValue(args, ref i, arg, out string? namespaceValue, out error))
                        {
                            return false;
                        }
                        options.Namespace = namespaceValue;
                        break;
                    default:
                        if (arg.StartsWith("--out=", StringComparison.Ordinal))
                        {
                            string value = arg.Substring("--out=".Length).Trim();
                            if (value.Length == 0)
                            {
                                error = "--out requires a non-empty directory.";
                                return false;
                            }

                            options.OutputDirectory = value;
                            break;
                        }

                        if (arg.StartsWith("--namespace=", StringComparison.Ordinal))
                        {
                            string value = arg.Substring("--namespace=".Length).Trim();
                            if (value.Length == 0)
                            {
                                error = "--namespace requires a non-empty namespace.";
                                return false;
                            }

                            options.Namespace = value;
                            break;
                        }

                        if (arg == "--")
                        {
                            for (int j = i + 1; j < args.Length; j++)
                            {
                                AddInput(options, args[j], ref error);
                                if (error != null)
                                {
                                    return false;
                                }
                            }

                            i = args.Length;
                            break;
                        }

                        if (arg.StartsWith("-", StringComparison.Ordinal))
                        {
                            error = "unknown option: " + arg;
                            return false;
                        }

                        AddInput(options, arg, ref error);
                        if (error != null)
                        {
                            return false;
                        }
                        break;
                }
            }

            if (options.Inputs.Count == 0)
            {
                error = "at least one .cyg file or directory is required.";
                return false;
            }

            if (options.OutputDirectory != null)
            {
                string trimmed = options.OutputDirectory.Trim();
                if (trimmed.Length == 0)
                {
                    error = "--out requires a non-empty directory.";
                    return false;
                }

                options.OutputDirectory = trimmed;
            }

            if (options.Namespace != null)
            {
                string trimmed = options.Namespace.Trim();
                if (trimmed.Length == 0)
                {
                    error = "--namespace requires a non-empty namespace.";
                    return false;
                }

                options.Namespace = trimmed;
            }

            return true;
        }

        private static bool TryReadValue(
            string[] args,
            ref int index,
            string option,
            out string? value,
            out string? error)
        {
            value = null;
            error = null;
            if (index + 1 >= args.Length)
            {
                error = option + " requires a value.";
                return false;
            }

            string candidate = args[index + 1];
            if (candidate.StartsWith("-", StringComparison.Ordinal))
            {
                error = option + " requires a non-empty value.";
                return false;
            }

            if (candidate.Trim().Length == 0)
            {
                error = option + " requires a non-empty value.";
                return false;
            }

            value = candidate;
            index++;
            return true;
        }

        private static void AddInput(CliOptions options, string value, ref string? error)
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                error = "input paths cannot be empty.";
                return;
            }

            options.Inputs.Add(value);
        }

        private static void PrintUsage(TextWriter writer)
        {
            writer.WriteLine("Usage: cyg <file.cyg|directory> [<file.cyg|directory>...] [--out <dir>] [--namespace <ns>] [--check]");
            writer.WriteLine();
            writer.WriteLine("Compile CookYourGame .cyg DSL files to C#.");
            writer.WriteLine();
            writer.WriteLine("Arguments:");
            writer.WriteLine("  <file.cyg|directory>  One or more .cyg files, or directories searched recursively.");
            writer.WriteLine("  --out <dir>           Write generated files below this directory.");
            writer.WriteLine("                        Default: a 'Generated' directory next to each input file.");
            writer.WriteLine("  --namespace <ns>      Override the namespace used for generated types.");
            writer.WriteLine("  --check               Validate only; do not write any files.");
            writer.WriteLine("  -h, --help            Show this help text.");
            writer.WriteLine();
            writer.WriteLine("Diagnostics are printed as: file:line:column severity id: message");
            writer.WriteLine("Exit codes: 0 success, 1 compilation or I/O failure, 2 invalid arguments.");
        }

        private static readonly StringComparer PathComparer =
            OperatingSystem.IsWindows() ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;

        private static readonly StringComparison PathComparison =
            OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;

        private sealed class CliOptions
        {
            public List<string> Inputs { get; } = new List<string>();
            public string? OutputDirectory { get; set; }
            public string? Namespace { get; set; }
            public bool Check { get; set; }
            public bool Help { get; set; }
        }
    }
}
