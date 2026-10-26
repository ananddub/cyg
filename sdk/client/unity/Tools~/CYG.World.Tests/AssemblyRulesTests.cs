using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;

namespace CookYourGame.Sim.Tests
{
    /// <summary>
    /// Enforces the deterministic assembly contract at source level. The pure
    /// Sim directories must not call wall-clock APIs, UnityEngine, System.Random
    /// or LINQ, and floating point is confined to the fixed-point and wire
    /// boundary files. UnityAdapter and Tests are separate assemblies/directories
    /// and are intentionally out of scope.
    /// </summary>
    internal static class AssemblyRulesTests
    {
        private static readonly string[] PureDirectories =
        {
            "Core",
            "Math",
            "World",
            "Physics3D",
            "Physics2D",
            "Navigation",
            "Netcode",
            "Replay",
            "Rollback",
            "TwoBox",
        };

        private static readonly string[] FloatAllowlist =
        {
            "Core/CYGChecksum.cs",
            "Core/CYGPlayerInput.cs",
            "Core/CYGWire.cs",
            "Math/FP.cs",
            "Math/Fixed64.cs",
        };

        private static readonly KeyValuePair<string, string>[] Forbidden =
        {
            new KeyValuePair<string, string>("System.Random", "System.Random"),
            new KeyValuePair<string, string>("new Random(", "new Random()"),
            new KeyValuePair<string, string>("Random.Range", "UnityEngine Random.Range"),
            new KeyValuePair<string, string>("Random.value", "UnityEngine Random.value"),
            new KeyValuePair<string, string>("UnityEngine", "UnityEngine"),
            new KeyValuePair<string, string>("Mathf.", "UnityEngine Mathf"),
            new KeyValuePair<string, string>("DateTime", "DateTime"),
            new KeyValuePair<string, string>("Stopwatch", "Stopwatch"),
            new KeyValuePair<string, string>("Guid.NewGuid", "Guid.NewGuid"),
            new KeyValuePair<string, string>("Environment.TickCount", "Environment.TickCount"),
            new KeyValuePair<string, string>("System.Linq", "System.Linq"),
        };

        private static readonly Regex FloatToken = new Regex(@"\b(?:float|double)\b", RegexOptions.Compiled);

        /// <summary>
        /// Fields/local variables whose iteration order is not defined.
        /// SortedDictionary is excluded because it always walks in key order.
        /// </summary>
        private static readonly Regex UnorderedCollection =
            new Regex(@"(?<!Sorted)(?:Dictionary|HashSet)\s*<[^;=]+>\s+(\w+)\s*(?:=|;)", RegexOptions.Compiled);

        public static void RunAll()
        {
            string root = FindSimRoot();
            List<string> violations = new List<string>();
            int files = 0;

            for (int d = 0; d < PureDirectories.Length; d++)
            {
                string directory = Path.Combine(root, PureDirectories[d]);
                if (!Directory.Exists(directory))
                {
                    violations.Add("missing pure Sim directory: " + PureDirectories[d]);
                    continue;
                }

                string[] paths = Directory.GetFiles(directory, "*.cs", SearchOption.AllDirectories);
                Array.Sort(paths, StringComparer.Ordinal);
                for (int i = 0; i < paths.Length; i++)
                {
                    files++;
                    ScanFile(root, paths[i], violations);
                }
            }

            if (files < 100)
            {
                violations.Add("only " + files + " files found under " + root + "; audit path is wrong");
            }
            if (violations.Count > 0)
            {
                int shown = Math.Min(violations.Count, 25);
                StringBuilder message = new StringBuilder("deterministic assembly rules failed (")
                    .Append(violations.Count)
                    .Append(" violation(s)):");
                for (int i = 0; i < shown; i++)
                {
                    message.Append("\n  ").Append(violations[i]);
                }
                if (violations.Count > shown)
                {
                    message.Append("\n  ... ").Append(violations.Count - shown).Append(" more");
                }
                throw new InvalidOperationException(message.ToString());
            }

            Console.WriteLine("assembly rules passed: files=" + files);
        }

