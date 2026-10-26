using System;
using System.Collections.Generic;
using System.Text;

namespace CYG.Schema
{
    /// <summary>Wire type codes shared by `OXSD` and `OXS2`.</summary>
    public enum OxWireKind : byte
    {
        None = 0,
        U8 = 1,
        U16 = 2,
        U32 = 3,
        I32 = 4,
        F32 = 5,
        F64 = 6,
        Bool = 7,
        I64 = 8,
        I8 = 9,
        I16 = 10,
        String = 11,
        Array = 12
    }

    /// <summary>One field of a schema, in wire order.</summary>
    public struct OxFieldDef
    {
        public const byte FlagOptional = 1;
        public const byte FlagStatic = 2;
        public const byte FlagTransient = 4;

        public readonly string Name;
        public readonly OxWireKind Kind;
        public readonly byte Flags;
        public readonly OxWireKind Element;
        public readonly int Size;

        public OxFieldDef(string name, OxWireKind kind, byte flags, OxWireKind element, int size)
        {
            Name = name;
            Kind = kind;
            Flags = flags;
            Element = element;
            Size = size;
        }

        public bool Optional { get { return (Flags & FlagOptional) != 0; } }
        public bool FullOnly { get { return (Flags & (FlagStatic | FlagTransient)) != 0; } }
    }

    /// <summary>A collection schema decoded from the host descriptor.</summary>
    public sealed class OxSchemaDef
    {
        public string Name;
        public uint Version;
        public uint Hash;
        public OxFieldDef[] Fields = new OxFieldDef[0];

        Dictionary<string, int> index;

        public int IndexOf(string fieldName)
        {
            if (index == null)
            {
                index = new Dictionary<string, int>(Fields.Length, StringComparer.Ordinal);
                for (int i = 0; i < Fields.Length; i++) index[Fields[i].Name] = i;
            }
            int found;
            return index.TryGetValue(fieldName, out found) ? found : -1;
        }
    }

    /// <summary>A root map name and the schema it carries.</summary>
    public sealed class OxSchemaCollectionDef
    {
        public readonly string Name;
        public readonly string Schema;

        public OxSchemaCollectionDef(string name, string schema)
        {
            Name = name;
            Schema = schema;
        }
    }

    /// <summary>The decoded `cyg.schema.bin` payload.</summary>
    public sealed class OxSchemaRoot
    {
        public byte FormatVersion;
        public string Name = string.Empty;
        public uint Version;
        public uint Hash;
        public OxFieldDef[] Scalars = new OxFieldDef[0];
        public OxSchemaCollectionDef[] Collections = new OxSchemaCollectionDef[0];
        public OxSchemaDef[] Schemas = new OxSchemaDef[0];

        public OxSchemaDef Find(string schemaName)
        {
            for (int i = 0; i < Schemas.Length; i++)
            {
                if (Schemas[i].Name == schemaName) return Schemas[i];
            }
            return null;
        }
    }

    /// <summary>
    /// Base state row. Generated classes override the accessors with typed
    /// fields; without generated bindings the decoder uses
    /// <see cref="OxDynamicEntity"/>.
    /// </summary>
    public abstract class OxSchemaEntity
    {
        public uint Id;
        public OxSchemaDef Definition;

        public abstract int OxFieldCount { get; }
        public abstract void OxSet(int field, object value);
        public abstract object OxGet(int field);
        public abstract void OxClear();

        /// <summary>Reads a field by name, or null when the schema has no such field.</summary>
        public object Get(string fieldName)
        {
            int index = Definition == null ? -1 : Definition.IndexOf(fieldName);
            return index < 0 ? null : OxGet(index);
        }

        public bool Has(string fieldName)
        {
            return Definition != null && Definition.IndexOf(fieldName) >= 0;
        }
    }

    /// <summary>Untyped row used when no generated class is registered.</summary>
    public sealed class OxDynamicEntity : OxSchemaEntity
    {
        object[] values = new object[0];

        public OxDynamicEntity() { }

        public OxDynamicEntity(int fieldCount)
        {
            values = new object[fieldCount];
        }

        public override int OxFieldCount { get { return values.Length; } }

        public override void OxSet(int field, object value)
        {
            if (field >= 0 && field < values.Length) values[field] = value;
        }

        public override object OxGet(int field)
        {
            return field >= 0 && field < values.Length ? values[field] : null;
        }

        public override void OxClear()
        {
            for (int i = 0; i < values.Length; i++) values[i] = null;
        }
    }

