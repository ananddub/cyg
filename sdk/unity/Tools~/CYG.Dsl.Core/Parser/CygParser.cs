using System;
using System.Collections.Generic;
using CookYourGame.Dsl;

namespace CookYourGame.Dsl.Core
{
    /// <summary>
    /// Recursive-descent parser. Every declaration is recovered individually so a
    /// single bad field never hides the rest of the file's diagnostics.
    /// </summary>
    internal sealed class CygParser
    {
        private readonly List<CygToken> _tokens;
        private readonly CygDiagnosticBag _diagnostics;
        private readonly string _file;
        private int _index;

        public CygParser(List<CygToken> tokens, string file, CygDiagnosticBag diagnostics)
        {
            _tokens = tokens ?? throw new ArgumentNullException(nameof(tokens));
            _file = file ?? string.Empty;
            _diagnostics = diagnostics ?? throw new ArgumentNullException(nameof(diagnostics));
        }

        private CygToken Current => _tokens[_index];
        private CygToken Previous => _tokens[_index > 0 ? _index - 1 : 0];

        public CygModule Parse()
        {
            CygModule module = new CygModule { SourcePath = _file };
            bool namespaceSeen = false;
            bool anyDeclaration = false;

            while (Current.Kind != CygTokenKind.EndOfFile)
            {
                int startIndex = _index;
                CygToken token = Current;

                if (token.IsIdentifier("namespace"))
                {
                    if (namespaceSeen || anyDeclaration)
                    {
                        Report(CygDiagnosticIds.DuplicateBlock, "The namespace declaration must be the first declaration in the file.", token.Span);
                    }

                    string? ns = ParseNamespace();
                    if (ns != null)
                    {
                        module.Namespace = ns;
                        namespaceSeen = true;
                    }
                }
                else if (token.IsIdentifier("asset"))
                {
                    anyDeclaration = true;
                    CygAssetDecl? asset = ParseAsset();
                    if (asset != null)
                    {
                        module.Assets.Add(asset);
                    }
                }
                else if (token.IsIdentifier("struct"))
                {
                    anyDeclaration = true;
                    CygStructDecl? decl = ParseStruct();
                    if (decl != null)
                    {
                        module.Types.Add(decl);
                    }
                }
                else if (token.IsIdentifier("component"))
                {
                    anyDeclaration = true;
                    CygComponentDecl? decl = ParseComponent();
                    if (decl != null)
                    {
                        module.Types.Add(decl);
                    }
                }
                else if (token.IsIdentifier("enum"))
                {
                    anyDeclaration = true;
                    CygEnumDecl? decl = ParseEnum(false);
                    if (decl != null)
                    {
                        module.Types.Add(decl);
                    }
                }
                else if (token.IsIdentifier("flags"))
                {
                    anyDeclaration = true;
                    CygEnumDecl? decl = ParseEnum(true);
                    if (decl != null)
                    {
                        module.Types.Add(decl);
                    }
                }
                else if (token.IsIdentifier("union"))
                {
                    anyDeclaration = true;
                    CygUnionDecl? decl = ParseUnion();
                    if (decl != null)
                    {
                        module.Types.Add(decl);
                    }
                }
                else if (token.IsIdentifier("input"))
                {
                    anyDeclaration = true;
                    CygInputDecl? decl = ParseInput();
                    if (decl != null)
                    {
                        if (module.Input != null)
                        {
                            Report(CygDiagnosticIds.DuplicateBlock, "Only one input block is allowed per module.", token.Span);
                        }
                        else
                        {
                            module.Input = decl;
                        }
                    }
                }
                else if (token.IsIdentifier("global"))
                {
                    anyDeclaration = true;
                    CygGlobalDecl? decl = ParseGlobal();
                    if (decl != null)
                    {
                        if (module.Global != null)
                        {
                            Report(CygDiagnosticIds.DuplicateBlock, "Only one global block is allowed per module.", token.Span);
                        }
                        else
                        {
                            module.Global = decl;
                        }
                    }
                }
                else if (token.IsIdentifier("schema"))
                {
                    anyDeclaration = true;
                    CygSchemaDecl? decl = ParseSchema();
                    if (decl != null)
                    {
                        module.Schemas.Add(decl);
                    }
                }
                else if (token.IsIdentifier("event") || token.IsIdentifier("synced_event"))
                {
                    anyDeclaration = true;
                    CygEventDecl? decl = ParseEvent(token.IsIdentifier("synced_event"));
                    if (decl != null)
                    {
                        module.Events.Add(decl);
                    }
                }
                else if (token.IsIdentifier("signal"))
                {
                    anyDeclaration = true;
                    CygSignalDecl? decl = ParseSignal();
                    if (decl != null)
                    {
                        module.Signals.Add(decl);
                    }
                }
                else if (token.IsIdentifier("command"))
                {
                    anyDeclaration = true;
                    CygCommandDecl? decl = ParseCommand();
                    if (decl != null)
                    {
                        module.Commands.Add(decl);
                    }
                }
                else if (token.IsIdentifier("rpc"))
                {
                    anyDeclaration = true;
                    Report(CygDiagnosticIds.UnsupportedType,
                        "'rpc' declarations are reserved for a later CYG release; use command, signal, event or synced_event.",
                        token.Span);
                    SkipDeclaration();
                }
                else
                {
                    Report(CygDiagnosticIds.Parser, $"Unexpected token '{token}' at the top level.", token.Span);
                    Next();
                }

                if (_index == startIndex)
                {
                    Next();
                }
            }

            return module;
        }

