using System.Collections.Generic;
using System.Linq;
using Microsoft.CodeAnalysis;

namespace CookYourGame.CodeGen
{
    public sealed partial class CYGComponentGenerator
    {
        private sealed class CandidateInfo
        {
            public CandidateInfo(INamedTypeSymbol symbol, AttributeData attribute, bool isComponent)
            {
                Symbol = symbol;
                Attribute = attribute;
                IsComponent = isComponent;
            }

            public INamedTypeSymbol Symbol { get; private set; }
            public AttributeData Attribute { get; private set; }
            public bool IsComponent { get; private set; }
        }

        /// <summary>
        /// Resolves wire sizes for structs that this generator emits in the same
        /// run. Their SizeInBytes constants do not exist yet in the input
        /// compilation, so the size has to be computed structurally.
        /// </summary>
        private sealed class GenerationContext
        {
            private readonly Dictionary<INamedTypeSymbol, CandidateInfo> _candidates =
                new Dictionary<INamedTypeSymbol, CandidateInfo>(SymbolEqualityComparer.Default);

            private readonly Dictionary<INamedTypeSymbol, int> _sizes =
                new Dictionary<INamedTypeSymbol, int>(SymbolEqualityComparer.Default);

            private readonly HashSet<INamedTypeSymbol> _visiting =
                new HashSet<INamedTypeSymbol>(SymbolEqualityComparer.Default);

            public void Add(CandidateInfo candidate)
            {
                _candidates[candidate.Symbol] = candidate;
            }

            public bool IsCandidate(INamedTypeSymbol type)
            {
                return type != null && _candidates.ContainsKey(type);
            }

            public bool TryGetSize(INamedTypeSymbol type, out int size)
            {
                if (_sizes.TryGetValue(type, out size))
                {
                    return true;
                }

                if (!_candidates.TryGetValue(type, out CandidateInfo candidate))
                {
                    size = 0;
                    return false;
                }

                if (!_visiting.Add(type))
                {
                    size = 0;
                    return false;
                }

                int total = 0;
                foreach (IFieldSymbol field in candidate.Symbol.GetMembers()
                    .OfType<IFieldSymbol>()
                    .Where(f => !f.IsStatic && !f.IsConst && !f.IsImplicitlyDeclared))
                {
                    int fieldSize;
                    if (!TryGetFieldSize(field.Type, out fieldSize))
                    {
                        _visiting.Remove(type);
                        size = 0;
                        return false;
                    }
                    total += fieldSize;
                }

                _visiting.Remove(type);
                _sizes[type] = total;
                size = total;
                return true;
            }

            private bool TryGetFieldSize(ITypeSymbol type, out int size)
            {
                if (type.TypeKind == TypeKind.Enum)
                {
                    INamedTypeSymbol enumType = (INamedTypeSymbol)type;
                    size = SizeOfSpecial(enumType.EnumUnderlyingType.SpecialType);
                    return size > 0;
                }

                switch (type.SpecialType)
                {
                    case SpecialType.System_Boolean:
                    case SpecialType.System_Byte:
                    case SpecialType.System_SByte:
                        size = 1;
                        return true;
                    case SpecialType.System_Int16:
                    case SpecialType.System_UInt16:
                    case SpecialType.System_Char:
                        size = 2;
                        return true;
                    case SpecialType.System_Int32:
                    case SpecialType.System_UInt32:
                    case SpecialType.System_Single:
                        size = 4;
                        return true;
                    case SpecialType.System_Int64:
                    case SpecialType.System_UInt64:
                    case SpecialType.System_Double:
                        size = 8;
                        return true;
                }

                INamedTypeSymbol named = type as INamedTypeSymbol;
                if (named == null || named.TypeKind != TypeKind.Struct || named.IsGenericType)
                {
                    size = 0;
                    return false;
                }

                if (_candidates.ContainsKey(named))
                {
                    return TryGetSize(named, out size);
                }

                size = ReadSize(named);
                return size > 0;
            }
        }
    }
}