    /// <summary>Maps schema names to generated entity constructors.</summary>
    public sealed class OxSchemaRegistry
    {
        readonly Dictionary<string, Func<OxSchemaEntity>> factories =
            new Dictionary<string, Func<OxSchemaEntity>>(StringComparer.Ordinal);

        public void Register(string schemaName, Func<OxSchemaEntity> factory)
        {
            if (string.IsNullOrEmpty(schemaName) || factory == null) return;
            factories[schemaName] = factory;
        }

        public OxSchemaEntity Create(OxSchemaDef definition)
        {
            Func<OxSchemaEntity> factory;
            OxSchemaEntity entity = null;
            if (factories.TryGetValue(definition.Name, out factory))
            {
                entity = factory();
            }
            if (entity == null) entity = new OxDynamicEntity(definition.Fields.Length);
            entity.Definition = definition;
            entity.Id = 0;
            entity.OxClear();
            return entity;
        }
    }

    /// <summary>One root collection: entities keyed by their wire id.</summary>
    public sealed class OxSchemaCollection
    {
        public string Name = string.Empty;
        public OxSchemaDef Definition;
        public readonly Dictionary<uint, OxSchemaEntity> Entities = new Dictionary<uint, OxSchemaEntity>();
        public readonly List<uint> Order = new List<uint>();
        public bool Ready;
    }

    /// <summary>
    /// Applies one `OXS2` section (a full snapshot or a field patch) to the
    /// state replicated from the host.
    ///
    /// ```csharp
    /// var registry = new OxSchemaRegistry();
    /// CYG.Schema.Generated.CYGSchemaGenerated.Register(registry);
    /// OxSchemaState state;
    /// if (OxSchemaState.TryCreate(descriptorBytes, out state, registry))
    ///     state.Apply(sectionBytes);
    /// ```
    /// </summary>
    public sealed class OxSchemaState
    {
        const int SectionHeader = 13;
        const int WorldHeader = 11;
        static readonly byte[] Magic = { (byte)'O', (byte)'X', (byte)'S', (byte)'2' };

        public OxSchemaRoot Root { get; private set; }
        public OxSchemaCollection[] Collections { get; private set; }
        public OxSchemaEntity RootEntity { get; private set; }
        public bool Ready { get; private set; }

        public event Action<string, uint, OxSchemaEntity> EntityAdded;
        public event Action<string, uint, OxSchemaEntity> EntityRemoved;
        public event Action<string, uint, int, object, object> FieldChanged;
        public event Action<string, uint, OxSchemaEntity> EntityUpdated;
        public event Action<int, object, object> RootFieldChanged;

        readonly OxSchemaRegistry registry;
        readonly OxSchemaDef rootDefinition;

        public OxSchemaState(OxSchemaRoot root, OxSchemaRegistry registry = null)
        {
            if (root == null) throw new ArgumentNullException("root");
            Root = root;
            this.registry = registry ?? new OxSchemaRegistry();

            Collections = new OxSchemaCollection[root.Collections.Length];
            for (int i = 0; i < Collections.Length; i++)
            {
                OxSchemaCollectionDef def = root.Collections[i];
                OxSchemaDef schema = root.Find(def.Schema);
                if (schema == null) throw new ArgumentException("unknown schema '" + def.Schema + "'");
                Collections[i] = new OxSchemaCollection
                {
                    Name = def.Name,
                    Definition = schema
                };
            }

            rootDefinition = new OxSchemaDef
            {
                Name = root.Name ?? string.Empty,
                Version = root.Version,
                Hash = root.Hash,
                Fields = root.Scalars
            };
            RootEntity = this.registry.Create(rootDefinition);
        }

        /// <summary>Parse an `OXSD` descriptor and build its state.</summary>
        public static bool TryCreate(
            byte[] descriptor,
            out OxSchemaState state,
            OxSchemaRegistry registry = null)
        {
            OxSchemaRoot root;
            if (!OxSchemaDescriptor.TryParse(descriptor, out root))
            {
                state = null;
                return false;
            }
            try
            {
                state = new OxSchemaState(root, registry);
                return true;
            }
            catch (ArgumentException)
            {
                state = null;
                return false;
            }
        }

        public OxSchemaCollection Collection(string name)
        {
            for (int i = 0; i < Collections.Length; i++)
            {
                if (Collections[i].Name == name) return Collections[i];
            }
            return null;
        }

