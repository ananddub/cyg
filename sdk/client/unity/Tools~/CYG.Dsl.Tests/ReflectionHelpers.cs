using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Reflection;
using System.Runtime.ExceptionServices;

namespace CookYourGame.Dsl.Tests
{
    internal static class ReflectionHelpers
    {
        private const BindingFlags InstanceAll =
            BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance;

        private const BindingFlags StaticAll =
            BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static;

        public static Type RequireType(Assembly assembly, string label, params string[] acceptedNames)
        {
            Type[] types = GetLoadableTypes(assembly);
            foreach (string name in acceptedNames)
            {
                Type[] matches = types.Where(t => string.Equals(t.Name, name, StringComparison.Ordinal)).ToArray();
                if (matches.Length == 1)
                {
                    return matches[0];
                }

                if (matches.Length > 1)
                {
                    Type? generated = matches.FirstOrDefault(t =>
                        t.Namespace != null && t.Namespace.Contains("Generated", StringComparison.Ordinal));
                    return generated ?? matches[0];
                }
            }

            string[] interesting = types
                .Select(t => t.Name)
                .Where(n => n.StartsWith("CYG", StringComparison.Ordinal)
                    || n.Contains("Fixed", StringComparison.Ordinal)
                    || acceptedNames.Any(a => n.Contains(a, StringComparison.OrdinalIgnoreCase)))
                .Distinct(StringComparer.Ordinal)
                .OrderBy(n => n, StringComparer.Ordinal)
                .Take(50)
                .ToArray();

            throw new CygTestException(
                $"generated assembly has no type '{label}' (tried: {string.Join(", ", acceptedNames)}). "
                + "Generated support type names seen: "
                + (interesting.Length == 0 ? "<none>" : string.Join(", ", interesting)));
        }

        public static Type[] GetLoadableTypes(Assembly assembly)
        {
            try
            {
                return assembly.GetTypes();
            }
            catch (ReflectionTypeLoadException ex)
            {
                Type[] loaded = ex.Types.Where(t => t != null).Select(t => t!).ToArray();
                if (loaded.Length == 0)
                {
                    string loaderMessages = string.Join("; ", ex.LoaderExceptions
                        .Where(e => e != null)
                        .Select(e => e!.Message));
                    throw new CygTestException("could not load any generated types: " + loaderMessages);
                }

                return loaded;
            }
        }

        public static int ReadSize(Type type)
        {
            FieldInfo? field = type.GetField("SizeInBytes", StaticAll);
            if (field != null)
            {
                object? value = field.IsLiteral ? field.GetRawConstantValue() : field.GetValue(null);
                if (value is int size)
                {
                    return size;
                }
            }

            PropertyInfo? property = type.GetProperty("SizeInBytes", StaticAll);
            if (property != null && property.GetIndexParameters().Length == 0)
            {
                object? value = property.GetValue(null);
                if (value is int size)
                {
                    return size;
                }
            }

            throw new CygTestException(
                $"{type.FullName} must expose a public const int SizeInBytes; it does not.");
        }

        public static int? TryReadStaticInt(Type type, string name)
        {
            FieldInfo? field = type.GetField(name, StaticAll);
            if (field == null)
            {
                return null;
            }

            object? value = field.IsLiteral ? field.GetRawConstantValue() : field.GetValue(null);
            return value is int intValue ? intValue : (int?)null;
        }

        public static object New(Type type)
        {
            object? value = Activator.CreateInstance(type);
            if (value == null)
            {
                throw new CygTestException(type.FullName + " could not be instantiated.");
            }

            return value;
        }

        public static object? GetValue(object target, string name, bool includeNonPublic = true)
        {
            Type type = target.GetType();
            BindingFlags flags = BindingFlags.Public | BindingFlags.Instance
                | (includeNonPublic ? BindingFlags.NonPublic : 0);
            FieldInfo? field = type.GetField(name, flags);
            if (field != null)
            {
                return field.GetValue(target);
            }

            PropertyInfo? property = type.GetProperty(name, flags);
            if (property != null && property.GetIndexParameters().Length == 0 && property.CanRead)
            {
                return property.GetValue(target);
            }

            return null;
        }

        public static void SetValue(object target, string name, object? value)
        {
            Type type = target.GetType();
            BindingFlags flags = InstanceAll;
            FieldInfo? field = type.GetField(name, flags);
            if (field != null)
            {
                field.SetValue(target, Coerce(field.FieldType, value));
                return;
            }

            PropertyInfo? property = type.GetProperty(name, flags);
            if (property != null && property.GetIndexParameters().Length == 0 && property.CanWrite)
            {
                property.SetValue(target, Coerce(property.PropertyType, value));
                return;
            }

            throw new CygTestException($"{type.FullName} has no writable member '{name}'.");
        }

