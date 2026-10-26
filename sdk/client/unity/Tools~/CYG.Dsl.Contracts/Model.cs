using System;
using System.Collections.Generic;
using System.Linq;

namespace CookYourGame.Dsl
{
    /// <summary>Underlying integer type allowed on a generated enum or flags block.</summary>
    public enum CygPrimitive
    {
        Byte,
        SByte,
        Int16,
        UInt16,
        Int32,
        UInt32,
        Int64,
        UInt64
    }

    /// <summary>The complete set of types the DSL can put on the wire.</summary>
    public enum CygTypeKind
    {
        FP,
        Byte,
        SByte,
        Int16,
        UInt16,
        Int32,
        UInt32,
        Int64,
        UInt64,
        Boolean,
        FPVector2,
        FPVector3,
        FPQuaternion,
        FPMatrix4x4,
        FPAABB,
        FPBounds,
        EntityRef,
        PlayerRef,
        AssetRef,
        FixedString,
        FixedArray,
        FixedList,
        FixedDictionary,
        Named
    }

    public enum CygSeverity
    {
        Info,
        Warning,
        Error
    }

    /// <summary>Source location carried by every AST node and diagnostic.</summary>
    public readonly struct CygSpan
    {
        public CygSpan(string file, int line, int column, int length)
        {
            File = file ?? string.Empty;
            Line = line;
            Column = column;
            Length = length;
        }

        public string File { get; }
        public int Line { get; }
        public int Column { get; }
        public int Length { get; }

        public static CygSpan None => new CygSpan(string.Empty, 0, 0, 0);

        public override string ToString()
        {
            if (string.IsNullOrEmpty(File))
            {
                return "<unknown>";
            }

            return Line > 0 ? $"{File}:{Line}:{Column}" : File;
        }
    }

    public sealed class CygDiagnostic
    {
        public CygDiagnostic(string id, CygSeverity severity, string message, CygSpan span)
        {
            Id = id;
            Severity = severity;
            Message = message;
            Span = span;
        }

        public string Id { get; }
        public CygSeverity Severity { get; }
        public string Message { get; }
        public CygSpan Span { get; }

        public override string ToString() => $"{Span}: {Severity.ToString().ToLowerInvariant()} {Id}: {Message}";
    }

    public sealed class CygDiagnosticBag
    {
        private readonly List<CygDiagnostic> _items = new List<CygDiagnostic>();

        public IReadOnlyList<CygDiagnostic> Items => _items;
        public bool HasErrors => _items.Any(d => d.Severity == CygSeverity.Error);
        public int Count => _items.Count;

        public void Add(CygDiagnostic diagnostic) => _items.Add(diagnostic);

        public void Error(string id, string message, CygSpan span)
            => _items.Add(new CygDiagnostic(id, CygSeverity.Error, message, span));

        public void Warning(string id, string message, CygSpan span)
            => _items.Add(new CygDiagnostic(id, CygSeverity.Warning, message, span));

        public void Info(string id, string message, CygSpan span)
            => _items.Add(new CygDiagnostic(id, CygSeverity.Info, message, span));
    }

    /// <summary>One field of a struct, component, global, event or union variant.</summary>
    public sealed class CygField
    {
        public string Name { get; set; } = string.Empty;
        public CygTypeRef Type { get; set; } = CygTypeRef.Builtin(CygTypeKind.Int32);
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygTypeRef
    {
        public CygTypeKind Kind { get; set; }
        public string? Name { get; set; }
        public CygTypeRef? Element { get; set; }
        public CygTypeRef? Key { get; set; }
        public CygTypeRef? Value { get; set; }
        public int Length { get; set; }

        public static CygTypeRef Builtin(CygTypeKind kind) => new CygTypeRef { Kind = kind };

        public static CygTypeRef NamedType(string name)
        {
            if (string.IsNullOrWhiteSpace(name))
            {
                throw new ArgumentException("A named type needs a name.", nameof(name));
            }

            return new CygTypeRef { Kind = CygTypeKind.Named, Name = name };
        }

        public static CygTypeRef ArrayOf(CygTypeRef element, int length)
            => new CygTypeRef { Kind = CygTypeKind.FixedArray, Element = element, Length = length };

        public static CygTypeRef ListOf(CygTypeRef element, int capacity)
            => new CygTypeRef { Kind = CygTypeKind.FixedList, Element = element, Length = capacity };

        public static CygTypeRef DictionaryOf(CygTypeRef key, CygTypeRef value, int capacity)
            => new CygTypeRef { Kind = CygTypeKind.FixedDictionary, Key = key, Value = value, Length = capacity };

        public static CygTypeRef AssetOf(string assetName)
            => new CygTypeRef { Kind = CygTypeKind.AssetRef, Name = assetName };

        public static CygTypeRef StringOf(int length)
            => new CygTypeRef { Kind = CygTypeKind.FixedString, Length = length };

        public string Display()
        {
            switch (Kind)
            {
                case CygTypeKind.FP: return "FP";
                case CygTypeKind.Byte: return "Byte";
                case CygTypeKind.SByte: return "SByte";
                case CygTypeKind.Int16: return "Int16";
                case CygTypeKind.UInt16: return "UInt16";
                case CygTypeKind.Int32: return "Int32";
                case CygTypeKind.UInt32: return "UInt32";
                case CygTypeKind.Int64: return "Int64";
                case CygTypeKind.UInt64: return "UInt64";
                case CygTypeKind.Boolean: return "Boolean";
                case CygTypeKind.FPVector2: return "FPVector2";
                case CygTypeKind.FPVector3: return "FPVector3";
                case CygTypeKind.FPQuaternion: return "FPQuaternion";
                case CygTypeKind.FPMatrix4x4: return "FPMatrix4x4";
                case CygTypeKind.FPAABB: return "FPAABB";
                case CygTypeKind.FPBounds: return "FPBounds";
                case CygTypeKind.EntityRef: return "entity_ref";
                case CygTypeKind.PlayerRef: return "player_ref";
                case CygTypeKind.AssetRef: return $"asset_ref<{Name}>";
                case CygTypeKind.FixedString: return $"QString<{Length}>";
                case CygTypeKind.FixedArray: return $"array<{Element?.Display()}>[{Length}]";
                case CygTypeKind.FixedList: return $"list<{Element?.Display()}>[{Length}]";
                case CygTypeKind.FixedDictionary: return $"dictionary<{Key?.Display()}, {Value?.Display()}>[{Length}]";
                case CygTypeKind.Named: return Name ?? "<missing>";
                default: return Kind.ToString();
            }
        }

        public override string ToString() => Display();
    }