        /// <summary>Apply one section. A malformed or unknown frame is ignored.</summary>
        public bool Apply(byte[] section)
        {
            if (section == null) return false;
            var reader = new OxReader(section, 0, section.Length);
            if (!IsMagic(ref reader)) return false;
            byte kind = reader.U8();
            if (reader.Failed) return false;
            if (kind == 1 || kind == 2)
            {
                if (Collections.Length == 0) return false;
                return ApplyBody(kind, Collections[0], ref reader);
            }
            if (kind == 3 || kind == 4)
            {
                return ApplyWorld(kind, ref reader);
            }
            return false;
        }

        bool ApplyBody(byte kind, OxSchemaCollection collection, ref OxReader reader)
        {
            return kind == 1
                ? ApplyFull(collection, ref reader)
                : ApplyPatch(collection, ref reader);
        }

        bool ApplySection(OxSchemaCollection collection, ref OxReader reader, byte expected)
        {
            if (!IsMagic(ref reader)) return false;
            byte kind = reader.U8();
            if (reader.Failed || kind != expected) return false;
            return ApplyBody(kind, collection, ref reader);
        }

        bool ApplyFull(OxSchemaCollection collection, ref OxReader reader)
        {
            uint hash = reader.U32();
            if (reader.Failed || hash != collection.Definition.Hash) return false;
            uint count = reader.U32();
            if (reader.Failed) return false;

            List<KeyValuePair<uint, object[]>> incoming =
                new List<KeyValuePair<uint, object[]>>((int)Math.Min(count, 65536u));
            HashSet<uint> ids = new HashSet<uint>();
            for (uint i = 0; i < count; i++)
            {
                uint id = reader.U32();
                if (reader.Failed) return false;
                if (!ids.Add(id)) return false;
                object[] values = new object[collection.Definition.Fields.Length];
                for (int field = 0; field < values.Length; field++)
                {
                    values[field] = ReadValue(ref reader, collection.Definition.Fields[field]);
                    if (reader.Failed) return false;
                }
                incoming.Add(new KeyValuePair<uint, object[]>(id, values));
            }

            for (int i = collection.Order.Count - 1; i >= 0; i--)
            {
                uint id = collection.Order[i];
                if (ids.Contains(id)) continue;
                OxSchemaEntity removed;
                if (collection.Entities.TryGetValue(id, out removed))
                {
                    collection.Entities.Remove(id);
                    collection.Order.RemoveAt(i);
                    if (EntityRemoved != null) EntityRemoved(collection.Name, id, removed);
                }
            }

            for (int i = 0; i < incoming.Count; i++)
            {
                uint id = incoming[i].Key;
                object[] values = incoming[i].Value;
                OxSchemaEntity entity;
                if (collection.Entities.TryGetValue(id, out entity))
                {
                    ApplyValues(collection, id, entity, values, true);
                    continue;
                }
                entity = registry.Create(collection.Definition);
                entity.Id = id;
                ApplyValues(collection, id, entity, values, false);
                collection.Entities.Add(id, entity);
                collection.Order.Add(id);
                if (EntityAdded != null) EntityAdded(collection.Name, id, entity);
            }
            collection.Ready = true;
            return true;
        }