        public static object? GetFieldOrProp(object target, string name)
        {
            return GetValue(target, name);
        }

        public static object ParseEnum(Type enumType, string value)
        {
            try
            {
                return Enum.Parse(enumType, value, true);
            }
            catch (Exception ex) when (ex is ArgumentException || ex is OverflowException)
            {
                throw new CygTestException($"could not parse '{value}' as {enumType.FullName}: {ex.Message}");
            }
        }

        public static object NewFP(Type fpType, int value)
        {
            ConstructorInfo? ctor = fpType.GetConstructor(new[] { typeof(int) });
            if (ctor != null)
            {
                return ctor.Invoke(new object[] { value });
            }

            ctor = fpType.GetConstructor(new[] { typeof(long) });
            if (ctor != null)
            {
                return ctor.Invoke(new object[] { (long)value });
            }

            object fp = New(fpType);
            SetValue(fp, "RawValue", (long)value);
            return fp;
        }

        public static object NewVector2(Type vectorType, Type fpType, int x, int y)
        {
            ConstructorInfo? ctor = vectorType.GetConstructor(new[] { fpType, fpType });
            if (ctor != null)
            {
                return ctor.Invoke(new[] { NewFP(fpType, x), NewFP(fpType, y) });
            }

            object value = New(vectorType);
            SetValue(value, "x", NewFP(fpType, x));
            SetValue(value, "y", NewFP(fpType, y));
            return value;
        }

        public static object NewVector3(Type vectorType, Type fpType, int x, int y, int z)
        {
            ConstructorInfo? ctor = vectorType.GetConstructor(new[] { fpType, fpType, fpType });
            if (ctor != null)
            {
                return ctor.Invoke(new[] { NewFP(fpType, x), NewFP(fpType, y), NewFP(fpType, z) });
            }

            object value = New(vectorType);
            SetValue(value, "x", NewFP(fpType, x));
            SetValue(value, "y", NewFP(fpType, y));
            SetValue(value, "z", NewFP(fpType, z));
            return value;
        }

        public static object NewQuaternion(Type quaternionType, Type fpType, int x, int y, int z, int w)
        {
            ConstructorInfo? ctor = quaternionType.GetConstructor(new[] { fpType, fpType, fpType, fpType });
            if (ctor != null)
            {
                return ctor.Invoke(new[]
                {
                    NewFP(fpType, x),
                    NewFP(fpType, y),
                    NewFP(fpType, z),
                    NewFP(fpType, w)
                });
            }

            object value = New(quaternionType);
            SetValue(value, "x", NewFP(fpType, x));
            SetValue(value, "y", NewFP(fpType, y));
            SetValue(value, "z", NewFP(fpType, z));
            SetValue(value, "w", NewFP(fpType, w));
            return value;
        }

        public static long ReadFPRaw(object fp)
        {
            object? raw = GetValue(fp, "RawValue");
            if (raw == null)
            {
                throw new CygTestException("FP value has no RawValue field.");
            }

            return Convert.ToInt64(raw, CultureInfo.InvariantCulture);
        }

        public static bool TrySetRefRaw(object owner, string fieldName, int raw)
        {
            Type ownerType = owner.GetType();
            FieldInfo? ownerField = ownerType.GetField(fieldName, InstanceAll);
            if (ownerField == null)
            {
                return false;
            }

            object? boxed = ownerField.GetValue(owner);
            if (boxed == null)
            {
                return false;
            }

            Type refType = boxed.GetType();
            string[] scalarNames = { "Id", "Value", "RawValue", "EntityId", "PlayerId", "Index", "AssetId" };
            foreach (string scalarName in scalarNames)
            {
                FieldInfo? field = refType.GetField(scalarName, InstanceAll);
                if (field != null && IsInteger(field.FieldType))
                {
                    field.SetValue(boxed, Convert.ChangeType(raw, field.FieldType, CultureInfo.InvariantCulture));
                    ownerField.SetValue(owner, boxed);
                    return true;
                }

                PropertyInfo? property = refType.GetProperty(scalarName, InstanceAll);
                if (property != null && property.CanWrite && IsInteger(property.PropertyType))
                {
                    property.SetValue(boxed, Convert.ChangeType(raw, property.PropertyType, CultureInfo.InvariantCulture));
                    ownerField.SetValue(owner, boxed);
                    return true;
                }
            }

            foreach (ConstructorInfo ctor in refType.GetConstructors())
            {
                ParameterInfo[] parameters = ctor.GetParameters();
                if (parameters.Length == 1 && IsInteger(parameters[0].ParameterType))
                {
                    object constructed = ctor.Invoke(new[]
                    {
                        Convert.ChangeType(raw, parameters[0].ParameterType, CultureInfo.InvariantCulture)
                    });
                    ownerField.SetValue(owner, constructed);
                    return true;
                }
            }

            return false;
        }