        private string? ParseNamespace()
        {
            CygToken keyword = Next();
            string name = ExpectIdentifier("a namespace name") ?? string.Empty;
            while (Match(CygTokenKind.Dot))
            {
                string? part = ExpectIdentifier("a namespace segment");
                if (part == null)
                {
                    break;
                }

                name += "." + part;
            }

            Expect(CygTokenKind.Semicolon, "';' after the namespace declaration");
            if (name.Length == 0)
            {
                Report(CygDiagnosticIds.Parser, "A namespace declaration needs a name.", keyword.Span);
                return null;
            }

            return name;
        }

        private CygAssetDecl? ParseAsset()
        {
            CygToken keyword = Next();
            string? name = ExpectIdentifier("an asset name");
            Expect(CygTokenKind.Semicolon, "';' after the asset declaration");
            if (name == null)
            {
                return null;
            }

            return new CygAssetDecl { Name = name, Span = keyword.Span };
        }

        private CygStructDecl? ParseStruct()
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, "struct"))
            {
                return null;
            }

            CygStructDecl decl = new CygStructDecl { Name = name!, Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygComponentDecl? ParseComponent()
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, "component"))
            {
                return null;
            }

            int? id = null;
            if (Match(CygTokenKind.Colon))
            {
                CygToken number = Current;
                if (number.Kind == CygTokenKind.Number)
                {
                    Next();
                    if (number.Number > int.MaxValue || number.Number < int.MinValue)
                    {
                        Report(CygDiagnosticIds.ComponentId, $"Component id {number.Number} does not fit in a 32-bit integer.", number.Span);
                    }
                    else
                    {
                        id = (int)number.Number;
                    }
                }
                else
                {
                    Report(CygDiagnosticIds.ComponentId, "Expected a numeric component id after ':'.", number.Span);
                    SkipToBlockStart();
                }
            }

            CygComponentDecl decl = new CygComponentDecl { Name = name!, Id = id, Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygSchemaDecl? ParseSchema()
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, "schema"))
            {
                return null;
            }

            uint version = 1;
            if (Match(CygTokenKind.Colon))
            {
                CygToken number = Current;
                if (number.Kind == CygTokenKind.Number)
                {
                    Next();
                    if (number.Number <= 0 || number.Number > uint.MaxValue)
                    {
                        Report(CygDiagnosticIds.Schema,
                            $"Schema version {number.Number} must be between 1 and {uint.MaxValue}.",
                            number.Span);
                    }
                    else
                    {
                        version = (uint)number.Number;
                    }
                }
                else
                {
                    Report(CygDiagnosticIds.Schema, "Expected a numeric schema version after ':'.", number.Span);
                    SkipToBlockStart();
                }
            }

            CygSchemaDecl decl = new CygSchemaDecl { Name = name!, Version = version, Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygEnumDecl? ParseEnum(bool flags)
        {
            CygToken keyword = Next();
            string? name = ExpectIdentifier(flags ? "a flags name" : "an enum name");
            if (name == null)
            {
                SkipDeclaration();
                return null;
            }

            CygEnumDecl decl = new CygEnumDecl { Name = name, IsFlags = flags, Span = keyword.Span };
            if (Match(CygTokenKind.Colon))
            {
                CygToken underlying = Current;
                if (underlying.Kind == CygTokenKind.Identifier)
                {
                    CygPrimitive? primitive = TryParsePrimitive(underlying.Text);
                    if (primitive.HasValue)
                    {
                        decl.Underlying = primitive.Value;
                        decl.UnderlyingExplicit = true;
                        Next();
                    }
                    else
                    {
                        Report(CygDiagnosticIds.Flags,
                            $"'{underlying.Text}' is not a valid enum underlying type. Use Byte, SByte, Int16, UInt16, Int32, UInt32, Int64 or UInt64.",
                            underlying.Span);
                        Next();
                    }
                }
                else
                {
                    Report(CygDiagnosticIds.Flags, "Expected an integer underlying type after ':'.", underlying.Span);
                }
            }

            if (!Expect(CygTokenKind.LBrace, "'{' before enum members"))
            {
                SkipDeclaration();
                return null;
            }

            while (Current.Kind != CygTokenKind.RBrace && Current.Kind != CygTokenKind.EndOfFile)
            {
                CygToken memberToken = Current;
                string? memberName = ExpectIdentifier("an enum member name");
                if (memberName == null)
                {
                    SkipToFieldEnd();
                    continue;
                }

                long? value = null;
                if (Match(CygTokenKind.Equals))
                {
                    value = ParseSignedNumber();
                }

                decl.Members.Add(new CygEnumMember
                {
                    Name = memberName,
                    Value = value,
                    Span = memberToken.Span
                });

                if (!Match(CygTokenKind.Comma))
                {
                    break;
                }
            }

            Expect(CygTokenKind.RBrace, "'}' after enum members");
            return decl;
        }

        private CygUnionDecl? ParseUnion()
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, "union"))
            {
                return null;
            }

            CygUnionDecl decl = new CygUnionDecl { Name = name!, Span = keyword.Span };
            Expect(CygTokenKind.LBrace, "'{' before union variants");
            while (Current.Kind != CygTokenKind.RBrace && Current.Kind != CygTokenKind.EndOfFile)
            {
                CygToken variantToken = Current;
                if (!variantToken.IsIdentifier("struct"))
                {
                    Report(CygDiagnosticIds.Union,
                        "Union variants must be declared as 'struct Name { ... }' blocks.",
                        variantToken.Span);
                    SkipToFieldEnd();
                    continue;
                }

                Next();
                string? variantName = ExpectIdentifier("a union variant name");
                if (variantName == null)
                {
                    SkipToFieldEnd();
                    continue;
                }

                CygUnionVariant variant = new CygUnionVariant
                {
                    Name = variantName,
                    Span = variantToken.Span
                };
                ReadFields(variant.Fields);
                decl.Variants.Add(variant);
            }

            Expect(CygTokenKind.RBrace, "'}' after union variants");
            return decl;
        }

        private CygInputDecl? ParseInput()
        {
            CygToken keyword = Next();
            CygInputDecl decl = new CygInputDecl { Span = keyword.Span };
            Expect(CygTokenKind.LBrace, "'{' before input fields");
            while (Current.Kind != CygTokenKind.RBrace && Current.Kind != CygTokenKind.EndOfFile)
            {
                CygToken start = Current;
                bool button = start.IsIdentifier("button");
                if (button)
                {
                    Next();
                    string? buttonName = ExpectIdentifier("a button name");
                    Expect(CygTokenKind.Semicolon, "';' after the button");
                    if (buttonName != null)
                    {
                        decl.Fields.Add(new CygInputField
                        {
                            Name = buttonName,
                            Type = null,
                            IsButton = true,
                            Span = start.Span
                        });
                    }
                    continue;
                }

                CygTypeRef? type = ParseType();
                if (type == null)
                {
                    SkipToFieldEnd();
                    continue;
                }

                string? name = ExpectIdentifier("an input field name");
                Expect(CygTokenKind.Semicolon, "';' after the input field");
                if (name != null)
                {
                    decl.Fields.Add(new CygInputField
                    {
                        Name = name,
                        Type = type,
                        IsButton = false,
                        Span = start.Span
                    });
                }
            }

            Expect(CygTokenKind.RBrace, "'}' after input fields");
            return decl;
        }

        private CygGlobalDecl? ParseGlobal()
        {
            CygToken keyword = Next();
            CygGlobalDecl decl = new CygGlobalDecl { Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygEventDecl? ParseEvent(bool synced)
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, synced ? "synced_event" : "event"))
            {
                return null;
            }

            int? id = null;
            if (Match(CygTokenKind.Colon))
            {
                CygToken number = Current;
                if (number.Kind == CygTokenKind.Number)
                {
                    Next();
                    if (number.Number > int.MaxValue || number.Number < int.MinValue)
                    {
                        Report(CygDiagnosticIds.EventId, $"Event id {number.Number} does not fit in a 32-bit integer.", number.Span);
                    }
                    else
                    {
                        id = (int)number.Number;
                    }
                }
                else
                {
                    Report(CygDiagnosticIds.EventId, "Expected a numeric event id after ':'.", number.Span);
                    SkipToBlockStart();
                }
            }

            CygEventDecl decl = new CygEventDecl { Name = name!, Id = id, Synced = synced, Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygCommandDecl? ParseCommand()
        {
            CygToken keyword = Next();
            if (!TryReadDeclarationHeader(out string? name, keyword, "command"))
            {
                return null;
            }

            int? id = null;
            if (Match(CygTokenKind.Colon))
            {
                CygToken number = Current;
                if (number.Kind == CygTokenKind.Number)
                {
                    Next();
                    if (number.Number > int.MaxValue || number.Number < int.MinValue)
                    {
                        Report(CygDiagnosticIds.CommandId, $"Command id {number.Number} does not fit in a 32-bit integer.", number.Span);
                    }
                    else
                    {
                        id = (int)number.Number;
                    }
                }
                else
                {
                    Report(CygDiagnosticIds.CommandId, "Expected a numeric command id after ':'.", number.Span);
                    SkipToBlockStart();
                }
            }

            CygCommandDecl decl = new CygCommandDecl { Name = name!, Id = id, Span = keyword.Span };
            ReadFields(decl.Fields);
            return decl;
        }

        private CygSignalDecl? ParseSignal()
        {
            CygToken keyword = Next();
            string? name = ExpectIdentifier("a signal name");
            if (name == null)
            {
                SkipToFieldEnd();
                return null;
            }

            CygSignalDecl decl = new CygSignalDecl { Name = name, Span = keyword.Span };
            if (!Expect(CygTokenKind.LParen, "'(' before signal parameters"))
            {
                SkipToFieldEnd();
                return null;
            }

            if (Current.Kind != CygTokenKind.RParen)
            {
                while (Current.Kind != CygTokenKind.RParen && Current.Kind != CygTokenKind.EndOfFile)
                {
                    CygToken parameterToken = Current;
                    CygTypeRef? type = ParseType();
                    if (type == null)
                    {
                        SkipToParameterEnd();
                        if (Match(CygTokenKind.Comma))
                        {
                            continue;
                        }

                        break;
                    }

                    string? parameterName = ExpectIdentifier("a signal parameter name");
                    if (parameterName != null)
                    {
                        decl.Parameters.Add(new CygSignalParam
                        {
                            Name = parameterName,
                            Type = type,
                            Span = parameterToken.Span
                        });
                    }

                    if (!Match(CygTokenKind.Comma))
                    {
                        break;
                    }
                }
            }

            Expect(CygTokenKind.RParen, "')' after signal parameters");
            Expect(CygTokenKind.Semicolon, "';' after the signal declaration");
            return decl;
        }

        private void ReadFields(List<CygField> fields)
        {
            if (!Expect(CygTokenKind.LBrace, "'{' before fields"))
            {
                return;
            }

            while (Current.Kind != CygTokenKind.RBrace && Current.Kind != CygTokenKind.EndOfFile)
            {
                CygToken start = Current;
                CygTypeRef? type = ParseType();
                if (type == null)
                {
                    SkipToFieldEnd();
                    continue;
                }

                string? name = ExpectIdentifier("a field name");
                if (!Expect(CygTokenKind.Semicolon, "';' after the field"))
                {
                    SkipToFieldEnd();
                }

                if (name != null)
                {
                    fields.Add(new CygField { Name = name, Type = type, Span = start.Span });
                }
            }

            Expect(CygTokenKind.RBrace, "'}' after fields");
        }

        private CygTypeRef? ParseType()
        {
            CygToken token = Current;
            if (token.Kind != CygTokenKind.Identifier)
            {
                Report(CygDiagnosticIds.Parser, $"Expected a type name, found '{token}'.", token.Span);
                return null;
            }

            if (IsFloatName(token.Text))
            {
                Report(CygDiagnosticIds.Float,
                    $"'{token.Text}' is not allowed in simulation state. Use FP for deterministic fixed-point math.",
                    token.Span);
                Next();
                SkipTypeTail();
                return CygTypeRef.Builtin(CygTypeKind.FP);
            }

            if (IsReferenceTypeName(token.Text))
            {
                Report(CygDiagnosticIds.ReferenceType,
                    $"'{token.Text}' is a managed reference type and cannot be part of a deterministic wire value.",
                    token.Span);
                Next();
                SkipTypeTail();
                return CygTypeRef.Builtin(CygTypeKind.Int32);
            }

            if (IsStandardCollectionName(token.Text))
            {
                Report(CygDiagnosticIds.StandardCollection,
                    $"'{token.Text}' uses the managed collection hierarchy. Use array<T>[N], list<T>[N] or dictionary<K,V>[N].",
                    token.Span);
                Next();
                SkipTypeTail();
                return CygTypeRef.Builtin(CygTypeKind.Int32);
            }

            if (IsUnityTypeName(token.Text))
            {
                Report(CygDiagnosticIds.UnityType,
                    $"'{token.Text}' is a UnityEngine type. Use FPVector2, FPVector3, FPQuaternion, FPMatrix4x4, FPAABB or FPBounds.",
                    token.Span);
                Next();
                SkipTypeTail();
                return CygTypeRef.Builtin(CygTypeKind.FPVector3);
            }

            if (token.IsIdentifier("asset_ref"))
            {
                Next();
                if (!Expect(CygTokenKind.LAngle, "'<' after asset_ref"))
                {
                    SkipTypeTail();
                    return null;
                }

                string? asset = ExpectIdentifier("an asset name");
                Expect(CygTokenKind.RAngle, "'>' after the asset name");
                if (asset == null)
                {
                    return null;
                }

                return CygTypeRef.AssetOf(asset);
            }

            if (token.IsIdentifier("QString"))
            {
                Next();
                if (!Expect(CygTokenKind.LAngle, "'<' after QString"))
                {
                    SkipTypeTail();
                    return CygTypeRef.StringOf(0);
                }

                CygToken lengthToken = Current;
                if (lengthToken.Kind != CygTokenKind.Number)
                {
                    Report(CygDiagnosticIds.StringLength, "QString requires a positive capacity, for example QString<32>.", lengthToken.Span);
                    SkipTypeTail();
                    return CygTypeRef.StringOf(0);
                }

                Next();
                Expect(CygTokenKind.RAngle, "'>' after the QString capacity");
                int length = ToLength(lengthToken);
                return CygTypeRef.StringOf(length);
            }

            if (token.IsIdentifier("array") || token.IsIdentifier("list") || token.IsIdentifier("dictionary"))
            {
                return ParseContainerType(token);
            }

            Next();
            CygPrimitive? primitive = TryParsePrimitive(token.Text);
            if (primitive.HasValue)
            {
                return CygTypeRef.Builtin(ToKind(primitive.Value));
            }

            CygTypeKind? builtin = TryParseBuiltin(token.Text);
            if (builtin.HasValue)
            {
                return CygTypeRef.Builtin(builtin.Value);
            }

            string name = token.Text;
            bool qualified = false;
            while (Current.Kind == CygTokenKind.Dot)
            {
                qualified = true;
                Next();
                string? segment = ExpectIdentifier("a qualified type name segment");
                if (segment == null)
                {
                    break;
                }
                name += "." + segment;
            }

            if (qualified)
            {
                Report(CygDiagnosticIds.UnityType,
                    $"Qualified type '{name}' is not supported. Reference a CYG type declared in the module.",
                    token.Span);
                SkipTypeTail();
                return CygTypeRef.Builtin(CygTypeKind.Int32);
            }

            if (Current.Kind == CygTokenKind.LAngle)
            {
                Report(CygDiagnosticIds.UnsupportedType,
                    $"Generic type '{token.Text}<...>' is not supported.",
                    token.Span);
                SkipTypeTail();
                return null;
            }

            return CygTypeRef.NamedType(name);
        }

        private CygTypeRef? ParseContainerType(CygToken token)
        {
            string container = token.Text;
            Next();
            if (!Expect(CygTokenKind.LAngle, $"'<' after {container}"))
            {
                SkipTypeTail();
                return null;
            }

            CygTypeRef? first = ParseType();
            if (first == null)
            {
                SkipTypeTail();
                return null;
            }

            CygTypeRef? second = null;
            if (container == "dictionary")
            {
                if (!Expect(CygTokenKind.Comma, "',' between dictionary key and value types"))
                {
                    SkipTypeTail();
                    return null;
                }

                second = ParseType();
                if (second == null)
                {
                    SkipTypeTail();
                    return null;
                }
            }

            Expect(CygTokenKind.RAngle, $"'>' after {container} element types");
            int length = 0;
            if (!Match(CygTokenKind.LBracket))
            {
                Report(CygDiagnosticIds.ArrayLength,
                    $"{container}<...> requires a fixed capacity, for example {container}<...>[16].",
                    token.Span);
                return null;
            }

            CygToken lengthToken = Current;
            if (lengthToken.Kind != CygTokenKind.Number)
            {
                Report(CygDiagnosticIds.ArrayLength, "Expected a numeric capacity inside '[...]'.", lengthToken.Span);
            }
            else
            {
                Next();
                length = ToLength(lengthToken);
            }

            Expect(CygTokenKind.RBracket, "']' after the capacity");

            if (container == "array")
            {
                return CygTypeRef.ArrayOf(first, length);
            }
            if (container == "list")
            {
                return CygTypeRef.ListOf(first, length);
            }
            return CygTypeRef.DictionaryOf(first, second!, length);
        }

        private long? ParseSignedNumber()
        {
            bool negative = Match(CygTokenKind.Minus);
            CygToken token = Current;
            if (token.Kind != CygTokenKind.Number)
            {
                Report(CygDiagnosticIds.Parser, "Expected an integer value.", token.Span);
                return null;
            }

            Next();
            long value = negative ? -token.Number : token.Number;
            if (negative && token.Number == long.MinValue)
            {
                Report(CygDiagnosticIds.Parser, "Integer value is out of range.", token.Span);
                return null;
            }

            return value;
        }

        private bool TryReadDeclarationHeader(out string? name, CygToken keyword, string what)
        {
            name = ExpectIdentifier($"a {what} name");
            if (name == null)
            {
                SkipDeclaration();
                return false;
            }

            return true;
        }

        private void SkipToBlockStart()
        {
            while (Current.Kind != CygTokenKind.EndOfFile
                && Current.Kind != CygTokenKind.LBrace
                && Current.Kind != CygTokenKind.RBrace)
            {
                Next();
            }
        }

        private CygToken Next()
        {
            CygToken token = Current;
            if (_index < _tokens.Count - 1)
            {
                _index++;
            }
            return token;
        }

        private bool Match(CygTokenKind kind)
        {
            if (Current.Kind != kind)
            {
                return false;
            }

            Next();
            return true;
        }

        private bool Expect(CygTokenKind kind, string what)
        {
            if (Current.Kind == kind)
            {
                Next();
                return true;
            }

            Report(CygDiagnosticIds.Parser, $"Expected {what}, found '{Current}'.", Current.Span);
            return false;
        }

        private string? ExpectIdentifier(string what)
        {
            if (Current.Kind != CygTokenKind.Identifier)
            {
                Report(CygDiagnosticIds.Parser, $"Expected {what}, found '{Current}'.", Current.Span);
                return null;
            }

            string text = Current.Text;
            Next();
            return text;
        }

        private void SkipTypeTail()
        {
            if (Current.Kind == CygTokenKind.LAngle)
            {
                int depth = 0;
                do
                {
                    if (Current.Kind == CygTokenKind.LAngle)
                    {
                        depth++;
                    }
                    else if (Current.Kind == CygTokenKind.RAngle)
                    {
                        depth--;
                    }
                    else if (Current.Kind == CygTokenKind.EndOfFile)
                    {
                        return;
                    }

                    Next();
                }
                while (depth > 0);
            }

            if (Current.Kind == CygTokenKind.LBracket)
            {
                while (Current.Kind != CygTokenKind.RBracket && Current.Kind != CygTokenKind.EndOfFile)
                {
                    Next();
                }

                Match(CygTokenKind.RBracket);
            }
        }

        private void SkipToFieldEnd()
        {
            while (Current.Kind != CygTokenKind.EndOfFile)
            {
                if (Current.Kind == CygTokenKind.Semicolon)
                {
                    Next();
                    return;
                }

                if (Current.Kind == CygTokenKind.RBrace)
                {
                    return;
                }

                Next();
            }
        }

        private void SkipToParameterEnd()
        {
            while (Current.Kind != CygTokenKind.EndOfFile)
            {
                if (Current.Kind == CygTokenKind.Comma)
                {
                    return;
                }

                if (Current.Kind == CygTokenKind.RParen)
                {
                    return;
                }

                Next();
            }
        }

        private void SkipDeclaration()
        {
            int braces = 0;
            while (Current.Kind != CygTokenKind.EndOfFile)
            {
                if (Current.Kind == CygTokenKind.LBrace)
                {
                    braces++;
                }
                else if (Current.Kind == CygTokenKind.RBrace)
                {
                    if (braces == 0)
                    {
                        Next();
                        return;
                    }

                    braces--;
                    if (braces == 0)
                    {
                        Next();
                        Match(CygTokenKind.Semicolon);
                        return;
                    }
                }
                else if (Current.Kind == CygTokenKind.Semicolon && braces == 0)
                {
                    Next();
                    return;
                }

                Next();
            }
        }

        private int ToLength(CygToken token)
        {
            if (token.Number > int.MaxValue)
            {
                Report(CygDiagnosticIds.ArrayLength, $"Capacity {token.Number} is too large.", token.Span);
                return 0;
            }

            return (int)token.Number;
        }

        private void Report(string id, string message, CygSpan span)
        {
            _diagnostics.Error(id, message, span);
        }

        private static bool IsFloatName(string name)
            => name == "float" || name == "double" || name == "Single" || name == "Double"
                || name == "decimal" || name == "Decimal";

        private static bool IsReferenceTypeName(string name)
            => name == "string" || name == "String" || name == "object" || name == "Object"
                || name == "class" || name == "interface" || name == "delegate" || name == "dynamic";

        private static bool IsStandardCollectionName(string name)
            => name == "List" || name == "Dictionary" || name == "HashSet" || name == "Queue"
                || name == "Stack" || name == "Array" || name == "IList" || name == "IDictionary"
                || name == "ICollection" || name == "IEnumerable";

        private static bool IsUnityTypeName(string name)
            => name == "Vector2" || name == "Vector3" || name == "Vector4" || name == "Quaternion"
                || name == "Transform" || name == "GameObject" || name == "Color" || name == "Rect"
                || name == "Matrix4x4" || name == "Rigidbody" || name == "MonoBehaviour";

        private static CygPrimitive? TryParsePrimitive(string name)
        {
            switch (name)
            {
                case "Byte":
                case "byte":
                    return CygPrimitive.Byte;
                case "SByte":
                case "sbyte":
                    return CygPrimitive.SByte;
                case "Int16":
                case "short":
                    return CygPrimitive.Int16;
                case "UInt16":
                case "ushort":
                    return CygPrimitive.UInt16;
                case "Int32":
                case "int":
                    return CygPrimitive.Int32;
                case "UInt32":
                case "uint":
                    return CygPrimitive.UInt32;
                case "Int64":
                case "long":
                    return CygPrimitive.Int64;
                case "UInt64":
                case "ulong":
                    return CygPrimitive.UInt64;
                default:
                    return null;
            }
        }

        private static CygTypeKind? TryParseBuiltin(string name)
        {
            switch (name)
            {
                case "FP": return CygTypeKind.FP;
                case "Boolean":
                case "bool": return CygTypeKind.Boolean;
                case "FPVector2": return CygTypeKind.FPVector2;
                case "FPVector3": return CygTypeKind.FPVector3;
                case "FPQuaternion": return CygTypeKind.FPQuaternion;
                case "FPMatrix4x4": return CygTypeKind.FPMatrix4x4;
                case "FPAABB": return CygTypeKind.FPAABB;
                case "FPBounds": return CygTypeKind.FPBounds;
                case "entity_ref": return CygTypeKind.EntityRef;
                case "player_ref": return CygTypeKind.PlayerRef;
                default: return null;
            }
        }

        private static CygTypeKind ToKind(CygPrimitive primitive)
        {
            switch (primitive)
            {
                case CygPrimitive.Byte: return CygTypeKind.Byte;
                case CygPrimitive.SByte: return CygTypeKind.SByte;
                case CygPrimitive.Int16: return CygTypeKind.Int16;
                case CygPrimitive.UInt16: return CygTypeKind.UInt16;
                case CygPrimitive.Int32: return CygTypeKind.Int32;
                case CygPrimitive.UInt32: return CygTypeKind.UInt32;
                case CygPrimitive.Int64: return CygTypeKind.Int64;
                case CygPrimitive.UInt64: return CygTypeKind.UInt64;
                default: return CygTypeKind.Int32;
            }
        }
    }
}
