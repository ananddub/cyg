using System;
using System.Collections.Generic;
using System.Linq;
using CookYourGame.Dsl;

namespace CookYourGame.Dsl.Core
{
    /// <summary>
    /// Enforces the CYG golden rules and resolves ids. The emitter assumes a
    /// module that passed this pass: every named type exists, every container
    /// has a legal fixed capacity, and every component/event has a stable id.
    /// </summary>
    internal static class CygValidator
    {
        private const int MaxArrayLength = 1024;
        private const int MaxCollectionLength = 256;
        private const int MaxStringLength = 256;
        private const int MaxButtons = 64;
        private const int MaxDepth = 32;
        private const int MaxSchemaFields = 32;

        private static readonly HashSet<string> BuiltinTypeNames = new HashSet<string>(StringComparer.Ordinal)
        {
            "FP", "Byte", "byte", "SByte", "sbyte", "Int16", "short", "UInt16", "ushort",
            "Int32", "int", "UInt32", "uint", "Int64", "long", "UInt64", "ulong",
            "Boolean", "bool", "FPVector2", "FPVector3", "FPQuaternion", "FPMatrix4x4",
            "FPAABB", "FPBounds", "entity_ref", "player_ref", "QString", "array", "list",
            "dictionary", "asset_ref", "float", "double", "decimal", "string", "object",
            "String", "List", "Dictionary", "HashSet", "Queue", "Stack", "Nullable"
        };

        public static void Validate(CygModule module, CygDiagnosticBag diagnostics)
        {
            ValidateNamespace(module, diagnostics);
            ValidateNames(module, diagnostics);
            ValidateDeclarations(module, diagnostics);
            ValidateIds(module, diagnostics);
            ValidateEmittedNames(module, diagnostics);
        }

        private static void ValidateNamespace(CygModule module, CygDiagnosticBag diagnostics)
        {
            string ns = module.Namespace ?? string.Empty;
            if (ns.Length == 0)
            {
                diagnostics.Error(CygDiagnosticIds.Parser, "The generated namespace cannot be empty.", CygSpan.None);
                return;
            }

            string[] segments = ns.Split('.');
            foreach (string segment in segments)
            {
                if (!IsIdentifier(segment))
                {
                    diagnostics.Error(CygDiagnosticIds.Parser,
                        $"'{segment}' is not a valid namespace segment.",
                        new CygSpan(module.SourcePath, 1, 1, 0));
                    return;
                }
            }
        }

        private static void ValidateNames(CygModule module, CygDiagnosticBag diagnostics)
        {
            Dictionary<string, CygSpan> declared = new Dictionary<string, CygSpan>(StringComparer.Ordinal);

            foreach (CygTypeDecl type in module.Types)
            {
                if (CygNames.IsReservedTypeName(type.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Type name '{type.Name}' is reserved for generated support types. Use a name that does not start with 'CYG' or end in FixedArray/FixedList/FixedDictionary/FixedString + digits.",
                        type.Span);
                }

                RejectBuiltinName("Type", type.Name, type.Span, diagnostics);
                Claim(declared, type.Name, type.Span, "type", diagnostics);
            }

            foreach (CygEventDecl eventDecl in module.Events)
            {
                if (CygNames.IsReservedTypeName(eventDecl.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Event name '{eventDecl.Name}' is reserved for generated support types.",
                        eventDecl.Span);
                }

                RejectBuiltinName("Event", eventDecl.Name, eventDecl.Span, diagnostics);
                Claim(declared, eventDecl.Name, eventDecl.Span, "event", diagnostics);
            }

            foreach (CygCommandDecl command in module.Commands)
            {
                if (CygNames.IsReservedTypeName(command.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Command name '{command.Name}' is reserved for generated support types.",
                        command.Span);
                }

                RejectBuiltinName("Command", command.Name, command.Span, diagnostics);
                Claim(declared, command.Name, command.Span, "command", diagnostics);
                Claim(declared, command.Name + "Command", command.Span, "command adapter", diagnostics);
            }

            foreach (CygSignalDecl signal in module.Signals)
            {
                if (CygNames.IsReservedTypeName(signal.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Signal name '{signal.Name}' is reserved for generated support types.",
                        signal.Span);
                }

                RejectBuiltinName("Signal", signal.Name, signal.Span, diagnostics);
                Claim(declared, signal.Name, signal.Span, "signal", diagnostics);
            }

            Dictionary<string, CygSpan> assets = new Dictionary<string, CygSpan>(StringComparer.Ordinal);
            foreach (CygAssetDecl asset in module.Assets)
            {
                if (CygNames.IsReservedTypeName(asset.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Asset name '{asset.Name}' is reserved for generated support types.",
                        asset.Span);
                }

                RejectBuiltinName("Asset", asset.Name, asset.Span, diagnostics);
                Claim(assets, asset.Name, asset.Span, "asset", diagnostics);
            }

            foreach (CygSchemaDecl schema in module.Schemas)
            {
                if (CygNames.IsReservedTypeName(schema.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Schema name '{schema.Name}' is reserved for generated support types.",
                        schema.Span);
                }

                RejectBuiltinName("Schema", schema.Name, schema.Span, diagnostics);
                Claim(declared, schema.Name, schema.Span, "schema", diagnostics);
            }
        }