        public static bool TrySetIndexed(object owner, string fieldName, int index, object value)
        {
            Type ownerType = owner.GetType();
            FieldInfo? ownerField = ownerType.GetField(fieldName, InstanceAll);
            if (ownerField == null)
            {
                return false;
            }

            object? boxed = ownerField.GetValue(owner);
            if (boxed == null)
            {
                return false;
            }

            if (!TrySetIndexedInPlace(boxed, index, value))
            {
                return false;
            }

            ownerField.SetValue(owner, boxed);
            return true;
        }

        public static bool TrySetIndexedInPlace(object container, int index, object value)
        {
            Type type = container.GetType();
            PropertyInfo? indexer = type
                .GetProperties(InstanceAll)
                .FirstOrDefault(p =>
                    p.GetIndexParameters().Length == 1
                    && p.GetIndexParameters()[0].ParameterType == typeof(int)
                    && p.CanWrite);
            if (indexer != null)
            {
                indexer.SetValue(container, value, new object[] { index });
                return true;
            }

            FieldInfo[] fields = type.GetFields(InstanceAll);
            string[] namePatterns = { "Item", "Value", "Element", "Entry", "E" };
            foreach (string pattern in namePatterns)
            {
                FieldInfo? named = fields.FirstOrDefault(f =>
                    f.FieldType == value.GetType()
                    && (string.Equals(f.Name, pattern + index, StringComparison.Ordinal)
                        || string.Equals(f.Name, "_" + pattern.ToLowerInvariant() + index, StringComparison.Ordinal)
                        || string.Equals(f.Name, "_" + pattern + index, StringComparison.Ordinal)));
                if (named != null)
                {
                    named.SetValue(container, value);
                    return true;
                }
            }

            FieldInfo[] elementFields = fields.Where(f => f.FieldType == value.GetType()).ToArray();
            if (elementFields.Length > index)
            {
                elementFields[index].SetValue(container, value);
                return true;
            }

            return false;
        }

        public static void SetButton(object input, string name)
        {
            Type type = input.GetType();
            PropertyInfo? property = type.GetProperty(name, InstanceAll);
            if (property != null && property.CanWrite)
            {
                property.SetValue(input, true);
                return;
            }

            FieldInfo? field = type.GetField(name, InstanceAll);
            if (field != null && field.FieldType == typeof(bool))
            {
                field.SetValue(input, true);
                return;
            }

            throw new CygTestException($"{type.FullName} has no writable button member '{name}'.");
        }

        public static bool GetButton(object input, string name)
        {
            Type type = input.GetType();
            PropertyInfo? property = type.GetProperty(name, InstanceAll);
            if (property != null && property.CanRead)
            {
                return Convert.ToBoolean(property.GetValue(input), CultureInfo.InvariantCulture);
            }

            FieldInfo? field = type.GetField(name, InstanceAll);
            if (field != null && field.FieldType == typeof(bool))
            {
                return (bool)(field.GetValue(input) ?? false);
            }

            throw new CygTestException($"{type.FullName} has no readable button member '{name}'.");
        }

        public static ulong ReadButtons(object input)
        {
            object? value = GetValue(input, "_buttons");
            if (value == null)
            {
                throw new CygTestException(input.GetType().FullName + " has no _buttons field.");
            }

            return Convert.ToUInt64(value, CultureInfo.InvariantCulture);
        }

        public static int Write(object value, byte[] buffer, int offset)
        {
            MethodInfo method = RequireCodecMethod(value.GetType(), "WriteTo", 2, typeof(int), typeof(byte[]));
            object? result = Invoke(method, value, new object[] { buffer, offset });
            return (int)(result ?? 0);
        }

        public static void Read(object value, byte[] buffer, int offset)
        {
            MethodInfo method = RequireCodecMethod(value.GetType(), "ReadFrom", 2, typeof(void), typeof(byte[]));
            Invoke(method, value, new object[] { buffer, offset });
        }