    public sealed class CygEnumMember
    {
        public string Name { get; set; } = string.Empty;
        public long? Value { get; set; }
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public abstract class CygTypeDecl
    {
        public string Name { get; set; } = string.Empty;
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygStructDecl : CygTypeDecl
    {
        public List<CygField> Fields { get; } = new List<CygField>();
    }

    public sealed class CygComponentDecl : CygTypeDecl
    {
        public int? Id { get; set; }
        public List<CygField> Fields { get; } = new List<CygField>();
    }

    /// <summary>
    /// A state-sync schema. Unlike components/structs, schema fields use only
    /// the eight numeric/bool wire types understood by `SchemaDef`.
    /// </summary>
    public sealed class CygSchemaDecl
    {
        public string Name { get; set; } = string.Empty;
        public uint Version { get; set; } = 1;
        public List<CygField> Fields { get; } = new List<CygField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygEnumDecl : CygTypeDecl
    {
        public CygPrimitive Underlying { get; set; } = CygPrimitive.Int32;
        public bool UnderlyingExplicit { get; set; }
        public bool IsFlags { get; set; }
        public List<CygEnumMember> Members { get; } = new List<CygEnumMember>();
    }

    public sealed class CygUnionVariant
    {
        public string Name { get; set; } = string.Empty;
        public List<CygField> Fields { get; } = new List<CygField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygUnionDecl : CygTypeDecl
    {
        public List<CygUnionVariant> Variants { get; } = new List<CygUnionVariant>();
    }

    public sealed class CygInputField
    {
        public string Name { get; set; } = string.Empty;
        public CygTypeRef? Type { get; set; }
        public bool IsButton { get; set; }
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygInputDecl
    {
        public string Name { get; set; } = "CYGInput";
        public List<CygInputField> Fields { get; } = new List<CygInputField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygGlobalDecl
    {
        public string Name { get; set; } = "CYGGlobals";
        public List<CygField> Fields { get; } = new List<CygField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygEventDecl
    {
        public string Name { get; set; } = string.Empty;
        public int? Id { get; set; }
        public bool Synced { get; set; }
        public List<CygField> Fields { get; } = new List<CygField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygCommandDecl
    {
        public string Name { get; set; } = string.Empty;
        public int? Id { get; set; }
        public List<CygField> Fields { get; } = new List<CygField>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygSignalParam
    {
        public string Name { get; set; } = string.Empty;
        public CygTypeRef Type { get; set; } = CygTypeRef.Builtin(CygTypeKind.Int32);
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygSignalDecl
    {
        public string Name { get; set; } = string.Empty;
        public List<CygSignalParam> Parameters { get; } = new List<CygSignalParam>();
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    public sealed class CygAssetDecl
    {
        public string Name { get; set; } = string.Empty;
        public CygSpan Span { get; set; } = CygSpan.None;
    }

    /// <summary>The parsed and validated contents of one .cyg file.</summary>
    public sealed class CygModule
    {
        public string Namespace { get; set; } = "CookYourGame.Sim.Generated";
        public string SourcePath { get; set; } = string.Empty;
        public List<CygTypeDecl> Types { get; } = new List<CygTypeDecl>();
        public List<CygAssetDecl> Assets { get; } = new List<CygAssetDecl>();
        public CygInputDecl? Input { get; set; }
        public CygGlobalDecl? Global { get; set; }
        public List<CygEventDecl> Events { get; } = new List<CygEventDecl>();
        public List<CygCommandDecl> Commands { get; } = new List<CygCommandDecl>();
        public List<CygSignalDecl> Signals { get; } = new List<CygSignalDecl>();
        public List<CygSchemaDecl> Schemas { get; } = new List<CygSchemaDecl>();
        public List<CygDiagnostic> Diagnostics { get; } = new List<CygDiagnostic>();

        public bool HasErrors => Diagnostics.Any(d => d.Severity == CygSeverity.Error);

        public IEnumerable<CygComponentDecl> Components => Types.OfType<CygComponentDecl>();
        public IEnumerable<CygStructDecl> Structs => Types.OfType<CygStructDecl>();
        public IEnumerable<CygEnumDecl> Enums => Types.OfType<CygEnumDecl>();
        public IEnumerable<CygUnionDecl> Unions => Types.OfType<CygUnionDecl>();

        public CygTypeDecl? FindType(string name)
            => Types.FirstOrDefault(t => string.Equals(t.Name, name, StringComparison.Ordinal));

        public CygAssetDecl? FindAsset(string name)
            => Assets.FirstOrDefault(a => string.Equals(a.Name, name, StringComparison.Ordinal));
    }

    public sealed class CygEmitOptions
    {
        public string? Namespace { get; set; }
        public bool IncludeHeader { get; set; } = true;
        public string? OutputDirectory { get; set; }
    }

    public sealed class CygEmittedFile
    {
        public CygEmittedFile(string relativePath, string content)
        {
            RelativePath = relativePath;
            Content = content;
        }

        public string RelativePath { get; }
        public string Content { get; }
    }

    public sealed class CygEmitResult
    {
        public List<CygEmittedFile> Files { get; } = new List<CygEmittedFile>();
        public List<CygDiagnostic> Diagnostics { get; } = new List<CygDiagnostic>();
        public bool Success => Diagnostics.All(d => d.Severity != CygSeverity.Error);
    }

    public interface ICygFrontend
    {
        CygModule Parse(string source, string filePath);
    }

    public interface ICygEmitter
    {
        CygEmitResult Emit(CygModule module, CygEmitOptions options);
    }

    /// <summary>Reserved component/event/command id ranges. Support values use no id at all.</summary>
    public static class CygIds
    {
        public const int Globals = 1;
        public const int Input = 2;
        public const int FirstEngine = 3;
        public const int LastEngine = 999;
        public const int FirstComponent = 1000;
        public const int LastComponent = 999_999;
        public const int FirstEvent = 1_000_000;
        public const int LastEvent = 1_999_999;
        public const int FirstCommand = 2_000_000;
        public const int LastCommand = 2_999_999;

        public static bool IsUserComponent(int id) => id >= FirstComponent && id <= LastComponent;
        public static bool IsEvent(int id) => id >= FirstEvent && id <= LastEvent;
        public static bool IsCommand(int id) => id >= FirstCommand && id <= LastCommand;
    }

    /// <summary>Names the generators reserve so emitted members can never collide.</summary>
    public static class CygReserved
    {
        public static readonly string[] FieldNames =
        {
            "SizeInBytes", "TypeId", "SchemaHash", "WireSize",
            "ComponentTypeId", "ComponentSchemaHash",
            "WriteTo", "ReadFrom", "ComputeChecksum", "CopyTo"
        };

        public static bool IsFieldName(string name)
            => FieldNames.Contains(name, StringComparer.Ordinal);
    }

    /// <summary>Diagnostic ids. CYGD1xx is DSL-specific; the C# generator keeps CYG0xx.</summary>
    public static class CygDiagnosticIds
    {
        public const string Float = "CYGD001";
        public const string UnityType = "CYGD002";
        public const string ReferenceType = "CYGD003";
        public const string StandardCollection = "CYGD004";
        public const string Lexer = "CYGD100";
        public const string Parser = "CYGD101";
        public const string DuplicateType = "CYGD102";
        public const string UnknownType = "CYGD103";
        public const string RecursiveType = "CYGD104";
        public const string ComponentId = "CYGD105";
        public const string ArrayLength = "CYGD106";
        public const string StringLength = "CYGD107";
        public const string Flags = "CYGD108";
        public const string Union = "CYGD109";
        public const string Input = "CYGD110";
        public const string DuplicateBlock = "CYGD111";
        public const string ReservedName = "CYGD112";
        public const string EventId = "CYGD113";
        public const string UnsupportedType = "CYGD114";
        public const string ReservedId = "CYGD115";
        public const string MissingExplicitId = "CYGD116";
        public const string UnknownAsset = "CYGD117";
        public const string DuplicateField = "CYGD118";
        public const string CommandId = "CYGD119";
        public const string Schema = "CYGD120";
        public const string Emit = "CYGD200";
    }
}
