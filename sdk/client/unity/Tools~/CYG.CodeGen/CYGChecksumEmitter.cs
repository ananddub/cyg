using System.Collections.Generic;
using System.Text;
using Microsoft.CodeAnalysis;

namespace CookYourGame.CodeGen
{
    public sealed partial class CYGComponentGenerator
    {
        private static void EmitChecksum(StringBuilder sb, string indent, List<FieldSpec> fields, bool isComponent)
        {
            sb.Append(indent).AppendLine("public readonly void ComputeChecksum(ref uint crc)");
            sb.Append(indent).AppendLine("{");
            if (isComponent)
            {
                sb.Append(indent).AppendLine("    global::CookYourGame.Sim.CYGChecksum.HashInt(ref crc, TypeId);");
                sb.Append(indent).AppendLine("    global::CookYourGame.Sim.CYGChecksum.HashInt(ref crc, unchecked((int)SchemaHash));");
            }
            for (int i = 0; i < fields.Count; i++)
            {
                EmitChecksumValue(sb, indent + "    ", fields[i]);
            }
            sb.Append(indent).AppendLine("}");
        }

        private static void EmitChecksumValue(StringBuilder sb, string indent, FieldSpec field)
        {
            string name = EscapeIdentifier(field.Symbol.Name);
            string prefix = indent + "global::CookYourGame.Sim.CYGChecksum.";
            switch (field.Kind)
            {
                case FieldKind.Bool:
                    sb.Append(prefix).Append("HashByte(ref crc, ").Append(name).AppendLine(" ? (byte)1 : (byte)0);");
                    break;
                case FieldKind.Byte:
                    sb.Append(prefix).Append("HashByte(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.SByte:
                    sb.Append(prefix).Append("HashByte(ref crc, unchecked((byte)").Append(name).AppendLine("));");
                    break;
                case FieldKind.Int16:
                    sb.Append(prefix).Append("HashInt16(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt16:
                    sb.Append(prefix).Append("HashUInt16(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Int32:
                    sb.Append(prefix).Append("HashInt(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt32:
                    sb.Append(prefix).Append("HashUInt(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Int64:
                    sb.Append(prefix).Append("HashLong(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt64:
                    sb.Append(prefix).Append("HashULong(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Char:
                    sb.Append(prefix).Append("HashUInt16(ref crc, (ushort)").Append(name).AppendLine(");");
                    break;
                case FieldKind.Single:
                    sb.Append(prefix).Append("HashSingle(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Double:
                    sb.Append(prefix).Append("HashDouble(ref crc, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Enum:
                    if (field.Underlying.SpecialType == SpecialType.System_SByte)
                    {
                        sb.Append(prefix).Append("HashByte(ref crc, unchecked((byte)(sbyte)").Append(name).AppendLine("));");
                    }
                    else
                    {
                        sb.Append(prefix).Append(ChecksumName(field.Underlying.SpecialType)).Append("(ref crc, ")
                            .Append(EnumCast(field.Underlying)).Append(name).AppendLine(");");
                    }
                    break;
                case FieldKind.Struct:
                    sb.Append(indent).Append(name).AppendLine(".ComputeChecksum(ref crc);");
                    break;
            }
        }

        private static string ChecksumName(SpecialType type)
        {
            switch (type)
            {
                case SpecialType.System_Byte: return "HashByte";
                case SpecialType.System_SByte: return "HashByte";
                case SpecialType.System_Int16: return "HashInt16";
                case SpecialType.System_UInt16: return "HashUInt16";
                case SpecialType.System_Int32: return "HashInt";
                case SpecialType.System_UInt32: return "HashUInt";
                case SpecialType.System_Int64: return "HashLong";
                case SpecialType.System_UInt64: return "HashULong";
                default: return "HashInt";
            }
        }
    }
}