        bool ApplyPatch(OxSchemaCollection collection, ref OxReader reader)
        {
            uint hash = reader.U32();
            if (reader.Failed || hash != collection.Definition.Hash) return false;
            // A patch before the full row would be lost; the host keeps sending
            // full snapshots until the client acknowledges the baseline.
            if (!collection.Ready) return false;
            uint operations = reader.U32();
            if (reader.Failed) return false;

            for (uint i = 0; i < operations; i++)
            {
                byte op = reader.U8();
                if (reader.Failed) return false;
                if (op == 1)
                {
                    uint id = reader.U32();
                    if (reader.Failed) return false;
                    object[] values = new object[collection.Definition.Fields.Length];
                    for (int field = 0; field < values.Length; field++)
                    {
                        values[field] = ReadValue(ref reader, collection.Definition.Fields[field]);
                        if (reader.Failed) return false;
                    }
                    OxSchemaEntity entity;
                    if (collection.Entities.TryGetValue(id, out entity))
                    {
                        ApplyValues(collection, id, entity, values, true);
                    }
                    else
                    {
                        entity = registry.Create(collection.Definition);
                        entity.Id = id;
                        ApplyValues(collection, id, entity, values, false);
                        collection.Entities.Add(id, entity);
                        collection.Order.Add(id);
                        if (EntityAdded != null) EntityAdded(collection.Name, id, entity);
                    }
                }
                else if (op == 2)
                {
                    uint id = reader.U32();
                    if (reader.Failed) return false;
                    OxSchemaEntity entity;
                    if (collection.Entities.TryGetValue(id, out entity))
                    {
                        collection.Entities.Remove(id);
                        collection.Order.Remove(id);
                        if (EntityRemoved != null) EntityRemoved(collection.Name, id, entity);
                    }
                }
                else if (op == 3)
                {
                    uint id = reader.U32();
                    uint mask = reader.U32();
                    if (reader.Failed) return false;
                    OxSchemaEntity entity;
                    collection.Entities.TryGetValue(id, out entity);
                    bool changed = false;
                    for (int field = 0; field < collection.Definition.Fields.Length; field++)
                    {
                        if ((mask & (1u << field)) == 0) continue;
                        object value = ReadValue(ref reader, collection.Definition.Fields[field]);
                        if (reader.Failed) return false;
                        if (entity == null) continue;
                        object previous = entity.OxGet(field);
                        entity.OxSet(field, value);
                        if (ValuesEqual(previous, value)) continue;
                        changed = true;
                        if (FieldChanged != null) FieldChanged(collection.Name, id, field, value, previous);
                    }
                    if (changed && EntityUpdated != null) EntityUpdated(collection.Name, id, entity);
                }
                else
                {
                    return false;
                }
            }
            return true;
        }

        bool ApplyWorld(byte kind, ref OxReader reader)
        {
            uint rootHash = reader.U32();
            ushort rootCount = reader.U16();
            if (reader.Failed) return false;
            if (rootCount != Root.Scalars.Length) return false;
            if (rootDefinition.Hash != rootHash) return false;

            uint mask = 0;
            if (kind == 4)
            {
                mask = reader.U32();
                if (reader.Failed) return false;
            }
            for (int field = 0; field < rootCount; field++)
            {
                if (kind == 4 && (mask & (1u << field)) == 0) continue;
                object value = ReadValue(ref reader, Root.Scalars[field]);
                if (reader.Failed) return false;
                object previous = RootEntity.OxGet(field);
                RootEntity.OxSet(field, value);
                if (!ValuesEqual(previous, value) && RootFieldChanged != null)
                {
                    RootFieldChanged(field, value, previous);
                }
            }

            ushort count = reader.U16();
            if (reader.Failed) return false;
            for (int i = 0; i < count; i++)
            {
                ushort collectionId = reader.U16();
                uint hash = reader.U32();
                uint length = reader.U32();
                if (reader.Failed) return false;
                if (collectionId >= Collections.Length) return false;
                OxSchemaCollection collection = Collections[collectionId];
                if (collection.Definition.Hash != hash) return false;
                long end = (long)reader.Offset + length;
                if (end > reader.End) return false;
                var section = new OxReader(reader.Bytes, reader.Offset, (int)end);
                byte expected = kind == 3 ? (byte)1 : (byte)2;
                if (!ApplySection(collection, ref section, expected)) return false;
                reader.Offset = (int)end;
            }
            if (reader.Offset != reader.End) return false;
            if (kind == 3) Ready = true;
            return true;
        }

        void ApplyValues(
            OxSchemaCollection collection,
            uint id,
            OxSchemaEntity entity,
            object[] values,
            bool fire)
        {
            bool changed = false;
            for (int field = 0; field < values.Length; field++)
            {
                object previous = entity.OxGet(field);
                object value = values[field];
                entity.OxSet(field, value);
                if (ValuesEqual(previous, value)) continue;
                changed = true;
                if (fire && FieldChanged != null)
                {
                    FieldChanged(collection.Name, id, field, value, previous);
                }
            }
            if (changed && fire && EntityUpdated != null)
            {
                EntityUpdated(collection.Name, id, entity);
            }
        }

        static bool IsMagic(ref OxReader reader)
        {
            for (int i = 0; i < Magic.Length; i++)
            {
                if (reader.U8() != Magic[i]) return false;
            }
            return !reader.Failed;
        }

        static object ReadValue(ref OxReader reader, OxFieldDef field)
        {
            if (field.Optional && reader.U8() == 0) return null;
            if (reader.Failed) return null;
            if (field.Kind != OxWireKind.Array) return ReadScalar(ref reader, field.Kind);