        private static void RejectBuiltinName(
            string what,
            string name,
            CygSpan span,
            CygDiagnosticBag diagnostics)
        {
            if (BuiltinTypeNames.Contains(name))
            {
                diagnostics.Error(CygDiagnosticIds.ReservedName,
                    $"{what} name '{name}' is a builtin DSL type and cannot be redeclared.",
                    span);
            }
        }

        private static void Claim(
            Dictionary<string, CygSpan> declared,
            string name,
            CygSpan span,
            string what,
            CygDiagnosticBag diagnostics)
        {
            if (declared.TryGetValue(name, out CygSpan previous))
            {
                diagnostics.Error(CygDiagnosticIds.DuplicateType,
                    $"Duplicate {what} name '{name}' (first declared at {previous}).",
                    span);
                return;
            }

            declared.Add(name, span);
        }

        private static void ValidateDeclarations(CygModule module, CygDiagnosticBag diagnostics)
        {
            foreach (CygTypeDecl type in module.Types)
            {
                switch (type)
                {
                    case CygStructDecl structDecl:
                        ValidateFields(module, structDecl.Fields, structDecl.Name, diagnostics, 0);
                        RequireSize(module, structDecl.Fields, structDecl.Span, diagnostics);
                        break;

                    case CygComponentDecl component:
                        ValidateFields(module, component.Fields, component.Name, diagnostics, 0);
                        RequireSize(module, component.Fields, component.Span, diagnostics);
                        break;

                    case CygEnumDecl enumDecl:
                        ValidateEnum(enumDecl, diagnostics);
                        break;

                    case CygUnionDecl union:
                        ValidateUnion(module, union, diagnostics);
                        break;
                }
            }

            foreach (CygTypeDecl type in module.Types)
            {
                if (type is CygStructDecl || type is CygUnionDecl)
                {
                    CheckCycle(module, type.Name, new HashSet<string>(StringComparer.Ordinal), diagnostics);
                }
            }

            if (module.Input != null)
            {
                ValidateInput(module, module.Input, diagnostics);
            }

            if (module.Global != null)
            {
                ValidateFields(module, module.Global.Fields, module.Global.Name, diagnostics, 0);
                RequireSize(module, module.Global.Fields, module.Global.Span, diagnostics);
            }

            foreach (CygSchemaDecl schema in module.Schemas)
            {
                ValidateSchema(module, schema, diagnostics);
            }

            foreach (CygEventDecl eventDecl in module.Events)
            {
                ValidateFields(module, eventDecl.Fields, eventDecl.Name, diagnostics, 0);
                RequireSize(module, eventDecl.Fields, eventDecl.Span, diagnostics);
                foreach (CygField field in eventDecl.Fields)
                {
                    if (field.Name == "EventId" || field.Name == "Synced")
                    {
                        diagnostics.Error(CygDiagnosticIds.ReservedName,
                            $"Event field '{field.Name}' collides with a generated event constant.",
                            field.Span);
                    }
                }
            }

            foreach (CygCommandDecl command in module.Commands)
            {
                ValidateFields(module, command.Fields, command.Name, diagnostics, 0);
                RequireSize(module, command.Fields, command.Span, diagnostics);
                foreach (CygField field in command.Fields)
                {
                    if (field.Name == "CommandId" || field.Name == "CommandTypeId")
                    {
                        diagnostics.Error(CygDiagnosticIds.ReservedName,
                            $"Command field '{field.Name}' collides with a generated command constant.",
                            field.Span);
                    }
                }
            }

            foreach (CygSignalDecl signal in module.Signals)
            {
                HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
                foreach (CygSignalParam parameter in signal.Parameters)
                {
                    if (!names.Add(parameter.Name))
                    {
                        diagnostics.Error(CygDiagnosticIds.DuplicateField,
                            $"Signal '{signal.Name}' declares parameter '{parameter.Name}' more than once.",
                            parameter.Span);
                    }

                    if (parameter.Name == "SignalIndex" || CygReserved.IsFieldName(parameter.Name))
                    {
                        diagnostics.Error(CygDiagnosticIds.ReservedName,
                            $"Signal parameter name '{parameter.Name}' is reserved.",
                            parameter.Span);
                    }

                    ValidateType(module, parameter.Type, parameter.Span, diagnostics, 0);
                }

                int total = 0;
                bool sizeOk = true;
                foreach (CygSignalParam parameter in signal.Parameters)
                {
                    int? size = CygLayout.SizeOf(module, parameter.Type);
                    if (size == null)
                    {
                        sizeOk = false;
                        break;
                    }

                    total += size.Value;
                }

                if (!sizeOk)
                {
                    diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                        $"Signal '{signal.Name}' has a parameter whose fixed wire size cannot be computed.",
                        signal.Span);
                }
            }
        }

