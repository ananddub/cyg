using System;
using System.Collections.Generic;

namespace CookYourGame.Dsl
{
    /// <summary>
    /// The single source of truth for wire sizes and generated C# names. The
    /// validator, the emitter and the tests all call this so the layout can
    /// never drift between them.
    /// </summary>
    public static class CygLayout
    {
        public static int PrimitiveSize(CygPrimitive primitive)
        {
            switch (primitive)
            {
                case CygPrimitive.Byte:
                case CygPrimitive.SByte:
                    return 1;
                case CygPrimitive.Int16:
                case CygPrimitive.UInt16:
                    return 2;
                case CygPrimitive.Int32:
                case CygPrimitive.UInt32:
                    return 4;
                case CygPrimitive.Int64:
                case CygPrimitive.UInt64:
                    return 8;
                default:
                    throw new ArgumentOutOfRangeException(nameof(primitive), primitive, null);
            }
        }

        public static int? SizeOf(CygModule module, CygTypeRef type)
        {
            return SizeOf(module, type, new HashSet<string>(StringComparer.Ordinal));
        }

        public static int? SizeOfFields(CygModule module, IEnumerable<CygField> fields)
        {
            var visiting = new HashSet<string>(StringComparer.Ordinal);
            long total = 0;
            foreach (CygField field in fields)
            {
                int? size = SizeOf(module, field.Type, visiting);
                if (size == null)
                {
                    return null;
                }

                total += size.Value;
            }

            return total > int.MaxValue ? (int?)null : (int)total;
        }

        private static int? SizeOf(CygModule module, CygTypeRef type, HashSet<string> visiting)
        {
            switch (type.Kind)
            {
                case CygTypeKind.FP: return 8;
                case CygTypeKind.Byte:
                case CygTypeKind.SByte:
                case CygTypeKind.Boolean: return 1;
                case CygTypeKind.Int16:
                case CygTypeKind.UInt16:
                case CygTypeKind.PlayerRef: return 2;
                case CygTypeKind.Int32:
                case CygTypeKind.UInt32:
                case CygTypeKind.AssetRef: return 4;
                case CygTypeKind.Int64:
                case CygTypeKind.UInt64:
                case CygTypeKind.EntityRef: return 8;
                case CygTypeKind.FPVector2: return 16;
                case CygTypeKind.FPVector3: return 24;
                case CygTypeKind.FPQuaternion: return 32;
                case CygTypeKind.FPMatrix4x4: return 128;
                case CygTypeKind.FPAABB:
                case CygTypeKind.FPBounds: return 48;
                case CygTypeKind.FixedString:
                    return type.Length > 0 ? (int?)checked(type.Length * 2) : null;
                case CygTypeKind.FixedArray:
                    return Sizes(type.Element, type.Length, 0, module, visiting);
                case CygTypeKind.FixedList:
                    return Sizes(type.Element, type.Length, 4, module, visiting);
                case CygTypeKind.FixedDictionary:
                    {
                        int? key = type.Key == null ? null : SizeOf(module, type.Key, visiting);
                        int? value = type.Value == null ? null : SizeOf(module, type.Value, visiting);
                        if (key == null || value == null || type.Length <= 0)
                        {
                            return null;
                        }

                        long one = 1L + key.Value + value.Value;
                        long total = 4L + one * type.Length;
                        return total > int.MaxValue ? (int?)null : (int)total;
                    }
                case CygTypeKind.Named:
                    return NamedSize(module, type.Name, visiting);
                default:
                    return null;
            }
        }

        private static int? Sizes(CygTypeRef? element, int length, int prefix, CygModule module, HashSet<string> visiting)
        {
            if (element == null || length <= 0)
            {
                return null;
            }

            int? size = SizeOf(module, element, visiting);
            if (size == null)
            {
                return null;
            }

            long total = (long)prefix + (long)size.Value * length;
            return total > int.MaxValue ? (int?)null : (int)total;
        }

        private static int? NamedSize(CygModule module, string? name, HashSet<string> visiting)
        {
            if (string.IsNullOrEmpty(name) || !visiting.Add(name))
            {
                return null;
            }

            try
            {
                CygTypeDecl? decl = module.FindType(name);
                switch (decl)
                {
                    case null:
                        return null;
                    case CygEnumDecl enumDecl:
                        return PrimitiveSize(enumDecl.Underlying);
                    case CygStructDecl structDecl:
                        return SumFields(module, structDecl.Fields, visiting);
                    case CygComponentDecl component:
                        return SumFields(module, component.Fields, visiting);
                    case CygUnionDecl union:
                        {
                            int max = 0;
                            foreach (CygUnionVariant variant in union.Variants)
                            {
                                int? size = SumFields(module, variant.Fields, visiting);
                                if (size == null)
                                {
                                    return null;
                                }

                                max = Math.Max(max, size.Value);
                            }

                            return 1 + max;
                        }
                    default:
                        return null;
                }
            }
            finally
            {
                visiting.Remove(name);
            }
        }

        private static int? SumFields(CygModule module, List<CygField> fields, HashSet<string> visiting)
        {
            long total = 0;
            foreach (CygField field in fields)
            {
                int? size = SizeOf(module, field.Type, visiting);
                if (size == null)
                {
                    return null;
                }

                total += size.Value;
            }

            return total > int.MaxValue ? (int?)null : (int)total;
        }

        public static int PairSize(int first, int second)
        {
            long total = (long)first + second;
            return total > int.MaxValue ? int.MaxValue : (int)total;
        }
    }