            OxWireKind element = field.Element == OxWireKind.None ? OxWireKind.F64 : field.Element;
            int count = reader.U8();
            if (reader.Failed) return null;
            Array values = Array.CreateInstance(ClrType(element), count);
            for (int i = 0; i < count; i++)
            {
                values.SetValue(ReadScalar(ref reader, element), i);
                if (reader.Failed) return null;
            }
            return values;
        }

        static object ReadScalar(ref OxReader reader, OxWireKind kind)
        {
            switch (kind)
            {
                case OxWireKind.U8: return reader.U8();
                case OxWireKind.I8: return reader.I8();
                case OxWireKind.U16: return reader.U16();
                case OxWireKind.I16: return reader.I16();
                case OxWireKind.U32: return reader.U32();
                case OxWireKind.I32: return reader.I32();
                case OxWireKind.F32: return reader.F32();
                case OxWireKind.F64: return reader.F64();
                case OxWireKind.Bool: return reader.U8() != 0;
                case OxWireKind.I64: return reader.I64();
                case OxWireKind.String: return reader.String8();
                default:
                    reader.Failed = true;
                    return null;
            }
        }

        static Type ClrType(OxWireKind kind)
        {
            switch (kind)
            {
                case OxWireKind.U8: return typeof(byte);
                case OxWireKind.I8: return typeof(sbyte);
                case OxWireKind.U16: return typeof(ushort);
                case OxWireKind.I16: return typeof(short);
                case OxWireKind.U32: return typeof(uint);
                case OxWireKind.I32: return typeof(int);
                case OxWireKind.F32: return typeof(float);
                case OxWireKind.F64: return typeof(double);
                case OxWireKind.Bool: return typeof(bool);
                case OxWireKind.I64: return typeof(long);
                case OxWireKind.String: return typeof(string);
                default: return typeof(object);
            }
        }

        static bool ValuesEqual(object a, object b)
        {
            if (ReferenceEquals(a, b)) return true;
            if (a == null || b == null) return false;
            Array left = a as Array;
            Array right = b as Array;
            if (left != null && right != null)
            {
                if (left.Length != right.Length) return false;
                for (int i = 0; i < left.Length; i++)
                {
                    if (!Equals(left.GetValue(i), right.GetValue(i))) return false;
                }
                return true;
            }
            return a.Equals(b);
        }
    }

    /// <summary>Decodes the `OXSD` descriptor sent by the host on join.</summary>
    public static class OxSchemaDescriptor
    {
        public const int Version = 3;

        public static bool TryParse(byte[] bytes, out OxSchemaRoot root)
        {
            root = null;
            if (bytes == null || bytes.Length < 6) return false;
            if (bytes[0] != (byte)'O' || bytes[1] != (byte)'X'
                || bytes[2] != (byte)'S' || bytes[3] != (byte)'D')
            {
                return false;
            }
            var reader = new OxReader(bytes, 4, bytes.Length);
            byte version = reader.U8();
            if (reader.Failed || version < 1 || version > Version) return false;
            reader.U8(); // flags

            ushort schemaCount = reader.U16();
            if (reader.Failed) return false;
            OxSchemaDef[] schemas = new OxSchemaDef[schemaCount];
            for (int i = 0; i < schemas.Length; i++)
            {
                string name = reader.String16();
                uint schemaVersion = reader.U32();
                uint hash = reader.U32();
                ushort fieldCount = reader.U16();
                if (reader.Failed) return false;
                OxFieldDef[] fields = new OxFieldDef[fieldCount];
                for (int f = 0; f < fields.Length; f++)
                {
                    fields[f] = ReadField(ref reader, version);
                    if (reader.Failed) return false;
                }
                schemas[i] = new OxSchemaDef
                {
                    Name = name,
                    Version = schemaVersion,
                    Hash = hash,
                    Fields = fields
                };
            }

            var result = new OxSchemaRoot
            {
                FormatVersion = version,
                Schemas = schemas
            };
            string rootName = reader.String16();
            if (reader.Failed) return false;

            if (version == 1)
            {
                ushort index = reader.U16();
                if (reader.Failed) return false;
                result.Name = rootName;
                if (rootName.Length > 0 && index < schemas.Length)
                {
                    result.Collections = new[]
                    {
                        new OxSchemaCollectionDef(rootName, schemas[index].Name)
                    };
                }
            }
            else
            {
                uint rootVersion = reader.U32();
                uint rootHash = reader.U32();
                ushort scalarCount = reader.U16();
                if (reader.Failed) return false;
                OxFieldDef[] scalars = new OxFieldDef[scalarCount];
                for (int i = 0; i < scalars.Length; i++)
                {
                    scalars[i] = ReadField(ref reader, version);
                    if (reader.Failed) return false;
                }
                ushort collectionCount = reader.U16();
                if (reader.Failed) return false;
                var collections = new OxSchemaCollectionDef[collectionCount];
                for (int i = 0; i < collections.Length; i++)
                {
                    string collectionName = reader.String16();
                    ushort index = reader.U16();
                    if (reader.Failed) return false;
                    if (index >= schemas.Length) return false;
                    collections[i] = new OxSchemaCollectionDef(collectionName, schemas[index].Name);
                }
                result.Name = rootName;
                result.Version = rootVersion;
                result.Hash = rootHash;
                result.Scalars = scalars;
                result.Collections = collections;
            }

            if (reader.Offset != bytes.Length) return false;
            root = result;
            return true;
        }