        private static void ValidateFields(
            CygModule module,
            List<CygField> fields,
            string owner,
            CygDiagnosticBag diagnostics,
            int depth)
        {
            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
            foreach (CygField field in fields)
            {
                if (CygReserved.IsFieldName(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Field '{field.Name}' in '{owner}' collides with a generated member.",
                        field.Span);
                }

                if (!names.Add(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.DuplicateField,
                        $"'{owner}' declares field '{field.Name}' more than once.",
                        field.Span);
                }

                ValidateType(module, field.Type, field.Span, diagnostics, depth);
            }
        }

        private static void ValidateSchema(
            CygModule module,
            CygSchemaDecl schema,
            CygDiagnosticBag diagnostics)
        {
            if (schema.Version == 0)
            {
                diagnostics.Error(CygDiagnosticIds.Schema,
                    $"Schema '{schema.Name}' version must be at least 1.",
                    schema.Span);
            }

            if (schema.Fields.Count == 0)
            {
                diagnostics.Error(CygDiagnosticIds.Schema,
                    $"Schema '{schema.Name}' must declare at least one field.",
                    schema.Span);
            }

            if (schema.Fields.Count > MaxSchemaFields)
            {
                diagnostics.Error(CygDiagnosticIds.Schema,
                    $"Schema '{schema.Name}' has {schema.Fields.Count} fields; the sync wire format allows at most {MaxSchemaFields}.",
                    schema.Span);
            }

            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
            foreach (CygField field in schema.Fields)
            {
                if (IsReservedSchemaField(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Schema field '{field.Name}' is reserved by SchemaEntity or the generated code.",
                        field.Span);
                }

                if (!names.Add(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.DuplicateField,
                        $"Schema '{schema.Name}' declares field '{field.Name}' more than once.",
                        field.Span);
                }

                if (!TryMapSchemaWireType(module, field.Type, out string? reason))
                {
                    diagnostics.Error(CygDiagnosticIds.Schema,
                        $"Schema field '{field.Name}' in '{schema.Name}': {reason}",
                        field.Span);
                }
            }
        }

        private static bool IsReservedSchemaField(string name)
        {
            return name == "id"
                || name == "schemaHash"
                || name == "dirtyMask"
                || name == "clearDirty"
                || name == "schema"
                || CygReserved.IsFieldName(name);
        }

        /// <summary>
        /// Mirror of engine-schema's wire table. Keep enum/flags support in sync:
        /// only underlying types with a SchemaDef builder method are allowed.
        /// </summary>
        private static bool TryMapSchemaWireType(CygModule module, CygTypeRef? type, out string? reason)
        {
            reason = null;
            if (type == null)
            {
                reason = "missing type reference.";
                return false;
            }

            switch (type.Kind)
            {
                case CygTypeKind.FP:
                case CygTypeKind.Byte:
                case CygTypeKind.UInt16:
                case CygTypeKind.Int32:
                case CygTypeKind.UInt32:
                case CygTypeKind.Int64:
                case CygTypeKind.Boolean:
                case CygTypeKind.EntityRef:
                case CygTypeKind.PlayerRef:
                    return true;

                case CygTypeKind.SByte:
                case CygTypeKind.Int16:
                case CygTypeKind.UInt64:
                    reason = $"type '{type.Display()}' has no schema wire encoding; use Byte, UInt16, Int32, UInt32, Int64, FP or Boolean.";
                    return false;

                case CygTypeKind.Named:
                    CygTypeDecl? declaration = module.FindType(type.Name ?? string.Empty);
                    if (declaration is CygEnumDecl enumDecl)
                    {
                        switch (enumDecl.Underlying)
                        {
                            case CygPrimitive.Byte:
                            case CygPrimitive.UInt16:
                            case CygPrimitive.Int32:
                            case CygPrimitive.UInt32:
                            case CygPrimitive.Int64:
                                return true;
                            default:
                                reason = $"enum '{enumDecl.Name}' uses underlying type '{enumDecl.Underlying}', which has no schema wire encoding.";
                                return false;
                        }
                    }

                    reason = $"type '{type.Name}' cannot be synced as a schema field; use numeric, Boolean, entity_ref, player_ref or an enum/flags.";
                    return false;

                default:
                    reason = $"type '{type.Display()}' cannot be synced as one schema field; split it into numeric/Boolean fields.";
                    return false;
            }
        }

