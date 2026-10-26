using Microsoft.CodeAnalysis;

namespace CookYourGame.CodeGen
{
    internal static class CYGDiagnostics
    {
        private const string Category = "CookYourGame.CodeGen";

        public static readonly DiagnosticDescriptor MustBePartial = new DiagnosticDescriptor(
            "CYG001",
            "CYG component must be partial",
            "Struct '{0}' is marked [CYGComponent] but is not declared partial",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor UnsupportedField = new DiagnosticDescriptor(
            "CYG002",
            "Unsupported CYG component field",
            "Field '{0}' has unsupported type '{1}'. Use fixed-point values or an ICYGValue struct",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor MissingCodec = new DiagnosticDescriptor(
            "CYG003",
            "Nested value has no wire codec",
            "Type '{0}' used by field '{1}' must expose public const int SizeInBytes and WriteTo/ReadFrom/ComputeChecksum",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor DuplicateTypeId = new DiagnosticDescriptor(
            "CYG004",
            "Duplicate CYG component id",
            "Component id {0} is already used by '{1}'",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor ReadonlyField = new DiagnosticDescriptor(
            "CYG005",
            "Readonly field cannot be deserialized",
            "Field '{0}' is readonly; generated ReadFrom cannot assign it",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor NestedTypeUnsupported = new DiagnosticDescriptor(
            "CYG006",
            "Nested CYG components are not supported yet",
            "Struct '{0}' is nested; move it to a top-level type before using [CYGComponent]",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor GenericUnsupported = new DiagnosticDescriptor(
            "CYG007",
            "Generic CYG components are not supported",
            "Struct '{0}' has type parameters; use a concrete component type",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor MemberConflict = new DiagnosticDescriptor(
            "CYG008",
            "Generated member name conflict",
            "Struct '{0}' already declares '{1}', which the generator needs to emit",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor UnityTypeUnsupported = new DiagnosticDescriptor(
            "CYG009",
            "Unity runtime types are not deterministic wire values",
            "Field '{0}' uses '{1}'. Use FP/FPVector/FPQuaternion for simulation state",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor FloatField = new DiagnosticDescriptor(
            "CYG010",
            "Floating-point field in deterministic component",
            "Field '{0}' is {1}; prefer FP for lockstep simulation. The generator will serialize the IEEE bits",
            Category,
            DiagnosticSeverity.Warning,
            true);

        public static readonly DiagnosticDescriptor ReadonlyStructUnsupported = new DiagnosticDescriptor(
            "CYG011",
            "Readonly structs cannot implement ReadFrom",
            "Struct '{0}' must be mutable so generated ReadFrom can assign its fields",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor NoFields = new DiagnosticDescriptor(
            "CYG012",
            "CYG component has no serializable fields",
            "Struct '{0}' has no instance fields; remove [CYGComponent] or add state fields",
            Category,
            DiagnosticSeverity.Warning,
            true);

        public static readonly DiagnosticDescriptor ExplicitLayoutUnsupported = new DiagnosticDescriptor(
            "CYG013",
            "Explicit layout cannot be serialized deterministically",
            "Struct '{0}' uses LayoutKind.Explicit; use sequential field order and let the generator emit the wire layout",
            Category,
            DiagnosticSeverity.Error,
            true);

        public static readonly DiagnosticDescriptor RefStructUnsupported = new DiagnosticDescriptor(
            "CYG014",
            "Ref structs cannot be wire components",
            "Struct '{0}' is a ref struct and cannot implement ICYGComponent",
            Category,
            DiagnosticSeverity.Error,
            true);
    }
}