        static OxFieldDef ReadField(ref OxReader reader, byte version)
        {
            string name = reader.String16();
            byte code = reader.U8();
            OxWireKind kind;
            if (!TryKind(code, out kind))
            {
                reader.Failed = true;
                return default(OxFieldDef);
            }
            byte flags = 0;
            OxWireKind element = OxWireKind.None;
            if (version >= 3)
            {
                flags = reader.U8();
                byte elementCode = reader.U8();
                if (elementCode != 0 && !TryKind(elementCode, out element))
                {
                    reader.Failed = true;
                    return default(OxFieldDef);
                }
            }
            return new OxFieldDef(name, kind, flags, element, SizeOf(kind));
        }

        static bool TryKind(byte code, out OxWireKind kind)
        {
            if (code >= 1 && code <= 12)
            {
                kind = (OxWireKind)code;
                return true;
            }
            kind = OxWireKind.None;
            return false;
        }

        static int SizeOf(OxWireKind kind)
        {
            switch (kind)
            {
                case OxWireKind.U8:
                case OxWireKind.I8:
                case OxWireKind.Bool:
                    return 1;
                case OxWireKind.U16:
                case OxWireKind.I16:
                    return 2;
                case OxWireKind.U32:
                case OxWireKind.I32:
                case OxWireKind.F32:
                    return 4;
                case OxWireKind.F64:
                case OxWireKind.I64:
                    return 8;
                default:
                    return 0;
            }
        }
    }

    /// <summary>Little-endian, bounds-checked reader shared by both decoders.</summary>
    internal struct OxReader
    {
        readonly byte[] bytes;
        public int Offset;
        public readonly int End;
        public bool Failed;

        public OxReader(byte[] bytes, int offset, int end)
        {
            this.bytes = bytes;
            Offset = offset;
            End = end;
            Failed = false;
        }

        public byte[] Bytes { get { return bytes; } }

        bool Need(int count)
        {
            if (Failed) return false;
            if (count < 0 || Offset + count > End)
            {
                Failed = true;
                return false;
            }
            return true;
        }

        public byte U8()
        {
            if (!Need(1)) return 0;
            return bytes[Offset++];
        }

        public sbyte I8()
        {
            return unchecked((sbyte)U8());
        }

        public ushort U16()
        {
            if (!Need(2)) return 0;
            ushort value = (ushort)(bytes[Offset] | (bytes[Offset + 1] << 8));
            Offset += 2;
            return value;
        }

        public short I16()
        {
            return unchecked((short)U16());
        }

        public uint U32()
        {
            if (!Need(4)) return 0;
            uint value = (uint)(bytes[Offset]
                | (bytes[Offset + 1] << 8)
                | (bytes[Offset + 2] << 16)
                | (bytes[Offset + 3] << 24));
            Offset += 4;
            return value;
        }

        public int I32()
        {
            return unchecked((int)U32());
        }

        public long I64()
        {
            uint low = U32();
            uint high = U32();
            return (long)(((ulong)high << 32) | low);
        }

        public float F32()
        {
            return BitConverter.Int32BitsToSingle(unchecked((int)U32()));
        }

        public double F64()
        {
            return BitConverter.Int64BitsToDouble(I64());
        }

        public string String8()
        {
            int length = U8();
            if (Failed || !Need(length)) return string.Empty;
            string value = Encoding.UTF8.GetString(bytes, Offset, length);
            Offset += length;
            return value;
        }

        public string String16()
        {
            int length = U16();
            if (Failed || !Need(length)) return string.Empty;
            string value = Encoding.UTF8.GetString(bytes, Offset, length);
            Offset += length;
            return value;
        }
    }
}
