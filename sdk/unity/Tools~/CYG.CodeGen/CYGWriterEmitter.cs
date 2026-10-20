using System.Collections.Generic;
using System.Text;
using Microsoft.CodeAnalysis;

namespace CookYourGame.CodeGen
{
    public sealed partial class CYGComponentGenerator
    {
        private static void EmitWrite(StringBuilder sb, string indent, List<FieldSpec> fields)
        {
            sb.Append(indent).AppendLine("public readonly int WriteTo(byte[] buffer, int offset)");
            sb.Append(indent).AppendLine("{");
            sb.Append(indent).AppendLine("    global::CookYourGame.Sim.CYGWire.RequireWritable(buffer, offset, SizeInBytes);");
            for (int i = 0; i < fields.Count; i++)
            {
                FieldSpec field = fields[i];
                EmitWriteValue(sb, indent + "    ", field);
                sb.Append(indent).Append("    offset += ").Append(field.Size).AppendLine(";");
            }
            sb.Append(indent).AppendLine("    return SizeInBytes;");
            sb.Append(indent).AppendLine("}");
        }

        private static void EmitWriteValue(StringBuilder sb, string indent, FieldSpec field)
        {
            string name = EscapeIdentifier(field.Symbol.Name);
            switch (field.Kind)
            {
                case FieldKind.Bool:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteByte(buffer, offset, ").Append(name).AppendLine(" ? (byte)1 : (byte)0);");
                    break;
                case FieldKind.Byte:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteByte(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.SByte:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteSByte(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Int16:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteInt16(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt16:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteUInt16(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Int32:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteInt32(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt32:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteUInt32(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Int64:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteInt64(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.UInt64:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteUInt64(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Char:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteUInt16(buffer, offset, (ushort)").Append(name).AppendLine(");");
                    break;
                case FieldKind.Single:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteSingle(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Double:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.WriteDouble(buffer, offset, ").Append(name).AppendLine(");");
                    break;
                case FieldKind.Enum:
                    sb.Append(indent).Append("global::CookYourGame.Sim.CYGWire.").Append(WriterName(field.Underlying.SpecialType))
                        .Append("(buffer, offset, ").Append(EnumCast(field.Underlying)).Append(name).AppendLine(");");
                    break;
                case FieldKind.Struct:
                    sb.Append(indent).Append(name).AppendLine(".WriteTo(buffer, offset);");
                    break;
            }
        }

        private static void EmitRead(StringBuilder sb, string indent, List<FieldSpec> fields)
        {
            sb.Append(indent).AppendLine("public void ReadFrom(byte[] buffer, int offset)");
            sb.Append(indent).AppendLine("{");
            sb.Append(indent).AppendLine("    global::CookYourGame.Sim.CYGWire.RequireReadable(buffer, offset, SizeInBytes);");
            for (int i = 0; i < fields.Count; i++)
            {
                FieldSpec field = fields[i];
                EmitReadValue(sb, indent + "    ", field);
                sb.Append(indent).Append("    offset += ").Append(field.Size).AppendLine(";");
            }
            sb.Append(indent).AppendLine("}");
        }

        private static void EmitReadValue(StringBuilder sb, string indent, FieldSpec field)
        {
            string name = EscapeIdentifier(field.Symbol.Name);
            string wire = "global::CookYourGame.Sim.CYGWire.";
            switch (field.Kind)
            {
                case FieldKind.Bool:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadByte(buffer, offset) != 0;");
                    break;
                case FieldKind.Byte:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadByte(buffer, offset);");
                    break;
                case FieldKind.SByte:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadSByte(buffer, offset);");
                    break;
                case FieldKind.Int16:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadInt16(buffer, offset);");
                    break;
                case FieldKind.UInt16:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadUInt16(buffer, offset);");
                    break;
                case FieldKind.Int32:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadInt32(buffer, offset);");
                    break;
                case FieldKind.UInt32:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadUInt32(buffer, offset);");
                    break;
                case FieldKind.Int64:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadInt64(buffer, offset);");
                    break;
                case FieldKind.UInt64:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadUInt64(buffer, offset);");
                    break;
                case FieldKind.Char:
                    sb.Append(indent).Append(name).AppendLine(" = (char)global::CookYourGame.Sim.CYGWire.ReadUInt16(buffer, offset);");
                    break;
                case FieldKind.Single:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadSingle(buffer, offset);");
                    break;
                case FieldKind.Double:
                    sb.Append(indent).Append(name).AppendLine(" = global::CookYourGame.Sim.CYGWire.ReadDouble(buffer, offset);");
                    break;
                case FieldKind.Enum:
                    sb.Append(indent).Append(name).Append(" = (").Append(field.Symbol.Type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat)).Append(")")
                        .Append(wire).Append(ReaderName(field.Underlying.SpecialType)).AppendLine("(buffer, offset);");
                    break;
                case FieldKind.Struct:
                    sb.Append(indent).Append(name).AppendLine(".ReadFrom(buffer, offset);");
                    break;
            }
        }

        private static string WriterName(SpecialType type)
        {
            switch (type)
            {
                case SpecialType.System_Byte: return "WriteByte";
                case SpecialType.System_SByte: return "WriteSByte";
                case SpecialType.System_Int16: return "WriteInt16";
                case SpecialType.System_UInt16: return "WriteUInt16";
                case SpecialType.System_Int32: return "WriteInt32";
                case SpecialType.System_UInt32: return "WriteUInt32";
                case SpecialType.System_Int64: return "WriteInt64";
                case SpecialType.System_UInt64: return "WriteUInt64";
                default: return "WriteInt32";
            }
        }

        private static string ReaderName(SpecialType type)
        {
            switch (type)
            {
                case SpecialType.System_Byte: return "ReadByte";
                case SpecialType.System_SByte: return "ReadSByte";
                case SpecialType.System_Int16: return "ReadInt16";
                case SpecialType.System_UInt16: return "ReadUInt16";
                case SpecialType.System_Int32: return "ReadInt32";
                case SpecialType.System_UInt32: return "ReadUInt32";
                case SpecialType.System_Int64: return "ReadInt64";
                case SpecialType.System_UInt64: return "ReadUInt64";
                default: return "ReadInt32";
            }
        }

        private static string EnumCast(ITypeSymbol underlying)
        {
            switch (underlying.SpecialType)
            {
                case SpecialType.System_Byte: return "(byte)";
                case SpecialType.System_SByte: return "(sbyte)";
                case SpecialType.System_Int16: return "(short)";
                case SpecialType.System_UInt16: return "(ushort)";
                case SpecialType.System_Int32: return "(int)";
                case SpecialType.System_UInt32: return "(uint)";
                case SpecialType.System_Int64: return "(long)";
                case SpecialType.System_UInt64: return "(ulong)";
                default: return "(int)";
            }
        }
    }
}
