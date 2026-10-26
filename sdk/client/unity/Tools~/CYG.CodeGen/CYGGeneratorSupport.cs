using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CookYourGame.CodeGen
{
    public sealed partial class CYGComponentGenerator
    {
        private static int ReadId(INamedTypeSymbol symbol, AttributeData attribute)
        {
            int id = 0;
            if (attribute.ConstructorArguments.Length > 0 && attribute.ConstructorArguments[0].Value is int)
            {
                id = (int)attribute.ConstructorArguments[0].Value;
            }
            foreach (KeyValuePair<string, TypedConstant> pair in attribute.NamedArguments)
            {
                if (pair.Key == "Id" && pair.Value.Value is int)
                {
                    id = (int)pair.Value.Value;
                }
            }

            if (id == 0)
            {
                id = unchecked((int)Fnv(symbol.ToDisplayString()));
            }
            return id == 0 ? 1 : id;
        }

        private static uint ComputeSchema(List<FieldSpec> fields)
        {
            uint hash = 2166136261u;
            for (int i = 0; i < fields.Count; i++)
            {
                FieldSpec field = fields[i];
                hash = MixString(hash, field.Symbol.Name);
                hash = MixString(hash, field.Symbol.Type.ToDisplayString());
                hash = MixInt(hash, field.Size);
            }
            return hash;
        }

        private static uint MixString(uint hash, string value)
        {
            for (int i = 0; i < value.Length; i++)
            {
                hash ^= value[i];
                hash *= 16777619u;
            }
            return hash;
        }

        private static uint MixInt(uint hash, int value)
        {
            hash ^= (byte)value;
            hash *= 16777619u;
            hash ^= (byte)(value >> 8);
            hash *= 16777619u;
            hash ^= (byte)(value >> 16);
            hash *= 16777619u;
            hash ^= (byte)(value >> 24);
            hash *= 16777619u;
            return hash;
        }

        private static uint Fnv(string value)
        {
            uint hash = 2166136261u;
            for (int i = 0; i < value.Length; i++)
            {
                hash ^= value[i];
                hash *= 16777619u;
            }
            return hash;
        }

        private static bool IsPartial(INamedTypeSymbol symbol)
        {
            foreach (SyntaxReference reference in symbol.DeclaringSyntaxReferences)
            {
                StructDeclarationSyntax syntax = reference.GetSyntax() as StructDeclarationSyntax;
                if (syntax == null || !syntax.Modifiers.Any(SyntaxKind.PartialKeyword))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool IsReadOnly(INamedTypeSymbol symbol)
        {
            foreach (SyntaxReference reference in symbol.DeclaringSyntaxReferences)
            {
                StructDeclarationSyntax syntax = reference.GetSyntax() as StructDeclarationSyntax;
                if (syntax != null && syntax.Modifiers.Any(SyntaxKind.ReadOnlyKeyword))
                {
                    return true;
                }
            }
            return false;
        }

        private static bool HasExplicitLayout(INamedTypeSymbol symbol)
        {
            foreach (SyntaxReference reference in symbol.DeclaringSyntaxReferences)
            {
                StructDeclarationSyntax syntax = reference.GetSyntax() as StructDeclarationSyntax;
                if (syntax == null)
                {
                    continue;
                }
                foreach (AttributeListSyntax list in syntax.AttributeLists)
                {
                    foreach (AttributeSyntax attributeSyntax in list.Attributes)
                    {
                        string text = attributeSyntax.ToString();
                        if (text.IndexOf("StructLayout", StringComparison.Ordinal) >= 0
                            && text.IndexOf("Explicit", StringComparison.Ordinal) >= 0)
                        {
                            return true;
                        }
                    }
                }
            }

            foreach (AttributeData attribute in symbol.GetAttributes())
            {
                if (attribute.AttributeClass == null
                    || attribute.AttributeClass.ToDisplayString() != "System.Runtime.InteropServices.StructLayoutAttribute")
                {
                    continue;
                }

                if (attribute.ConstructorArguments.Length > 0
                    && attribute.ConstructorArguments[0].Value != null
                    && attribute.ConstructorArguments[0].Value.ToString() == "Explicit")
                {
                    return true;
                }

                foreach (KeyValuePair<string, TypedConstant> pair in attribute.NamedArguments)
                {
                    if (pair.Key == "Value" && pair.Value.Value != null && pair.Value.Value.ToString() == "Explicit")
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        private static string EscapeIdentifier(string name)
        {
            if (string.IsNullOrEmpty(name))
            {
                return name;
            }
            return SyntaxFacts.GetKeywordKind(name) != SyntaxKind.None
                || SyntaxFacts.GetContextualKeywordKind(name) != SyntaxKind.None
                ? "@" + name
                : name;
        }

        private static string EscapeNamespace(string ns)
        {
            if (string.IsNullOrEmpty(ns))
            {
                return ns;
            }
            string[] parts = ns.Split('.');
            for (int i = 0; i < parts.Length; i++)
            {
                parts[i] = EscapeIdentifier(parts[i]);
            }
            return string.Join(".", parts);
        }

        private static string FieldOrder(IFieldSymbol field)
        {
            Location location = field.Locations.FirstOrDefault(l => l.IsInSource);
            if (location == null || location.SourceTree == null)
            {
                return "~";
            }
            // Do not include the file path: partial declarations must produce
            // the same wire order on every machine.
            return location.SourceSpan.Start.ToString("D10") + "|" + field.Name;
        }

        private static string HintName(INamedTypeSymbol symbol)
        {
            string name = symbol.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat);
            name = name.Replace("global::", string.Empty).Replace('.', '_').Replace('+', '_');
            return name + ".CYG.g.cs";
        }
    }
}