        private static void ValidateType(
            CygModule module,
            CygTypeRef? type,
            CygSpan span,
            CygDiagnosticBag diagnostics,
            int depth)
        {
            if (type == null)
            {
                diagnostics.Error(CygDiagnosticIds.UnsupportedType, "Missing type reference.", span);
                return;
            }

            if (depth > MaxDepth)
            {
                diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                    $"Type nesting is deeper than {MaxDepth} levels.",
                    span);
                return;
            }

            switch (type.Kind)
            {
                case CygTypeKind.Named:
                    CygTypeDecl? declaration = module.FindType(type.Name ?? string.Empty);
                    if (declaration == null)
                    {
                        diagnostics.Error(CygDiagnosticIds.UnknownType,
                            $"Unknown type '{type.Name}'. Declare it with struct, component, enum, flags or union.",
                            span);
                    }
                    else if (declaration is CygComponentDecl)
                    {
                        diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                            $"Component '{declaration.Name}' cannot be embedded as a field. Store an entity_ref instead.",
                            span);
                    }
                    break;

                case CygTypeKind.AssetRef:
                    if (module.FindAsset(type.Name ?? string.Empty) == null)
                    {
                        diagnostics.Error(CygDiagnosticIds.UnknownAsset,
                            $"asset_ref<{type.Name}> refers to an asset that is not declared. Add 'asset {type.Name};'.",
                            span);
                    }
                    break;

                case CygTypeKind.FixedString:
                    if (type.Length < 1 || type.Length > MaxStringLength)
                    {
                        diagnostics.Error(CygDiagnosticIds.StringLength,
                            $"QString capacity must be between 1 and {MaxStringLength}; got {type.Length}.",
                            span);
                    }
                    break;

                case CygTypeKind.FixedArray:
                    ValidateLength(type.Length, 1, MaxArrayLength, "array", span, diagnostics);
                    ValidateType(module, type.Element, span, diagnostics, depth + 1);
                    break;

                case CygTypeKind.FixedList:
                    ValidateLength(type.Length, 1, MaxCollectionLength, "list", span, diagnostics);
                    ValidateType(module, type.Element, span, diagnostics, depth + 1);
                    break;