        public static uint Checksum(object value)
        {
            Type type = value.GetType();
            MethodInfo? method = type
                .GetMethods(InstanceAll)
                .FirstOrDefault(m =>
                    (m.Name == "ComputeChecksum" || m.Name.EndsWith(".ComputeChecksum", StringComparison.Ordinal))
                    && !m.IsStatic
                    && m.ReturnType == typeof(void)
                    && m.GetParameters().Length == 1
                    && m.GetParameters()[0].ParameterType == typeof(uint).MakeByRefType());
            if (method == null)
            {
                throw new CygTestException(type.FullName + " has no ComputeChecksum(ref uint) method.");
            }

            object[] args = { 0xFFFFFFFFu };
            Invoke(method, value, args);
            return (uint)args[0];
        }

        private static MethodInfo RequireCodecMethod(
            Type type,
            string name,
            int parameterCount,
            Type returnType,
            Type firstParameterType)
        {
            MethodInfo? method = type
                .GetMethods(InstanceAll)
                .FirstOrDefault(m =>
                    (m.Name == name || m.Name.EndsWith("." + name, StringComparison.Ordinal))
                    && !m.IsStatic
                    && m.ReturnType == returnType
                    && m.GetParameters().Length == parameterCount
                    && m.GetParameters()[0].ParameterType == firstParameterType
                    && m.GetParameters()[1].ParameterType == typeof(int));
            if (method == null)
            {
                throw new CygTestException(
                    $"{type.FullName} has no {name}({(firstParameterType == typeof(byte[]) ? "byte[], int" : "…")}) codec method.");
            }

            return method;
        }

        public static RoundtripResult Roundtrip(object value, int offset)
        {
            Type type = value.GetType();
            int size = ReadSize(type);
            byte[] buffer = new byte[offset + size + 3];
            int written = Write(value, buffer, offset);
            TestAssert.Equal(size, written, type.Name + ".WriteTo return value");

            object restored = New(type);
            Read(restored, buffer, offset);

            byte[] second = new byte[size];
            int writtenAgain = Write(restored, second, 0);
            TestAssert.Equal(size, writtenAgain, type.Name + " restored WriteTo return value");
            for (int i = 0; i < size; i++)
            {
                if (second[i] != buffer[offset + i])
                {
                    throw new CygTestException(
                        $"{type.Name} wire bytes differ after round-trip at byte {i}: "
                        + $"0x{buffer[offset + i]:X2} != 0x{second[i]:X2}");
                }
            }

            uint firstChecksum = Checksum(value);
            uint secondChecksum = Checksum(restored);
            TestAssert.Equal(firstChecksum, secondChecksum, type.Name + " checksum after round-trip");
            return new RoundtripResult(restored, buffer, offset, size, firstChecksum);
        }

        public static object Invoke(MethodInfo method, object target, object?[] args)
        {
            try
            {
                return method.Invoke(target, args) ?? new object();
            }
            catch (TargetInvocationException ex) when (ex.InnerException != null)
            {
                ExceptionDispatchInfo.Capture(ex.InnerException).Throw();
                throw;
            }
        }

        private static bool IsInteger(Type type)
        {
            switch (Type.GetTypeCode(type))
            {
                case TypeCode.Byte:
                case TypeCode.SByte:
                case TypeCode.Int16:
                case TypeCode.UInt16:
                case TypeCode.Int32:
                case TypeCode.UInt32:
                case TypeCode.Int64:
                case TypeCode.UInt64:
                    return true;
                default:
                    return false;
            }
        }

        private static object? Coerce(Type targetType, object? value)
        {
            if (value == null || targetType.IsInstanceOfType(value))
            {
                return value;
            }

            if (targetType.IsEnum)
            {
                if (value is string text)
                {
                    return Enum.Parse(targetType, text, true);
                }

                if (value is Enum)
                {
                    return Enum.ToObject(targetType, Convert.ToInt64(value, CultureInfo.InvariantCulture));
                }

                return Enum.ToObject(targetType, value);
            }

            if (value is IConvertible && IsInteger(targetType))
            {
                return Convert.ChangeType(value, targetType, CultureInfo.InvariantCulture);
            }

            return value;
        }

        internal sealed class RoundtripResult
        {
            public RoundtripResult(object restored, byte[] buffer, int offset, int size, uint checksum)
            {
                Restored = restored;
                Buffer = buffer;
                Offset = offset;
                Size = size;
                Checksum = checksum;
            }

            public object Restored { get; }
            public byte[] Buffer { get; }
            public int Offset { get; }
            public int Size { get; }
            public uint Checksum { get; }
        }
    }
}
