using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace CookYourGame.CodeGen
{
    [Generator]
    public sealed partial class CYGComponentGenerator : ISourceGenerator
    {
        private const string ComponentAttributeName = "CookYourGame.Sim.CYGComponentAttribute";
        private const string ValueAttributeName = "CookYourGame.Sim.CYGValueAttribute";

        public void Initialize(GeneratorInitializationContext context)
        {
            context.RegisterForSyntaxNotifications(() => new Receiver());
        }

        public void Execute(GeneratorExecutionContext context)
        {
            Receiver receiver = context.SyntaxReceiver as Receiver;
            if (receiver == null)
            {
                return;
            }

            INamedTypeSymbol attribute = context.Compilation.GetTypeByMetadataName(ComponentAttributeName);
            INamedTypeSymbol valueAttribute = context.Compilation.GetTypeByMetadataName(ValueAttributeName);
            if (attribute == null && valueAttribute == null)
            {
                return;
            }

            HashSet<string> emitted = new HashSet<string>();
            Dictionary<int, string> ids = new Dictionary<int, string>();
            GenerationContext state = new GenerationContext();
            List<CandidateInfo> candidates = new List<CandidateInfo>();

            foreach (StructDeclarationSyntax candidate in receiver.Candidates)
            {
                SemanticModel model = context.Compilation.GetSemanticModel(candidate.SyntaxTree);
                INamedTypeSymbol symbol = model.GetDeclaredSymbol(candidate) as INamedTypeSymbol;
                if (symbol == null || symbol.TypeKind != TypeKind.Struct)
                {
                    continue;
                }

                AttributeData componentAttribute = FindAttribute(symbol, attribute);
                bool isComponent = componentAttribute != null;
                if (componentAttribute == null)
                {
                    componentAttribute = FindAttribute(symbol, valueAttribute);
                }
                if (componentAttribute == null)
                {
                    continue;
                }

                string key = symbol.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat);
                if (!emitted.Add(key))
                {
                    continue;
                }

                CandidateInfo info = new CandidateInfo(symbol, componentAttribute, isComponent);
                state.Add(info);
                candidates.Add(info);
            }

            foreach (CandidateInfo candidate in candidates)
            {
                string source = Build(candidate, context, ids, state);
                if (source != null)
                {
                    context.AddSource(HintName(candidate.Symbol), SourceText.From(source, Encoding.UTF8));
                }
            }
        }

        private static AttributeData FindAttribute(INamedTypeSymbol symbol, INamedTypeSymbol attribute)
        {
            if (attribute == null)
            {
                return null;
            }

            foreach (AttributeData data in symbol.GetAttributes())
            {
                if (SymbolEqualityComparer.Default.Equals(data.AttributeClass, attribute))
                {
                    return data;
                }
            }
            return null;
        }

        private static string Build(
            CandidateInfo candidate,
            GeneratorExecutionContext context,
            Dictionary<int, string> ids,
            GenerationContext state)
        {
            INamedTypeSymbol symbol = candidate.Symbol;
            AttributeData componentAttribute = candidate.Attribute;
            bool isComponent = candidate.IsComponent;
            bool failed = false;
            Location location = symbol.Locations.FirstOrDefault() ?? Location.None;

            if (symbol.ContainingType != null)
            {
                Report(context, CYGDiagnostics.NestedTypeUnsupported, location, symbol.Name);
                return null;
            }
            if (symbol.TypeParameters.Length != 0)
            {
                Report(context, CYGDiagnostics.GenericUnsupported, location, symbol.Name);
                return null;
            }
            if (symbol.IsStatic || IsReadOnly(symbol))
            {
                Report(context, CYGDiagnostics.ReadonlyStructUnsupported, location, symbol.Name);
                return null;
            }
            if (symbol.IsRefLikeType)
            {
                Report(context, CYGDiagnostics.RefStructUnsupported, location, symbol.Name);
                return null;
            }
            if (HasExplicitLayout(symbol))
            {
                Report(context, CYGDiagnostics.ExplicitLayoutUnsupported, location, symbol.Name);
                return null;
            }
            if (!IsPartial(symbol))
            {
                Report(context, CYGDiagnostics.MustBePartial, location, symbol.Name);
                failed = true;
            }

            List<FieldSpec> fields = CollectFields(symbol, context, ref failed, state, !isComponent);
            if (failed)
            {
                return null;
            }

            int id = 0;
            uint schema = 0;
            if (isComponent)
            {
                id = ReadId(symbol, componentAttribute);
                string previous;
                if (ids.TryGetValue(id, out previous))
                {
                    Report(context, CYGDiagnostics.DuplicateTypeId, location, id, previous);
                    return null;
                }
                ids.Add(id, symbol.Name);
                schema = ComputeSchema(fields);
            }

            int size = 0;
            for (int i = 0; i < fields.Count; i++)
            {
                size += fields[i].Size;
            }

            return Emit(symbol, fields, id, schema, size, isComponent);
        }

        private static List<FieldSpec> CollectFields(
            INamedTypeSymbol symbol,
            GeneratorExecutionContext context,
            ref bool failed,
            GenerationContext state,
            bool allowEmpty)
        {
            List<FieldSpec> fields = new List<FieldSpec>();
            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal)
            {
                "SizeInBytes", "TypeId", "SchemaHash", "WireSize",
                "ComponentTypeId", "ComponentSchemaHash",
                "WriteTo", "ReadFrom", "ComputeChecksum", "CopyTo"
            };

            IEnumerable<IFieldSymbol> members = symbol.GetMembers()
                .OfType<IFieldSymbol>()
                .Where(f => !f.IsStatic && !f.IsConst && !f.IsImplicitlyDeclared)
                .OrderBy(FieldOrder);

            foreach (IFieldSymbol field in members)
            {
                if (names.Contains(field.Name))
                {
                    Report(context, CYGDiagnostics.MemberConflict, field.Locations.FirstOrDefault(), symbol.Name, field.Name);
                    failed = true;
                    continue;
                }
                names.Add(field.Name);

                if (field.IsReadOnly)
                {
                    Report(context, CYGDiagnostics.ReadonlyField, field.Locations.FirstOrDefault(), field.Name);
                    failed = true;
                    continue;
                }
                if (field.IsFixedSizeBuffer)
                {
                    Report(context, CYGDiagnostics.UnsupportedField, field.Locations.FirstOrDefault(), field.Name, field.Type.ToDisplayString());
                    failed = true;
                    continue;
                }

                FieldSpec spec = BuildField(symbol, field, context, state);
                if (spec == null)
                {
                    failed = true;
                    continue;
                }
                fields.Add(spec);
            }

            if (fields.Count == 0 && !failed && !allowEmpty)
            {
                Report(context, CYGDiagnostics.NoFields, symbol.Locations.FirstOrDefault(), symbol.Name);
            }
            return fields;
        }

        private static void Report(GeneratorExecutionContext context, DiagnosticDescriptor descriptor, Location location, params object[] args)
        {
            context.ReportDiagnostic(Diagnostic.Create(descriptor, location ?? Location.None, args));
        }

        private sealed class Receiver : ISyntaxReceiver
        {
            public List<StructDeclarationSyntax> Candidates { get; private set; }

            public Receiver()
            {
                Candidates = new List<StructDeclarationSyntax>();
            }

            public void OnVisitSyntaxNode(SyntaxNode syntaxNode)
            {
                StructDeclarationSyntax syntax = syntaxNode as StructDeclarationSyntax;
                if (syntax != null && syntax.AttributeLists.Count > 0)
                {
                    Candidates.Add(syntax);
                }
            }
        }
    }
}
