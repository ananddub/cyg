using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace CookYourGame.CodeGen
{
    public sealed partial class CYGComponentGenerator
    {
        private static FieldSpec BuildField(
            INamedTypeSymbol owner,
            IFieldSymbol field,
            GeneratorExecutionContext context,
            GenerationContext state)
        {
            ITypeSymbol type = field.Type;
            Location location = field.Locations.FirstOrDefault() ?? Location.None;
            string typeName = type.ToDisplayString();

            if (type == null || type.TypeKind == TypeKind.Array || type.TypeKind == TypeKind.Pointer
                || type.TypeKind == TypeKind.Dynamic || type.TypeKind == TypeKind.TypeParameter)
            {
                Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                return null;
            }
            if (type.IsReferenceType)
            {
                Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                return null;
            }

            INamedTypeSymbol named = type as INamedTypeSymbol;
            if (named != null && named.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T)
            {
                Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                return null;
            }

            if (type.TypeKind == TypeKind.Enum)
            {
                ITypeSymbol underlying = named.EnumUnderlyingType;
                FieldSpec enumSpec = new FieldSpec(field, FieldKind.Enum, SizeOfSpecial(underlying.SpecialType));
                enumSpec.Underlying = underlying;
                if (enumSpec.Size <= 0)
                {
                    Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                    return null;
                }
                return enumSpec;
            }

            if (named != null)
            {
                string ns = named.ContainingNamespace == null ? string.Empty : named.ContainingNamespace.ToDisplayString();
                if (ns == "UnityEngine" || ns.StartsWith("UnityEngine.", StringComparison.Ordinal))
                {
                    Report(context, CYGDiagnostics.UnityTypeUnsupported, location, field.Name, typeName);
                    return null;
                }
            }

            switch (type.SpecialType)
            {
                case SpecialType.System_Boolean: return new FieldSpec(field, FieldKind.Bool, 1);
                case SpecialType.System_Byte: return new FieldSpec(field, FieldKind.Byte, 1);
                case SpecialType.System_SByte: return new FieldSpec(field, FieldKind.SByte, 1);
                case SpecialType.System_Int16: return new FieldSpec(field, FieldKind.Int16, 2);
                case SpecialType.System_UInt16: return new FieldSpec(field, FieldKind.UInt16, 2);
                case SpecialType.System_Int32: return new FieldSpec(field, FieldKind.Int32, 4);
                case SpecialType.System_UInt32: return new FieldSpec(field, FieldKind.UInt32, 4);
                case SpecialType.System_Int64: return new FieldSpec(field, FieldKind.Int64, 8);
                case SpecialType.System_UInt64: return new FieldSpec(field, FieldKind.UInt64, 8);
                case SpecialType.System_Char: return new FieldSpec(field, FieldKind.Char, 2);
                case SpecialType.System_Single:
                    Report(context, CYGDiagnostics.FloatField, location, field.Name, "float");
                    return new FieldSpec(field, FieldKind.Single, 4);
                case SpecialType.System_Double:
                    Report(context, CYGDiagnostics.FloatField, location, field.Name, "double");
                    return new FieldSpec(field, FieldKind.Double, 8);
            }

            if (named == null || named.TypeKind != TypeKind.Struct || named.IsGenericType)
            {
                Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                return null;
            }

            if (state.IsCandidate(named))
            {
                int candidateSize;
                if (!state.TryGetSize(named, out candidateSize) || candidateSize <= 0)
                {
                    Report(context, CYGDiagnostics.UnsupportedField, location, field.Name, typeName);
                    return null;
                }
                return new FieldSpec(field, FieldKind.Struct, candidateSize);
            }

            int size = ReadSize(named);
            if (size <= 0 || !HasCodec(named))
            {
                Report(context, CYGDiagnostics.MissingCodec, location, typeName, field.Name);
                return null;
            }

            return new FieldSpec(field, FieldKind.Struct, size);
        }

        private static bool HasCodec(INamedTypeSymbol type)
        {
            bool write = false;
            bool read = false;
            bool checksum = false;

            foreach (IMethodSymbol method in type.GetMembers("WriteTo").OfType<IMethodSymbol>())
            {
                if (!method.IsStatic
                    && method.Arity == 0
                    && IsAccessible(method)
                    && method.Parameters.Length == 2
                    && IsByteArray(method.Parameters[0].Type)
                    && method.Parameters[1].RefKind == RefKind.None
                    && method.Parameters[1].Type.SpecialType == SpecialType.System_Int32
                    && method.ReturnType.SpecialType == SpecialType.System_Int32)
                {
                    write = true;
                }
            }
            foreach (IMethodSymbol method in type.GetMembers("ReadFrom").OfType<IMethodSymbol>())
            {
                if (!method.IsStatic
                    && method.Arity == 0
                    && IsAccessible(method)
                    && method.Parameters.Length == 2
                    && IsByteArray(method.Parameters[0].Type)
                    && method.Parameters[1].RefKind == RefKind.None
                    && method.Parameters[1].Type.SpecialType == SpecialType.System_Int32
                    && method.ReturnType.SpecialType == SpecialType.System_Void)
                {
                    read = true;
                }
            }
            foreach (IMethodSymbol method in type.GetMembers("ComputeChecksum").OfType<IMethodSymbol>())
            {
                if (!method.IsStatic
                    && method.Arity == 0
                    && IsAccessible(method)
                    && method.Parameters.Length == 1
                    && method.Parameters[0].RefKind == RefKind.Ref
                    && method.Parameters[0].Type.SpecialType == SpecialType.System_UInt32
                    && method.ReturnType.SpecialType == SpecialType.System_Void)
                {
                    checksum = true;
                }
            }
            return write && read && checksum;
        }

        private static bool IsAccessible(ISymbol symbol)
        {
            return symbol.DeclaredAccessibility == Accessibility.Public
                || symbol.DeclaredAccessibility == Accessibility.Internal
                || symbol.DeclaredAccessibility == Accessibility.ProtectedOrInternal;
        }

        private static bool IsByteArray(ITypeSymbol type)
        {
            IArrayTypeSymbol array = type as IArrayTypeSymbol;
            return array != null && array.ElementType.SpecialType == SpecialType.System_Byte && array.Rank == 1;
        }

        private static int ReadSize(INamedTypeSymbol type)
        {
            foreach (ISymbol member in type.GetMembers("SizeInBytes"))
            {
                IFieldSymbol field = member as IFieldSymbol;
                if (field != null && field.IsConst && IsAccessible(field) && field.Type.SpecialType == SpecialType.System_Int32
                    && field.ConstantValue is int)
                {
                    return (int)field.ConstantValue;
                }
            }
            return -1;
        }

        private static int SizeOfSpecial(SpecialType type)
        {
            switch (type)
            {
                case SpecialType.System_Byte:
                case SpecialType.System_SByte:
                case SpecialType.System_Boolean:
                    return 1;
                case SpecialType.System_Int16:
                case SpecialType.System_UInt16:
                case SpecialType.System_Char:
                    return 2;
                case SpecialType.System_Int32:
                case SpecialType.System_UInt32:
                case SpecialType.System_Single:
                    return 4;
                case SpecialType.System_Int64:
                case SpecialType.System_UInt64:
                case SpecialType.System_Double:
                    return 8;
                default:
                    return -1;
            }
        }

        private enum FieldKind
        {
            Bool,
            Byte,
            SByte,
            Int16,
            UInt16,
            Int32,
            UInt32,
            Int64,
            UInt64,
            Char,
            Single,
            Double,
            Enum,
            Struct
        }

        private sealed class FieldSpec
        {
            public FieldSpec(IFieldSymbol symbol, FieldKind kind, int size)
            {
                Symbol = symbol;
                Kind = kind;
                Size = size;
            }

            public IFieldSymbol Symbol { get; private set; }
            public FieldKind Kind { get; private set; }
            public int Size { get; private set; }
            public ITypeSymbol Underlying { get; set; }
        }
    }
}