                case CygTypeKind.FixedDictionary:
                    ValidateLength(type.Length, 1, MaxCollectionLength, "dictionary", span, diagnostics);
                    ValidateType(module, type.Key, span, diagnostics, depth + 1);
                    ValidateType(module, type.Value, span, diagnostics, depth + 1);
                    ValidateDictionaryKey(module, type.Key, span, diagnostics);
                    break;
            }
        }

        private static void ValidateLength(
            int length,
            int min,
            int max,
            string what,
            CygSpan span,
            CygDiagnosticBag diagnostics)
        {
            if (length < min || length > max)
            {
                diagnostics.Error(CygDiagnosticIds.ArrayLength,
                    $"{what} capacity must be between {min} and {max}; got {length}.",
                    span);
            }
        }

        private static void ValidateDictionaryKey(
            CygModule module,
            CygTypeRef? key,
            CygSpan span,
            CygDiagnosticBag diagnostics)
        {
            if (key == null)
            {
                return;
            }

            bool hashable;
            switch (key.Kind)
            {
                case CygTypeKind.Byte:
                case CygTypeKind.SByte:
                case CygTypeKind.Int16:
                case CygTypeKind.UInt16:
                case CygTypeKind.Int32:
                case CygTypeKind.UInt32:
                case CygTypeKind.Int64:
                case CygTypeKind.UInt64:
                case CygTypeKind.Boolean:
                case CygTypeKind.FP:
                case CygTypeKind.EntityRef:
                case CygTypeKind.PlayerRef:
                case CygTypeKind.AssetRef:
                    hashable = true;
                    break;
                case CygTypeKind.Named:
                    CygTypeDecl? declaration = module.FindType(key.Name ?? string.Empty);
                    hashable = declaration is CygEnumDecl;
                    break;
                default:
                    hashable = false;
                    break;
            }

            if (!hashable)
            {
                diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                    $"Dictionary key '{key.Display()}' is not a hashable deterministic value. Use an integer, FP, enum, entity_ref, player_ref or asset_ref.",
                    span);
            }
        }

        private static void ValidateEnum(CygEnumDecl declaration, CygDiagnosticBag diagnostics)
        {
            if (declaration.IsFlags && !declaration.UnderlyingExplicit)
            {
                diagnostics.Error(CygDiagnosticIds.Flags,
                    $"Flags '{declaration.Name}' must declare an explicit underlying type, for example '{declaration.Name} : Byte'.",
                    declaration.Span);
            }

            if (declaration.Members.Count == 0)
            {
                diagnostics.Error(CygDiagnosticIds.Parser,
                    $"{(declaration.IsFlags ? "Flags" : "Enum")} '{declaration.Name}' must declare at least one member.",
                    declaration.Span);
                return;
            }

            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
            long next = 0;
            long nextBit = 1;
            foreach (CygEnumMember member in declaration.Members)
            {
                if (!names.Add(member.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.DuplicateField,
                        $"'{declaration.Name}' declares member '{member.Name}' more than once.",
                        member.Span);
                }

                long value;
                if (declaration.IsFlags)
                {
                    if (member.Value.HasValue)
                    {
                        value = member.Value.Value;
                    }
                    else if (string.Equals(member.Name, "None", StringComparison.OrdinalIgnoreCase))
                    {
                        value = 0;
                    }
                    else
                    {
                        value = nextBit;
                        nextBit = nextBit > (long.MaxValue >> 1) ? long.MaxValue : nextBit << 1;
                    }

                    if (value < 0)
                    {
                        diagnostics.Error(CygDiagnosticIds.Flags,
                            $"Flag '{declaration.Name}.{member.Name}' cannot be negative.",
                            member.Span);
                    }
                }
                else
                {
                    value = member.Value ?? next;
                    next = value == long.MaxValue ? long.MaxValue : value + 1;
                }

                if (!FitsUnderlying(declaration.Underlying, value))
                {
                    diagnostics.Error(CygDiagnosticIds.Flags,
                        $"Value {value} for '{declaration.Name}.{member.Name}' does not fit in {declaration.Underlying}.",
                        member.Span);
                }

                member.Value = value;
            }
        }

        private static void ValidateUnion(CygModule module, CygUnionDecl union, CygDiagnosticBag diagnostics)
        {
            if (union.Variants.Count < 2)
            {
                diagnostics.Error(CygDiagnosticIds.Union,
                    $"Union '{union.Name}' must declare at least two variants.",
                    union.Span);
            }

            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
            HashSet<string> tagConstants = new HashSet<string>(StringComparer.Ordinal);
            int longest = 0;
            foreach (CygUnionVariant variant in union.Variants)
            {
                if (!names.Add(variant.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.Union,
                        $"Union '{union.Name}' declares variant '{variant.Name}' more than once.",
                        variant.Span);
                }

                string tagConstant = CygNames.SanitizeIdentifier("Tag" + variant.Name);
                if (!tagConstants.Add(tagConstant))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Union variant '{variant.Name}' collides with another variant's '{tagConstant}' constant.",
                        variant.Span);
                }

                if (variant.Name == "Tag" || CygReserved.IsFieldName(variant.Name) || variant.Name == "Clear")
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Union variant '{variant.Name}' collides with a generated member of '{union.Name}'.",
                        variant.Span);
                }

                if (variant.Fields.Count == 0)
                {
                    diagnostics.Error(CygDiagnosticIds.Union,
                        $"Union variant '{union.Name}.{variant.Name}' must declare at least one field.",
                        variant.Span);
                }

                ValidateFields(module, variant.Fields, union.Name + "." + variant.Name, diagnostics, 0);
                int? size = CygLayout.SizeOfFields(module, variant.Fields);
                if (size == null)
                {
                    diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                        $"Variant '{union.Name}.{variant.Name}' has a field whose fixed wire size cannot be computed.",
                        variant.Span);
                }
                else if (size.Value > longest)
                {
                    longest = size.Value;
                }
            }
        }

        private static void ValidateInput(CygModule module, CygInputDecl input, CygDiagnosticBag diagnostics)
        {
            if (input.Fields.Count == 0)
            {
                diagnostics.Error(CygDiagnosticIds.Input,
                    "The input block must declare at least one button or field.",
                    input.Span);
            }

            HashSet<string> names = new HashSet<string>(StringComparer.Ordinal);
            int buttons = 0;
            foreach (CygInputField field in input.Fields)
            {
                if (!names.Add(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.DuplicateField,
                        $"Input declares '{field.Name}' more than once.",
                        field.Span);
                }

                if (field.Name == "_buttons" || CygReserved.IsFieldName(field.Name))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Input member '{field.Name}' collides with a generated member.",
                        field.Span);
                }

                if (field.IsButton)
                {
                    buttons++;
                    continue;
                }

                ValidateType(module, field.Type, field.Span, diagnostics, 0);
                int? size = CygLayout.SizeOf(module, field.Type!);
                if (size == null)
                {
                    diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                        $"Input field '{field.Name}' has a type whose fixed wire size cannot be computed.",
                        field.Span);
                }
            }

            if (buttons > MaxButtons)
            {
                diagnostics.Error(CygDiagnosticIds.Input,
                    $"Input declares {buttons} buttons; the button mask supports at most {MaxButtons}.",
                    input.Span);
            }
        }

        private static void ValidateIds(CygModule module, CygDiagnosticBag diagnostics)
        {
            Dictionary<int, string> usedComponents = new Dictionary<int, string>();
            foreach (CygComponentDecl component in module.Components)
            {
                if (!component.Id.HasValue)
                {
                    continue;
                }

                int id = component.Id.Value;
                if (!CygIds.IsUserComponent(id))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedId,
                        $"Component '{component.Name}' id {id} is outside the user range {CygIds.FirstComponent}..{CygIds.LastComponent}.",
                        component.Span);
                    continue;
                }

                if (usedComponents.TryGetValue(id, out string? previous))
                {
                    diagnostics.Error(CygDiagnosticIds.ComponentId,
                        $"Component id {id} for '{component.Name}' is already used by '{previous}'.",
                        component.Span);
                    continue;
                }

                usedComponents.Add(id, component.Name);
            }

            foreach (CygComponentDecl component in module.Components
                .Where(c => !c.Id.HasValue)
                .OrderBy(c => c.Name, StringComparer.Ordinal))
            {
                int id = DeriveId(component.Name, CygIds.FirstComponent, CygIds.LastComponent);
                int origin = id;
                while (usedComponents.ContainsKey(id))
                {
                    id++;
                    if (id > CygIds.LastComponent)
                    {
                        id = CygIds.FirstComponent;
                    }
                    if (id == origin)
                    {
                        diagnostics.Error(CygDiagnosticIds.ComponentId,
                            $"No free component id remains for '{component.Name}'.",
                            component.Span);
                        break;
                    }
                }

                component.Id = id;
                usedComponents[id] = component.Name;
                diagnostics.Warning(CygDiagnosticIds.MissingExplicitId,
                    $"Component '{component.Name}' has no explicit id; derived {id}. Set ': {id}' explicitly before shipping.",
                    component.Span);
            }

            Dictionary<int, string> usedEvents = new Dictionary<int, string>();
            foreach (CygEventDecl eventDecl in module.Events)
            {
                if (eventDecl.Id.HasValue)
                {
                    int id = eventDecl.Id.Value;
                    if (!CygIds.IsEvent(id))
                    {
                        diagnostics.Error(CygDiagnosticIds.EventId,
                            $"Event '{eventDecl.Name}' id {id} is outside the event range {CygIds.FirstEvent}..{CygIds.LastEvent}.",
                            eventDecl.Span);
                        continue;
                    }

                    if (usedEvents.TryGetValue(id, out string? previous))
                    {
                        diagnostics.Error(CygDiagnosticIds.EventId,
                            $"Event id {id} for '{eventDecl.Name}' is already used by '{previous}'.",
                            eventDecl.Span);
                        continue;
                    }

                    usedEvents.Add(id, eventDecl.Name);
                }
            }

            foreach (CygEventDecl eventDecl in module.Events
                .Where(e => !e.Id.HasValue)
                .OrderBy(e => e.Name, StringComparer.Ordinal))
            {
                int id = DeriveId(eventDecl.Name, CygIds.FirstEvent, CygIds.LastEvent);
                int origin = id;
                while (usedEvents.ContainsKey(id))
                {
                    id++;
                    if (id > CygIds.LastEvent)
                    {
                        id = CygIds.FirstEvent;
                    }
                    if (id == origin)
                    {
                        diagnostics.Error(CygDiagnosticIds.EventId,
                            $"No free event id remains for '{eventDecl.Name}'.",
                            eventDecl.Span);
                        break;
                    }
                }

                eventDecl.Id = id;
                usedEvents[id] = eventDecl.Name;
                diagnostics.Warning(CygDiagnosticIds.MissingExplicitId,
                    $"Event '{eventDecl.Name}' has no explicit id; derived {id}. Set ': {id}' explicitly before shipping.",
                    eventDecl.Span);
            }

            Dictionary<int, string> usedCommands = new Dictionary<int, string>();
            foreach (CygCommandDecl command in module.Commands)
            {
                if (!command.Id.HasValue)
                {
                    continue;
                }

                int id = command.Id.Value;
                if (!CygIds.IsCommand(id))
                {
                    diagnostics.Error(CygDiagnosticIds.CommandId,
                        $"Command '{command.Name}' id {id} is outside the command range {CygIds.FirstCommand}..{CygIds.LastCommand}.",
                        command.Span);
                    continue;
                }

                if (usedCommands.TryGetValue(id, out string? previous))
                {
                    diagnostics.Error(CygDiagnosticIds.CommandId,
                        $"Command id {id} for '{command.Name}' is already used by '{previous}'.",
                        command.Span);
                    continue;
                }

                usedCommands.Add(id, command.Name);
            }

            foreach (CygCommandDecl command in module.Commands
                .Where(c => !c.Id.HasValue)
                .OrderBy(c => c.Name, StringComparer.Ordinal))
            {
                int id = DeriveId(command.Name, CygIds.FirstCommand, CygIds.LastCommand);
                int origin = id;
                while (usedCommands.ContainsKey(id))
                {
                    id++;
                    if (id > CygIds.LastCommand)
                    {
                        id = CygIds.FirstCommand;
                    }
                    if (id == origin)
                    {
                        diagnostics.Error(CygDiagnosticIds.CommandId,
                            $"No free command id remains for '{command.Name}'.",
                            command.Span);
                        break;
                    }
                }

                command.Id = id;
                usedCommands[id] = command.Name;
                diagnostics.Warning(CygDiagnosticIds.MissingExplicitId,
                    $"Command '{command.Name}' has no explicit id; derived {id}. Set ': {id}' explicitly before shipping.",
                    command.Span);
            }
        }

        private static void CheckCycle(
            CygModule module,
            string typeName,
            HashSet<string> path,
            CygDiagnosticBag diagnostics)
        {
            CygTypeDecl? declaration = module.FindType(typeName);
            if (declaration == null || !path.Add(typeName))
            {
                return;
            }

            IEnumerable<CygField> fields;
            if (declaration is CygStructDecl structDecl)
            {
                fields = structDecl.Fields;
            }
            else if (declaration is CygUnionDecl union)
            {
                fields = union.Variants.SelectMany(v => v.Fields);
            }
            else
            {
                path.Remove(typeName);
                return;
            }

            foreach (CygField field in fields)
            {
                VisitCycle(module, field.Type, field.Span, path, diagnostics);
            }

            path.Remove(typeName);
        }

        private static void VisitCycle(
            CygModule module,
            CygTypeRef? type,
            CygSpan span,
            HashSet<string> path,
            CygDiagnosticBag diagnostics)
        {
            if (type == null)
            {
                return;
            }

            switch (type.Kind)
            {
                case CygTypeKind.Named:
                    string name = type.Name ?? string.Empty;
                    if (path.Contains(name))
                    {
                        diagnostics.Error(CygDiagnosticIds.RecursiveType,
                            $"Type '{name}' contains itself by value. Break the cycle with entity_ref.",
                            span);
                        return;
                    }

                    CygTypeDecl? nested = module.FindType(name);
                    if (nested is CygStructDecl || nested is CygUnionDecl)
                    {
                        CheckCycle(module, name, path, diagnostics);
                    }
                    break;

                case CygTypeKind.FixedArray:
                case CygTypeKind.FixedList:
                    VisitCycle(module, type.Element, span, path, diagnostics);
                    break;

                case CygTypeKind.FixedDictionary:
                    VisitCycle(module, type.Key, span, path, diagnostics);
                    VisitCycle(module, type.Value, span, path, diagnostics);
                    break;
            }
        }

        private static void RequireSize(
            CygModule module,
            List<CygField> fields,
            CygSpan span,
            CygDiagnosticBag diagnostics)
        {
            if (CygLayout.SizeOfFields(module, fields) == null)
            {
                diagnostics.Error(CygDiagnosticIds.UnsupportedType,
                    "The fixed wire size cannot be computed (recursive type, missing type or oversized container).",
                    span);
            }
        }

        private static void ValidateEmittedNames(CygModule module, CygDiagnosticBag diagnostics)
        {
            Dictionary<string, CygSpan> emitted = new Dictionary<string, CygSpan>(StringComparer.Ordinal);
            Dictionary<string, string> support = new Dictionary<string, string>(StringComparer.Ordinal);

            void ClaimEmitted(string name, CygSpan span, string what)
            {
                if (emitted.TryGetValue(name, out CygSpan previous))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Generated type '{name}' for {what} collides with a declaration at {previous}.",
                        span);
                    return;
                }

                emitted.Add(name, span);
            }

            foreach (CygTypeDecl type in module.Types)
            {
                ClaimEmitted(CygNames.SanitizeIdentifier(type.Name), type.Span, "a user type");
                if (type is CygUnionDecl union)
                {
                    foreach (CygUnionVariant variant in union.Variants)
                    {
                        ClaimEmitted(CygNames.SanitizeIdentifier(union.Name + variant.Name), variant.Span,
                            "a union variant");
                    }
                }
            }

            foreach (CygEventDecl eventDecl in module.Events)
            {
                ClaimEmitted(CygNames.SanitizeIdentifier(eventDecl.Name), eventDecl.Span, "an event");
            }

            foreach (CygCommandDecl command in module.Commands)
            {
                ClaimEmitted(CygNames.SanitizeIdentifier(command.Name), command.Span, "a command payload");
                ClaimEmitted(CygNames.SanitizeIdentifier(command.Name + "Command"), command.Span, "a command adapter");
            }

            foreach (CygSignalDecl signal in module.Signals)
            {
                ClaimEmitted(CygNames.SanitizeIdentifier(signal.Name), signal.Span, "a signal");
            }

            foreach (CygAssetDecl asset in module.Assets)
            {
                ClaimEmitted(CygNames.SanitizeIdentifier(asset.Name) + "Ref", asset.Span, "an asset ref");
            }

            if (module.Input != null)
            {
                ClaimEmitted("CYGInput", module.Input.Span, "the input block");
            }

            if (module.Global != null)
            {
                ClaimEmitted("CYGGlobals", module.Global.Span, "the global block");
            }

            void Collect(CygTypeRef? type, CygSpan span)
            {
                if (type == null)
                {
                    return;
                }

                if (type.Kind == CygTypeKind.FixedArray
                    || type.Kind == CygTypeKind.FixedList
                    || type.Kind == CygTypeKind.FixedDictionary
                    || type.Kind == CygTypeKind.FixedString)
                {
                    string name = CygNames.CsTypeName(type);
                    string display = type.Display();
                    if (support.TryGetValue(name, out string? existing))
                    {
                        if (!string.Equals(existing, display, StringComparison.Ordinal))
                        {
                            diagnostics.Error(CygDiagnosticIds.ReservedName,
                                $"Generated support type '{name}' is needed by both '{existing}' and '{display}'.",
                                span);
                        }
                    }
                    else
                    {
                        support.Add(name, display);
                    }
                }

                Collect(type.Element, span);
                Collect(type.Key, span);
                Collect(type.Value, span);
            }

            foreach (CygTypeDecl type in module.Types)
            {
                if (type is CygStructDecl structDecl)
                {
                    foreach (CygField field in structDecl.Fields) Collect(field.Type, field.Span);
                }
                else if (type is CygComponentDecl component)
                {
                    foreach (CygField field in component.Fields) Collect(field.Type, field.Span);
                }
                else if (type is CygUnionDecl union)
                {
                    foreach (CygUnionVariant variant in union.Variants)
                    {
                        foreach (CygField field in variant.Fields) Collect(field.Type, field.Span);
                    }
                }
            }

            if (module.Input != null)
            {
                foreach (CygInputField field in module.Input.Fields) Collect(field.Type, field.Span);
            }

            if (module.Global != null)
            {
                foreach (CygField field in module.Global.Fields) Collect(field.Type, field.Span);
            }

            foreach (CygEventDecl eventDecl in module.Events)
            {
                foreach (CygField field in eventDecl.Fields) Collect(field.Type, field.Span);
            }

            foreach (CygCommandDecl command in module.Commands)
            {
                foreach (CygField field in command.Fields) Collect(field.Type, field.Span);
            }

            foreach (CygSignalDecl signal in module.Signals)
            {
                foreach (CygSignalParam parameter in signal.Parameters) Collect(parameter.Type, parameter.Span);
            }

            foreach (KeyValuePair<string, string> pair in support)
            {
                if (emitted.TryGetValue(pair.Key, out CygSpan previous))
                {
                    diagnostics.Error(CygDiagnosticIds.ReservedName,
                        $"Generated support type '{pair.Key}' for '{pair.Value}' collides with a declaration at {previous}.",
                        previous);
                }
                else
                {
                    emitted.Add(pair.Key, CygSpan.None);
                }
            }
        }

        internal static int DeriveId(string name, int first, int last)
        {
            uint hash = 2166136261u;
            foreach (char c in name)
            {
                hash ^= c;
                hash *= 16777619u;
            }

            long span = (long)last - first + 1;
            return first + (int)(hash % (uint)span);
        }

        private static bool FitsUnderlying(CygPrimitive primitive, long value)
        {
            switch (primitive)
            {
                case CygPrimitive.Byte: return value >= byte.MinValue && value <= byte.MaxValue;
                case CygPrimitive.SByte: return value >= sbyte.MinValue && value <= sbyte.MaxValue;
                case CygPrimitive.Int16: return value >= short.MinValue && value <= short.MaxValue;
                case CygPrimitive.UInt16: return value >= ushort.MinValue && value <= ushort.MaxValue;
                case CygPrimitive.Int32: return value >= int.MinValue && value <= int.MaxValue;
                case CygPrimitive.UInt32: return value >= uint.MinValue && value <= uint.MaxValue;
                case CygPrimitive.Int64: return true;
                case CygPrimitive.UInt64: return value >= 0;
                default: return false;
            }
        }

        private static bool IsIdentifier(string value)
        {
            if (string.IsNullOrEmpty(value))
            {
                return false;
            }

            char first = value[0];
            if (!((first >= 'a' && first <= 'z') || (first >= 'A' && first <= 'Z') || first == '_'))
            {
                return false;
            }

            for (int i = 1; i < value.Length; i++)
            {
                char c = value[i];
                if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_'))
                {
                    return false;
                }
            }

            return true;
        }
    }
}