        private static void ScanFile(string root, string path, List<string> violations)
        {
            string relative = Path.GetRelativePath(root, path).Replace('\\', '/');
            bool allowFloat = false;
            for (int i = 0; i < FloatAllowlist.Length; i++)
            {
                if (string.Equals(relative, FloatAllowlist[i], StringComparison.Ordinal))
                {
                    allowFloat = true;
                    break;
                }
            }

            string[] lines = StripComments(File.ReadAllText(path)).Split('\n');
            List<string> unordered = new List<string>();
            for (int i = 0; i < lines.Length; i++)
            {
                Match declaration = UnorderedCollection.Match(lines[i]);
                while (declaration.Success)
                {
                    unordered.Add(declaration.Groups[1].Value);
                    declaration = declaration.NextMatch();
                }
            }

            for (int i = 0; i < lines.Length; i++)
            {
                string code = lines[i];
                if (code.Trim().Length == 0)
                {
                    continue;
                }

                if (code.IndexOf("foreach", StringComparison.Ordinal) >= 0)
                {
                    for (int u = 0; u < unordered.Count; u++)
                    {
                        string name = unordered[u];
                        if (code.IndexOf("in " + name, StringComparison.Ordinal) >= 0
                            || code.IndexOf(name + ".Keys", StringComparison.Ordinal) >= 0
                            || code.IndexOf(name + ".Values", StringComparison.Ordinal) >= 0)
                        {
                            violations.Add(relative + ":" + (i + 1) + " iterates " + name + " with undefined order");
                        }
                    }
                }

                for (int rule = 0; rule < Forbidden.Length; rule++)
                {
                    if (code.IndexOf(Forbidden[rule].Key, StringComparison.Ordinal) >= 0)
                    {
                        violations.Add(relative + ":" + (i + 1) + " uses " + Forbidden[rule].Value);
                    }
                }

                if (!allowFloat && FloatToken.IsMatch(code))
                {
                    violations.Add(relative + ":" + (i + 1) + " uses float/double");
                }
            }
        }

        private static string FindSimRoot()
        {
            string[] probes = { Environment.CurrentDirectory, AppContext.BaseDirectory };
            for (int i = 0; i < probes.Length; i++)
            {
                DirectoryInfo directory = new DirectoryInfo(probes[i]);
                while (directory != null)
                {
                    string candidate = Path.Combine(directory.FullName, "sdk", "unity", "Runtime", "Sim");
                    if (Directory.Exists(candidate))
                    {
                        return candidate;
                    }
                    directory = directory.Parent;
                }
            }

            throw new InvalidOperationException(
                "could not locate sdk/unity/Runtime/Sim from " + Environment.CurrentDirectory);
        }

        /// <summary>
        /// Replaces line and block comments with spaces while preserving line
        /// breaks, so a rule match always maps to the original line number.
        /// Strings and chars are kept as-is because the rules only match code
        /// identifiers, never user-facing text.
        /// </summary>
        private static string StripComments(string text)
        {
            StringBuilder output = new StringBuilder(text.Length);
            bool line = false;
            bool block = false;
            bool quoted = false;
            bool character = false;

            for (int i = 0; i < text.Length; i++)
            {
                char current = text[i];
                char next = i + 1 < text.Length ? text[i + 1] : '\0';

                if (line)
                {
                    if (current == '\n')
                    {
                        line = false;
                        output.Append(current);
                    }
                    else
                    {
                        output.Append(' ');
                    }
                    continue;
                }

                if (block)
                {
                    if (current == '*' && next == '/')
                    {
                        block = false;
                        output.Append("  ");
                        i++;
                    }
                    else
                    {
                        output.Append(current == '\n' ? '\n' : ' ');
                    }
                    continue;
                }

                if (quoted || character)
                {
                    output.Append(current);
                    if (current == '\\' && next != '\0')
                    {
                        output.Append(next);
                        i++;
                    }
                    else if (quoted && current == '"')
                    {
                        quoted = false;
                    }
                    else if (character && current == '\'')
                    {
                        character = false;
                    }
                    continue;
                }

                if (current == '/' && next == '/')
                {
                    line = true;
                    output.Append("  ");
                    i++;
                    continue;
                }
                if (current == '/' && next == '*')
                {
                    block = true;
                    output.Append("  ");
                    i++;
                    continue;
                }
                if (current == '"')
                {
                    quoted = true;
                }
                else if (current == '\'')
                {
                    character = true;
                }
                output.Append(current);
            }

            return output.ToString();
        }
    }
}