    /// <summary>Canonical generated-type naming shared by the emitter and tests.</summary>
    public static class CygNames
    {
        public static string CsTypeName(CygTypeRef type)
        {
            switch (type.Kind)
            {
                case CygTypeKind.FP: return "FP";
                case CygTypeKind.Byte: return "byte";
                case CygTypeKind.SByte: return "sbyte";
                case CygTypeKind.Int16: return "short";
                case CygTypeKind.UInt16: return "ushort";
                case CygTypeKind.Int32: return "int";
                case CygTypeKind.UInt32: return "uint";
                case CygTypeKind.Int64: return "long";
                case CygTypeKind.UInt64: return "ulong";
                case CygTypeKind.Boolean: return "bool";
                case CygTypeKind.FPVector2: return "FPVector2";
                case CygTypeKind.FPVector3: return "FPVector3";
                case CygTypeKind.FPQuaternion: return "FPQuaternion";
                case CygTypeKind.FPMatrix4x4: return "FPMatrix4x4";
                case CygTypeKind.FPAABB: return "FPAABB";
                case CygTypeKind.FPBounds: return "FPBounds";
                case CygTypeKind.EntityRef: return "CYGEntityRef";
                case CygTypeKind.PlayerRef: return "CYGPlayerRef";
                case CygTypeKind.AssetRef: return TypeStem(type) + "Ref";
                case CygTypeKind.FixedString: return "CYGFixedString" + type.Length;
                case CygTypeKind.FixedArray:
                case CygTypeKind.FixedList:
                case CygTypeKind.FixedDictionary:
                    return TypeStem(type);
                case CygTypeKind.Named: return SanitizeIdentifier(type.Name ?? "Missing");
                default: throw new ArgumentOutOfRangeException(nameof(type), type.Kind, null);
            }
        }

        /// <summary>
        /// The stem used for generated support types (arrays, lists, dictionaries,
        /// asset refs). It intentionally ignores PascalCase/underscore style so
        /// two spellings of the same type cannot create two different structs.
        /// </summary>
        public static string TypeStem(CygTypeRef type)
        {
            switch (type.Kind)
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
                case CygTypeKind.EntityRef: return "EntityRef";
                case CygTypeKind.PlayerRef: return "PlayerRef";
                case CygTypeKind.AssetRef: return SanitizeIdentifier(type.Name ?? "Missing");
                case CygTypeKind.FixedString: return "String" + type.Length;
                case CygTypeKind.FixedArray: return TypeStem(type.Element!) + "FixedArray" + type.Length;
                case CygTypeKind.FixedList: return TypeStem(type.Element!) + "FixedList" + type.Length;
                case CygTypeKind.FixedDictionary:
                    return TypeStem(type.Key!) + TypeStem(type.Value!) + "FixedDictionary" + type.Length;
                case CygTypeKind.Named: return SanitizeIdentifier(type.Name ?? "Missing");
                default: throw new ArgumentOutOfRangeException(nameof(type), type.Kind, null);
            }
        }

        private static readonly HashSet<string> CsKeywords = new HashSet<string>(StringComparer.Ordinal)
        {
            "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked",
            "class", "const", "continue", "decimal", "default", "delegate", "do", "double", "else",
            "enum", "event", "explicit", "extern", "false", "finally", "fixed", "float", "for",
            "foreach", "goto", "if", "implicit", "in", "int", "interface", "internal", "is", "lock",
            "long", "namespace", "new", "null", "object", "operator", "out", "override", "params",
            "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed", "short",
            "sizeof", "stackalloc", "static", "string", "struct", "switch", "this", "throw", "true",
            "try", "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual",
            "void", "volatile", "while"
        };

        /// <summary>Escapes a C# identifier so DSL names may collide with language keywords.</summary>
        public static string CsIdentifier(string name)
        {
            string sanitized = SanitizeIdentifier(name);
            return CsKeywords.Contains(sanitized) ? "@" + sanitized : sanitized;
        }

        public static string SanitizeIdentifier(string name)
        {
            if (string.IsNullOrEmpty(name))
            {
                return "Missing";
            }

            char[] chars = name.ToCharArray();
            for (int i = 0; i < chars.Length; i++)
            {
                char c = chars[i];
                bool valid = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
                if (!valid)
                {
                    chars[i] = '_';
                }
            }

            if (char.IsDigit(chars[0]))
            {
                return "_" + new string(chars);
            }

            return new string(chars);
        }

        public static bool IsReservedTypeName(string name)
        {
            if (string.IsNullOrEmpty(name))
            {
                return true;
            }

            if (name.StartsWith("CYG", StringComparison.Ordinal))
            {
                return true;
            }

            return ContainsGeneratedSuffix(name, "FixedArray")
                || ContainsGeneratedSuffix(name, "FixedList")
                || ContainsGeneratedSuffix(name, "FixedDictionary")
                || ContainsGeneratedSuffix(name, "FixedString");
        }

        private static bool ContainsGeneratedSuffix(string name, string suffix)
        {
            int index = name.IndexOf(suffix, StringComparison.Ordinal);
            if (index < 0)
            {
                return false;
            }

            int cursor = index + suffix.Length;
            if (cursor == name.Length)
            {
                return true;
            }

            for (; cursor < name.Length; cursor++)
            {
                if (name[cursor] < '0' || name[cursor] > '9')
                {
                    return false;
                }
            }

            return true;
        }
    }
}
