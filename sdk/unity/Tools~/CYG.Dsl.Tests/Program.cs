using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using CookYourGame.CodeGen;
using CookYourGame.Dsl;
using CookYourGame.Dsl.Core;
using CookYourGame.Dsl.Emit;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;

namespace CookYourGame.Dsl.Tests
{
    internal static class Program
    {
        private static string _repoRoot = string.Empty;

        private static int Main()
        {
            try
            {
                _repoRoot = FindRepoRoot();
                Assembly arena = CompileSample("arena.cyg", "CygArenaGenerated");
                Assembly types = CompileSample("types.cyg", "CygTypesGenerated");

                TestArena(arena);
                TestAllTypes(types);
                TestDiagnostics();

                Console.WriteLine(
                    "CYG DSL tests passed: assertions=" + TestAssert.Count
                    + " arenaTypes=" + ReflectionHelpers.GetLoadableTypes(arena).Length
                    + " typeTypes=" + ReflectionHelpers.GetLoadableTypes(types).Length);
                return 0;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine("CYG DSL tests FAILED: " + ex.GetType().Name + ": " + ex.Message);
                if (!(ex is CygTestException))
                {
                    Console.Error.WriteLine(ex.StackTrace);
                }
                return 1;
            }
        }

        private static void TestArena(Assembly assembly)
        {
            Type fp = Require(assembly, "FP");
            Type entityRef = Require(assembly, "CYGEntityRef");
            Type playerRef = Require(assembly, "CYGPlayerRef");

            TestArenaMetadata(assembly);
            TestCharacter(assembly, fp, entityRef, playerRef);
            TestInventory(assembly, fp, entityRef);
            TestPlayerInfo(assembly);
            TestInput(assembly, fp);
            TestUnion(assembly, fp);
            TestGlobals(assembly, fp, entityRef);
            TestEventConstants(assembly, fp);
            TestGeneratedWorld(assembly);
        }

        private static void TestArenaMetadata(Assembly assembly)
        {
            Type character = Require(assembly, "Character");
            TestAssert.Equal(125, ReflectionHelpers.ReadSize(character), "Character.SizeInBytes");
            TestAssert.Equal(1001, ReflectionHelpers.TryReadStaticInt(character, "TypeId"), "Character.TypeId");
            object probe = ReflectionHelpers.New(character);
            TestAssert.Equal(1001, Convert.ToInt32(character.GetProperty("ComponentTypeId")!.GetValue(probe), System.Globalization.CultureInfo.InvariantCulture), "Character.ComponentTypeId");
            TestAssert.Require(ReadStaticUInt(character, "SchemaHash") != 0, "Character.SchemaHash must be non-zero");
            TestAssert.Require(Convert.ToUInt32(character.GetProperty("ComponentSchemaHash")!.GetValue(probe), System.Globalization.CultureInfo.InvariantCulture) != 0,
                "Character.ComponentSchemaHash must be non-zero");

            TestAssert.Equal(17, ReflectionHelpers.ReadSize(Require(assembly, "DamageInfo")), "DamageInfo.SizeInBytes");
            TestAssert.Equal(12, ReflectionHelpers.ReadSize(Require(assembly, "Loadout")), "Loadout.SizeInBytes");
            TestAssert.Equal(4, ReflectionHelpers.ReadSize(Require(assembly, "WeaponSpecRef")), "WeaponSpecRef.SizeInBytes");
            TestAssert.Equal(64, ReflectionHelpers.ReadSize(Require(assembly, "CYGFixedString32")), "CYGFixedString32.SizeInBytes");
            TestAssert.Equal(32, ReflectionHelpers.ReadSize(Require(assembly, "FPFixedArray4")), "FPFixedArray4.SizeInBytes");
            TestAssert.Equal(132, ReflectionHelpers.ReadSize(Require(assembly, "EntityRefFixedList16")), "EntityRefFixedList16.SizeInBytes");
            TestAssert.Equal(212, ReflectionHelpers.ReadSize(Require(assembly, "Int32FPFixedDictionary16")), "Int32FPFixedDictionary16.SizeInBytes");
        }

        private static void TestCharacter(Assembly assembly, Type fp, Type entityRef, Type playerRef)
        {
            Type characterType = Require(assembly, "Character");
            Type damageType = Require(assembly, "DamageInfo");
            Type weaponType = Require(assembly, "WeaponType");
            Type statusType = Require(assembly, "PlayerStatus");

            object character = ReflectionHelpers.New(characterType);
            ReflectionHelpers.SetValue(character, "Health", ReflectionHelpers.NewFP(fp, 100));
            ReflectionHelpers.SetValue(character, "MaxHealth", ReflectionHelpers.NewFP(fp, 150));
            ReflectionHelpers.SetValue(character, "Velocity", ReflectionHelpers.NewVector3(Require(assembly, "FPVector3"), fp, 1, -2, 3));
            ReflectionHelpers.SetValue(character, "Rotation", ReflectionHelpers.NewQuaternion(Require(assembly, "FPQuaternion"), fp, 0, 0, 0, 7));
            ReflectionHelpers.SetValue(character, "ActiveWeapon", "Rifle");
            ReflectionHelpers.SetValue(character, "Status", "Sprinting");
            ReflectionHelpers.SetValue(character, "Owner", playerRef.GetConstructor(new[] { typeof(ushort) })!.Invoke(new object[] { (ushort)7 }));
            SetElement(character, "AbilityCooldowns", 0, ReflectionHelpers.NewFP(fp, 5));
            SetElement(character, "AbilityCooldowns", 3, ReflectionHelpers.NewFP(fp, 9));

            object damage = ReflectionHelpers.New(damageType);
            ReflectionHelpers.SetValue(damage, "Source", NewEntityRef(entityRef, 3, 4));
            ReflectionHelpers.SetValue(damage, "Amount", ReflectionHelpers.NewFP(fp, 25));
            ReflectionHelpers.SetValue(damage, "DamageType", (byte)2);
            ReflectionHelpers.SetValue(character, "LastDamage", damage);

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(character, 3);
            object restored = result.Restored;

            TestAssert.Equal(RawOfInt(fp, 100), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restored, "Health")!), "Character.Health raw");
            TestAssert.Equal("Rifle", ReflectionHelpers.GetValue(restored, "ActiveWeapon")!.ToString(), "Character.ActiveWeapon");
            TestAssert.Equal("Sprinting", ReflectionHelpers.GetValue(restored, "Status")!.ToString(), "Character.Status");
            TestAssert.Equal(7, Convert.ToInt32(ReflectionHelpers.GetValue(ReflectionHelpers.GetValue(restored, "Owner"), "Value"), System.Globalization.CultureInfo.InvariantCulture), "Character.Owner");

            object restoredDamage = ReflectionHelpers.GetValue(restored, "LastDamage")!;
            TestAssert.Equal(RawOfInt(fp, 25), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restoredDamage, "Amount")!), "Character.LastDamage.Amount");
            object restoredSource = ReflectionHelpers.GetValue(restoredDamage, "Source")!;
            TestAssert.Equal(3, Convert.ToInt32(ReflectionHelpers.GetValue(restoredSource, "Index"), System.Globalization.CultureInfo.InvariantCulture), "Character.LastDamage.Source.Index");
            TestAssert.Equal(4, Convert.ToInt32(ReflectionHelpers.GetValue(restoredSource, "Version"), System.Globalization.CultureInfo.InvariantCulture), "Character.LastDamage.Source.Version");

            object cooldowns = ReflectionHelpers.GetValue(restored, "AbilityCooldowns")!;
            TestAssert.Equal(RawOfInt(fp, 5), ReflectionHelpers.ReadFPRaw(GetIndexed(cooldowns, 0)), "Character.AbilityCooldowns[0]");
            TestAssert.Equal(RawOfInt(fp, 9), ReflectionHelpers.ReadFPRaw(GetIndexed(cooldowns, 3)), "Character.AbilityCooldowns[3]");

            object changed = ReflectionHelpers.New(characterType);
            CopyViaCodec(character, changed);
            object health = ReflectionHelpers.GetValue(changed, "Health")!;
            ReflectionHelpers.SetValue(changed, "Health", ReflectionHelpers.NewFP(fp, 101));
            TestAssert.Require(ReflectionHelpers.Checksum(character) != ReflectionHelpers.Checksum(changed),
                "Character checksum must change when Health changes");
            _ = weaponType;
            _ = statusType;
            _ = health;
        }

        private static void TestInventory(Assembly assembly, Type fp, Type entityRef)
        {
            Type inventoryType = Require(assembly, "Inventory");
            TestAssert.Equal(344, ReflectionHelpers.ReadSize(inventoryType), "Inventory.SizeInBytes");

            object inventory = ReflectionHelpers.New(inventoryType);
            AddToList(inventory, "Items", NewEntityRef(entityRef, 1, 1));
            AddToList(inventory, "Items", NewEntityRef(entityRef, 2, 1));
            SetDictionary(inventory, "BuffDurations", 7, ReflectionHelpers.NewFP(fp, 3));

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(inventory, 0);
            object restored = result.Restored;
            object items = ReflectionHelpers.GetValue(restored, "Items")!;
            TestAssert.Equal(2, Convert.ToInt32(ReflectionHelpers.GetValue(items, "_count"), System.Globalization.CultureInfo.InvariantCulture), "Inventory.Items.Count");
            object first = GetIndexed(items, 0);
            TestAssert.Equal(1, Convert.ToInt32(ReflectionHelpers.GetValue(first, "Index"), System.Globalization.CultureInfo.InvariantCulture), "Inventory.Items[0].Index");

            object value = TryGetDictionary(restored, "BuffDurations", 7);
            TestAssert.Require(value != null, "Inventory.BuffDurations must contain key 7");
            TestAssert.Equal(RawOfInt(fp, 3), ReflectionHelpers.ReadFPRaw(value!), "Inventory.BuffDurations[7]");
            TestAssert.Require(TryGetDictionary(restored, "BuffDurations", 8) == null, "Inventory.BuffDurations must not contain key 8");
        }

        private static void TestPlayerInfo(Assembly assembly)
        {
            Type infoType = Require(assembly, "PlayerInfo");
            TestAssert.Equal(64, ReflectionHelpers.ReadSize(infoType), "PlayerInfo.SizeInBytes");

            object info = ReflectionHelpers.New(infoType);
            object name = ReflectionHelpers.GetValue(info, "DisplayName")!;
            Invoke(name, "Set", "Rogue");
            ReflectionHelpers.SetValue(info, "DisplayName", name);

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(info, 0);
            object restoredName = ReflectionHelpers.GetValue(result.Restored, "DisplayName")!;
            TestAssert.Equal("Rogue", Invoke(restoredName, "AsString"), "PlayerInfo.DisplayName");
            TestAssert.Equal("Rogue", restoredName.ToString(), "PlayerInfo.DisplayName.ToString");
        }

        private static void TestInput(Assembly assembly, Type fp)
        {
            Type inputType = Require(assembly, "CYGInput");
            TestAssert.Equal(33, ReflectionHelpers.ReadSize(inputType), "CYGInput.SizeInBytes");
            Type vector2 = Require(assembly, "FPVector2");

            object input = ReflectionHelpers.New(inputType);
            ReflectionHelpers.SetButton(input, "Jump");
            ReflectionHelpers.SetButton(input, "Fire");
            ReflectionHelpers.SetButton(input, "Sprint");
            ReflectionHelpers.SetValue(input, "MoveDirection", ReflectionHelpers.NewVector2(vector2, fp, 1, -2));
            ReflectionHelpers.SetValue(input, "AimDirection", ReflectionHelpers.NewVector2(vector2, fp, 3, 4));

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(input, 1);
            object restored = result.Restored;
            TestAssert.Equal(35UL, ReflectionHelpers.ReadButtons(restored), "CYGInput buttons");
            TestAssert.Require(ReflectionHelpers.GetButton(restored, "Jump"), "CYGInput.Jump");
            TestAssert.Require(ReflectionHelpers.GetButton(restored, "Fire"), "CYGInput.Fire");
            TestAssert.Require(ReflectionHelpers.GetButton(restored, "Sprint"), "CYGInput.Sprint");
            TestAssert.Require(!ReflectionHelpers.GetButton(restored, "Reload"), "CYGInput.Reload stays false");

            object move = ReflectionHelpers.GetValue(restored, "MoveDirection")!;
            TestAssert.Equal(RawOfInt(fp, -2), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(move, "y")!), "CYGInput.MoveDirection.y");
        }

        private static void TestUnion(Assembly assembly, Type fp)
        {
            Type unionType = Require(assembly, "AbilityData");
            TestAssert.Equal(33, ReflectionHelpers.ReadSize(unionType), "AbilityData.SizeInBytes");
            Type aType = Require(assembly, "AbilityDataAbilityA");
            Type bType = Require(assembly, "AbilityDataAbilityB");
            TestAssert.Equal(32, ReflectionHelpers.ReadSize(aType), "AbilityDataAbilityA.SizeInBytes");
            TestAssert.Equal(12, ReflectionHelpers.ReadSize(bType), "AbilityDataAbilityB.SizeInBytes");

            object union = ReflectionHelpers.New(unionType);
            ReflectionHelpers.SetValue(union, "Tag", (byte)1);
            object variant = ReflectionHelpers.New(aType);
            ReflectionHelpers.SetValue(variant, "Damage", ReflectionHelpers.NewFP(fp, 11));
            ReflectionHelpers.SetValue(variant, "Direction", ReflectionHelpers.NewVector3(Require(assembly, "FPVector3"), fp, 1, 2, 3));
            ReflectionHelpers.SetValue(union, "AbilityA", variant);

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(union, 5);
            object restored = result.Restored;
            TestAssert.Equal((byte)1, Convert.ToByte(ReflectionHelpers.GetValue(restored, "Tag"), System.Globalization.CultureInfo.InvariantCulture), "AbilityData.Tag");
            object restoredA = ReflectionHelpers.GetValue(restored, "AbilityA")!;
            TestAssert.Equal(RawOfInt(fp, 11), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restoredA, "Damage")!), "AbilityData.AbilityA.Damage");
            object direction = ReflectionHelpers.GetValue(restoredA, "Direction")!;
            TestAssert.Equal(RawOfInt(fp, 3), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(direction, "z")!), "AbilityData.AbilityA.Direction.z");

            object second = ReflectionHelpers.New(unionType);
            ReflectionHelpers.SetValue(second, "Tag", (byte)2);
            object variantB = ReflectionHelpers.New(bType);
            ReflectionHelpers.SetValue(variantB, "TargetCount", 6);
            ReflectionHelpers.SetValue(variantB, "Radius", ReflectionHelpers.NewFP(fp, 4));
            ReflectionHelpers.SetValue(second, "AbilityB", variantB);
            object restoredB = ReflectionHelpers.Roundtrip(second, 0).Restored;
            TestAssert.Equal(6, Convert.ToInt32(ReflectionHelpers.GetValue(ReflectionHelpers.GetValue(restoredB, "AbilityB"), "TargetCount"), System.Globalization.CultureInfo.InvariantCulture), "AbilityData.AbilityB.TargetCount");
            TestAssert.Require(ReflectionHelpers.Checksum(union) != ReflectionHelpers.Checksum(second), "union checksum must differ by active variant");
        }

        private static void TestGlobals(Assembly assembly, Type fp, Type entityRef)
        {
            Type globalsType = Require(assembly, "CYGGlobals");
            TestAssert.Equal(25, ReflectionHelpers.ReadSize(globalsType), "CYGGlobals.SizeInBytes");

            object globals = ReflectionHelpers.New(globalsType);
            ReflectionHelpers.SetValue(globals, "MatchTimer", ReflectionHelpers.NewFP(fp, 30));
            ReflectionHelpers.SetValue(globals, "RedTeamScore", 2);
            ReflectionHelpers.SetValue(globals, "BlueTeamScore", 3);
            ReflectionHelpers.SetValue(globals, "Flag", NewEntityRef(entityRef, 9, 1));
            ReflectionHelpers.SetValue(globals, "IsMatchStarted", true);

            object restored = ReflectionHelpers.Roundtrip(globals, 2).Restored;
            TestAssert.Equal(3, Convert.ToInt32(ReflectionHelpers.GetValue(restored, "BlueTeamScore"), System.Globalization.CultureInfo.InvariantCulture), "CYGGlobals.BlueTeamScore");
            TestAssert.Require(Convert.ToBoolean(ReflectionHelpers.GetValue(restored, "IsMatchStarted"), System.Globalization.CultureInfo.InvariantCulture), "CYGGlobals.IsMatchStarted");
            TestAssert.Equal(9, Convert.ToInt32(ReflectionHelpers.GetValue(ReflectionHelpers.GetValue(restored, "Flag"), "Index"), System.Globalization.CultureInfo.InvariantCulture), "CYGGlobals.Flag.Index");
        }

        private static void TestEventConstants(Assembly assembly, Type fp)
        {
            Type hit = Require(assembly, "HitEffect");
            Type eliminated = Require(assembly, "PlayerEliminated");
            Type damaged = Require(assembly, "OnPlayerDamaged");
            Type ended = Require(assembly, "OnMatchEnded");

            TestAssert.Equal(49, ReflectionHelpers.ReadSize(hit), "HitEffect.SizeInBytes");
            TestAssert.Equal(1000001, ReflectionHelpers.TryReadStaticInt(hit, "EventId"), "HitEffect.EventId");
            TestAssert.Equal(false, ReadStaticBool(hit, "Synced"), "HitEffect.Synced");
            TestAssert.Equal(21, ReflectionHelpers.ReadSize(eliminated), "PlayerEliminated.SizeInBytes");
            TestAssert.Equal(1000002, ReflectionHelpers.TryReadStaticInt(eliminated, "EventId"), "PlayerEliminated.EventId");
            TestAssert.Equal(true, ReadStaticBool(eliminated, "Synced"), "PlayerEliminated.Synced");
            TestAssert.Equal(24, ReflectionHelpers.ReadSize(damaged), "OnPlayerDamaged.SizeInBytes");
            TestAssert.Equal(1, ReflectionHelpers.TryReadStaticInt(damaged, "SignalIndex"), "OnPlayerDamaged.SignalIndex");
            TestAssert.Equal(4, ReflectionHelpers.ReadSize(ended), "OnMatchEnded.SizeInBytes");
            TestAssert.Equal(2, ReflectionHelpers.TryReadStaticInt(ended, "SignalIndex"), "OnMatchEnded.SignalIndex");

            object payload = ReflectionHelpers.New(hit);
            ReflectionHelpers.SetValue(payload, "Position", ReflectionHelpers.NewVector3(Require(assembly, "FPVector3"), fp, 1, 2, 3));
            ReflectionHelpers.SetValue(payload, "Normal", ReflectionHelpers.NewVector3(Require(assembly, "FPVector3"), fp, 0, 1, 0));
            ReflectionHelpers.SetValue(payload, "MaterialType", (byte)4);
            ReflectionHelpers.Roundtrip(payload, 0);
        }

        private static void TestGeneratedWorld(Assembly assembly)
        {
            Type fp = Require(assembly, "FP");
            Type worldType = Require(assembly, "CYGWorld");
            Type characterType = Require(assembly, "Character");
            Type transformType = Require(assembly, "Transform");
            object world = Activator.CreateInstance(worldType)!;

            MethodInfo register = worldType.GetMethod("Register", Type.EmptyTypes)!;
            register.MakeGenericMethod(characterType).Invoke(world, null);
            register.MakeGenericMethod(transformType).Invoke(world, null);

            MethodInfo create = worldType.GetMethod("CreateEntity", Type.EmptyTypes)!;
            object first = create.Invoke(world, null)!;
            object second = create.Invoke(world, null)!;
            TestAssert.Equal(2, ReadIntProperty(world, "EntityCount"), "world.EntityCount after two spawns");

            object character = Activator.CreateInstance(characterType)!;
            ReflectionHelpers.SetValue(character, "Health", ReflectionHelpers.NewFP(fp, 100));
            ReflectionHelpers.SetValue(character, "MaxHealth", ReflectionHelpers.NewFP(fp, 100));
            MethodInfo setCharacter = worldType.GetMethod("Set")!.MakeGenericMethod(characterType);
            setCharacter.Invoke(world, new[] { first, character });

            object transform = Activator.CreateInstance(transformType)!;
            ReflectionHelpers.SetValue(transform, "Position",
                ReflectionHelpers.NewVector3(Require(assembly, "FPVector3"), fp, 3, 4, 5));
            MethodInfo setTransform = worldType.GetMethod("Set")!.MakeGenericMethod(transformType);
            setTransform.Invoke(world, new[] { first, transform });

            MethodInfo tryGetCharacter = worldType.GetMethod("TryGet")!.MakeGenericMethod(characterType);
            object?[] characterArgs = { first, null };
            TestAssert.Require((bool)tryGetCharacter.Invoke(world, characterArgs)!, "world.TryGet(Character) after Set");
            TestAssert.Equal(RawOfInt(fp, 100),
                ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(characterArgs[1]!, "Health")!),
                "world Character.Health raw");

            MethodInfo tryGetTransform = worldType.GetMethod("TryGet")!.MakeGenericMethod(transformType);
            object?[] transformArgs = { first, null };
            TestAssert.Require((bool)tryGetTransform.Invoke(world, transformArgs)!, "world.TryGet(Transform) after Set");
            object storedPosition = ReflectionHelpers.GetValue(transformArgs[1]!, "Position")!;
            TestAssert.Equal(RawOfInt(fp, 3),
                ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(storedPosition, "X")!),
                "world Transform.Position.X raw");

            MethodInfo checksum = worldType.GetMethod("ComputeChecksum", Type.EmptyTypes)!;
            uint beforeSnapshot = (uint)checksum.Invoke(world, null)!;
            object snapshot = worldType.GetMethod("CreateSnapshot", Type.EmptyTypes)!.Invoke(world, null)!;
            TestAssert.Equal(beforeSnapshot, (uint)checksum.Invoke(world, null)!, "checksum stable across snapshot capture");

            ReflectionHelpers.SetValue(character, "Health", ReflectionHelpers.NewFP(fp, 1));
            setCharacter.Invoke(world, new[] { first, character });
            MethodInfo removeTransform = worldType.GetMethod("Remove")!.MakeGenericMethod(transformType);
            TestAssert.Require((bool)removeTransform.Invoke(world, new[] { first })!, "world.Remove(Transform)");
            MethodInfo hasTransform = worldType.GetMethod("Has")!.MakeGenericMethod(transformType);
            TestAssert.Equal(false, (bool)hasTransform.Invoke(world, new[] { first })!, "Transform removed");
            MethodInfo destroy = worldType.GetMethod("DestroyEntity")!;
            TestAssert.Require((bool)destroy.Invoke(world, new[] { second })!, "world.DestroyEntity(second)");
            TestAssert.Require((uint)checksum.Invoke(world, null)! != beforeSnapshot,
                "world checksum changes after mutation");

            worldType.GetMethod("Restore")!.Invoke(world, new[] { snapshot });
            TestAssert.Equal(beforeSnapshot, (uint)checksum.Invoke(world, null)!,
                "world checksum restored from snapshot");
            MethodInfo exists = worldType.GetMethod("Exists")!;
            TestAssert.Require((bool)exists.Invoke(world, new[] { second })!, "second entity alive after restore");
            TestAssert.Equal(2, ReadIntProperty(world, "EntityCount"), "world.EntityCount after restore");

            object?[] restoredCharacter = { first, null };
            TestAssert.Require((bool)tryGetCharacter.Invoke(world, restoredCharacter)!, "Character alive after restore");
            TestAssert.Equal(RawOfInt(fp, 100),
                ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restoredCharacter[1]!, "Health")!),
                "world Character.Health restored");
            object?[] restoredTransform = { first, null };
            TestAssert.Require((bool)tryGetTransform.Invoke(world, restoredTransform)!, "Transform restored");
            object restoredPosition = ReflectionHelpers.GetValue(restoredTransform[1]!, "Position")!;
            TestAssert.Equal(RawOfInt(fp, 5),
                ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restoredPosition, "Z")!),
                "world Transform.Position.Z restored");
        }

        private static int ReadIntProperty(object instance, string name)
        {
            return Convert.ToInt32(
                instance.GetType().GetProperty(name)!.GetValue(instance),
                System.Globalization.CultureInfo.InvariantCulture);
        }

        private static void TestAllTypes(Assembly assembly)
        {
            Type fp = Require(assembly, "FP");
            Type vector3 = Require(assembly, "FPVector3");

            TestAssert.Equal(31, ReflectionHelpers.ReadSize(Require(assembly, "PrimitiveSet")), "PrimitiveSet.SizeInBytes");
            TestAssert.Equal(314, ReflectionHelpers.ReadSize(Require(assembly, "MathSet")), "MathSet.SizeInBytes");
            TestAssert.Equal(32, ReflectionHelpers.ReadSize(Require(assembly, "StringSet")), "StringSet.SizeInBytes");
            TestAssert.Equal(208, ReflectionHelpers.ReadSize(Require(assembly, "CollectionSet")), "CollectionSet.SizeInBytes");
            TestAssert.Equal(4, ReflectionHelpers.ReadSize(Require(assembly, "AssetSet")), "AssetSet.SizeInBytes");
            TestAssert.Equal(18, ReflectionHelpers.ReadSize(Require(assembly, "Choice")), "Choice.SizeInBytes");
            TestAssert.Equal(423, ReflectionHelpers.ReadSize(Require(assembly, "FullState")), "FullState.SizeInBytes");
            TestAssert.Equal(10, ReflectionHelpers.ReadSize(Require(assembly, "CYGInput")), "types CYGInput.SizeInBytes");
            TestAssert.Equal(20, ReflectionHelpers.ReadSize(Require(assembly, "CYGGlobals")), "types CYGGlobals.SizeInBytes");
            TestAssert.Equal(4, ReflectionHelpers.ReadSize(Require(assembly, "Plain")), "Plain.SizeInBytes");
            TestAssert.Equal(17, ReflectionHelpers.ReadSize(Require(assembly, "Shaped")), "Shaped.SizeInBytes");
            TestAssert.Equal(16, ReflectionHelpers.ReadSize(Require(assembly, "OnPing")), "OnPing.SizeInBytes");
            TestAssert.Equal(4, ReflectionHelpers.ReadSize(Require(assembly, "OnDone")), "OnDone.SizeInBytes");

            Type empty = Require(assembly, "Empty");
            TestAssert.Equal(0, ReflectionHelpers.ReadSize(empty), "Empty.SizeInBytes");
            TestAssert.Equal(1000102, ReflectionHelpers.TryReadStaticInt(empty, "EventId"), "Empty.EventId");
            TestAssert.Equal(false, ReadStaticBool(empty, "Synced"), "Empty.Synced");
            ReflectionHelpers.Roundtrip(ReflectionHelpers.New(empty), 0);

            Type heartbeat = Require(assembly, "Heartbeat");
            TestAssert.Equal(0, ReflectionHelpers.ReadSize(heartbeat), "Heartbeat.SizeInBytes");
            TestAssert.Equal(3, ReflectionHelpers.TryReadStaticInt(heartbeat, "SignalIndex"), "Heartbeat.SignalIndex");
            ReflectionHelpers.Roundtrip(ReflectionHelpers.New(heartbeat), 0);

            Type fullState = Require(assembly, "FullState");
            TestAssert.Equal(2001, ReflectionHelpers.TryReadStaticInt(fullState, "TypeId"), "FullState.TypeId");
            TestAssert.Require(ReadStaticUInt(fullState, "SchemaHash") != 0, "FullState.SchemaHash must be non-zero");

            TestFPMatrix(assembly, fp);
            TestPrimitiveSet(assembly);
            TestMathSet(assembly, fp);
            TestStringSet(assembly);
            TestCollectionSet(assembly, fp);
            TestAssetSet(assembly);
            TestChoiceUnion(assembly, fp);
            TestFullState(assembly, fp, vector3);
            TestWideInput(assembly, fp);
            TestFlags(assembly);
        }

        private static void TestFPMatrix(Assembly assembly, Type fp)
        {
            Type matrixType = Require(assembly, "FPMatrix4x4");
            TestAssert.Equal(128, ReflectionHelpers.ReadSize(matrixType), "FPMatrix4x4.SizeInBytes");
            Type aabbType = Require(assembly, "FPAABB");
            Type boundsType = Require(assembly, "FPBounds");
            TestAssert.Equal(48, ReflectionHelpers.ReadSize(aabbType), "FPAABB.SizeInBytes");
            TestAssert.Equal(48, ReflectionHelpers.ReadSize(boundsType), "FPBounds.SizeInBytes");

            object matrix = NewMatrix(matrixType, fp, 3);
            ReflectionHelpers.Roundtrip(matrix, 7);
            Type vector3 = Require(assembly, "FPVector3");
            object aabb = NewAABB(aabbType, fp, vector3);
            ReflectionHelpers.Roundtrip(aabb, 0);
            object bounds = NewBounds(boundsType, fp, vector3);
            ReflectionHelpers.Roundtrip(bounds, 0);
        }

        private static void TestPrimitiveSet(Assembly assembly)
        {
            Type type = Require(assembly, "PrimitiveSet");
            object value = ReflectionHelpers.New(type);
            ReflectionHelpers.SetValue(value, "A", (byte)0xAB);
            ReflectionHelpers.SetValue(value, "B", (sbyte)-7);
            ReflectionHelpers.SetValue(value, "C", (short)-1234);
            ReflectionHelpers.SetValue(value, "D", (ushort)54321);
            ReflectionHelpers.SetValue(value, "E", -123456789);
            ReflectionHelpers.SetValue(value, "F", 0xDEADBEEFu);
            ReflectionHelpers.SetValue(value, "G", -1234567890123456789L);
            ReflectionHelpers.SetValue(value, "H", 0xFEDCBA9876543210UL);
            ReflectionHelpers.SetValue(value, "I", true);

            object restored = ReflectionHelpers.Roundtrip(value, 1).Restored;
            TestAssert.Equal((sbyte)-7, Convert.ToSByte(ReflectionHelpers.GetValue(restored, "B"), System.Globalization.CultureInfo.InvariantCulture), "PrimitiveSet.B");
            TestAssert.Equal(-1234567890123456789L, Convert.ToInt64(ReflectionHelpers.GetValue(restored, "G"), System.Globalization.CultureInfo.InvariantCulture), "PrimitiveSet.G");
            TestAssert.Equal(true, Convert.ToBoolean(ReflectionHelpers.GetValue(restored, "I"), System.Globalization.CultureInfo.InvariantCulture), "PrimitiveSet.I");
        }

        private static void TestMathSet(Assembly assembly, Type fp)
        {
            Type type = Require(assembly, "MathSet");
            Type vector2 = Require(assembly, "FPVector2");
            Type vector3 = Require(assembly, "FPVector3");
            Type quaternion = Require(assembly, "FPQuaternion");
            Type matrix = Require(assembly, "FPMatrix4x4");
            Type aabb = Require(assembly, "FPAABB");
            Type bounds = Require(assembly, "FPBounds");
            Type entityRef = Require(assembly, "CYGEntityRef");
            Type playerRef = Require(assembly, "CYGPlayerRef");

            object value = ReflectionHelpers.New(type);
            ReflectionHelpers.SetValue(value, "F", ReflectionHelpers.NewFP(fp, 42));
            ReflectionHelpers.SetValue(value, "V2", ReflectionHelpers.NewVector2(vector2, fp, 1, 2));
            ReflectionHelpers.SetValue(value, "V3", ReflectionHelpers.NewVector3(vector3, fp, 3, 4, 5));
            ReflectionHelpers.SetValue(value, "Q", ReflectionHelpers.NewQuaternion(quaternion, fp, 0, 0, 0, 1));
            ReflectionHelpers.SetValue(value, "M", NewMatrix(matrix, fp, 1));
            ReflectionHelpers.SetValue(value, "Box", NewAABB(aabb, fp, vector3));
            ReflectionHelpers.SetValue(value, "Bounds", NewBounds(bounds, fp, vector3));
            ReflectionHelpers.SetValue(value, "Entity", NewEntityRef(entityRef, 12, 5));
            ReflectionHelpers.SetValue(value, "Player", playerRef.GetConstructor(new[] { typeof(ushort) })!.Invoke(new object[] { (ushort)3 }));

            object restored = ReflectionHelpers.Roundtrip(value, 4).Restored;
            TestAssert.Equal(RawOfInt(fp, 42), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restored, "F")!), "MathSet.F");
            object restoredMatrix = ReflectionHelpers.GetValue(restored, "M")!;
            TestAssert.Equal(RawOfInt(fp, 102), ReflectionHelpers.ReadFPRaw(GetIndexed(restoredMatrix, 2)), "MathSet.M[2] raw");
            object restoredBox = ReflectionHelpers.GetValue(restored, "Box")!;
            object boxMin = ReflectionHelpers.GetValue(restoredBox, "Min")!;
            TestAssert.Equal(RawOfInt(fp, 1), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(boxMin, "x")!), "MathSet.Box.Min.x");
            object restoredEntity = ReflectionHelpers.GetValue(restored, "Entity")!;
            TestAssert.Equal(5, Convert.ToInt32(ReflectionHelpers.GetValue(restoredEntity, "Version"), System.Globalization.CultureInfo.InvariantCulture), "MathSet.Entity.Version");
        }

        private static void TestStringSet(Assembly assembly)
        {
            Type type = Require(assembly, "StringSet");
            object value = ReflectionHelpers.New(type);
            object label = ReflectionHelpers.GetValue(value, "Label")!;
            Invoke(label, "Set", "Cook Your Game");
            ReflectionHelpers.SetValue(value, "Label", label);

            object restored = ReflectionHelpers.Roundtrip(value, 0).Restored;
            object restoredLabel = ReflectionHelpers.GetValue(restored, "Label")!;
            TestAssert.Equal("Cook Your Game", Invoke(restoredLabel, "AsString"), "StringSet.Label");
        }

        private static void TestCollectionSet(Assembly assembly, Type fp)
        {
            Type type = Require(assembly, "CollectionSet");
            object value = ReflectionHelpers.New(type);
            for (int i = 0; i < 8; i++)
            {
                SetElement(value, "Values", i, ReflectionHelpers.NewFP(fp, i + 1));
            }
            for (int i = 0; i < 4; i++)
            {
                AddToList(value, "History", ReflectionHelpers.NewFP(fp, 10 + i));
            }
            SetDictionary(value, "Lookup", 3, ReflectionHelpers.NewFP(fp, 33));
            SetDictionary(value, "Lookup", 9, ReflectionHelpers.NewFP(fp, 99));

            object restored = ReflectionHelpers.Roundtrip(value, 0).Restored;
            object values = ReflectionHelpers.GetValue(restored, "Values")!;
            TestAssert.Equal(RawOfInt(fp, 8), ReflectionHelpers.ReadFPRaw(GetIndexed(values, 7)), "CollectionSet.Values[7]");
            object history = ReflectionHelpers.GetValue(restored, "History")!;
            TestAssert.Equal(4, Convert.ToInt32(ReflectionHelpers.GetValue(history, "_count"), System.Globalization.CultureInfo.InvariantCulture), "CollectionSet.History.Count");
            object found = TryGetDictionary(restored, "Lookup", 9);
            TestAssert.Require(found != null && ReflectionHelpers.ReadFPRaw(found) == RawOfInt(fp, 99), "CollectionSet.Lookup[9]");
        }

        private static void TestAssetSet(Assembly assembly)
        {
            Type type = Require(assembly, "AssetSet");
            object value = ReflectionHelpers.New(type);
            object item = ReflectionHelpers.GetValue(value, "Item")!;
            ReflectionHelpers.SetValue(item, "Id", 42);
            ReflectionHelpers.SetValue(value, "Item", item);

            object restored = ReflectionHelpers.Roundtrip(value, 0).Restored;
            object restoredItem = ReflectionHelpers.GetValue(restored, "Item")!;
            TestAssert.Equal(42, Convert.ToInt32(ReflectionHelpers.GetValue(restoredItem, "Id"), System.Globalization.CultureInfo.InvariantCulture), "AssetSet.Item.Id");
            TestAssert.Equal(false, Invoke(restoredItem, "get_IsNone"), "AssetSet.Item.IsNone");
        }

        private static void TestChoiceUnion(Assembly assembly, Type fp)
        {
            Type type = Require(assembly, "Choice");
            Type one = Require(assembly, "ChoiceOne");
            Type two = Require(assembly, "ChoiceTwo");

            object value = ReflectionHelpers.New(type);
            ReflectionHelpers.SetValue(value, "Tag", (byte)2);
            object second = ReflectionHelpers.New(two);
            ReflectionHelpers.SetValue(second, "Direction", ReflectionHelpers.NewVector2(Require(assembly, "FPVector2"), fp, 6, 7));
            ReflectionHelpers.SetValue(second, "Enabled", true);
            ReflectionHelpers.SetValue(value, "Two", second);

            object restored = ReflectionHelpers.Roundtrip(value, 3).Restored;
            TestAssert.Equal((byte)2, Convert.ToByte(ReflectionHelpers.GetValue(restored, "Tag"), System.Globalization.CultureInfo.InvariantCulture), "Choice.Tag");
            object restoredTwo = ReflectionHelpers.GetValue(restored, "Two")!;
            TestAssert.Equal(true, Convert.ToBoolean(ReflectionHelpers.GetValue(restoredTwo, "Enabled"), System.Globalization.CultureInfo.InvariantCulture), "Choice.Two.Enabled");
            _ = one;
        }

        private static void TestFullState(Assembly assembly, Type fp, Type vector3)
        {
            Type type = Require(assembly, "FullState");
            Type vector2 = Require(assembly, "FPVector2");
            Type quaternion = Require(assembly, "FPQuaternion");
            Type matrix = Require(assembly, "FPMatrix4x4");
            Type aabb = Require(assembly, "FPAABB");
            Type bounds = Require(assembly, "FPBounds");
            Type entityRef = Require(assembly, "CYGEntityRef");
            Type playerRef = Require(assembly, "CYGPlayerRef");

            object value = ReflectionHelpers.New(type);
            ReflectionHelpers.SetValue(value, "F", ReflectionHelpers.NewFP(fp, 8));
            ReflectionHelpers.SetValue(value, "V2", ReflectionHelpers.NewVector2(vector2, fp, 1, 2));
            ReflectionHelpers.SetValue(value, "V3", ReflectionHelpers.NewVector3(vector3, fp, 3, 4, 5));
            ReflectionHelpers.SetValue(value, "Q", ReflectionHelpers.NewQuaternion(quaternion, fp, 0, 0, 0, 1));
            ReflectionHelpers.SetValue(value, "M", NewMatrix(matrix, fp, 2));
            ReflectionHelpers.SetValue(value, "Box", NewAABB(aabb, fp, vector3));
            ReflectionHelpers.SetValue(value, "Bounds", NewBounds(bounds, fp, vector3));
            ReflectionHelpers.SetValue(value, "Entity", NewEntityRef(entityRef, 7, 9));
            ReflectionHelpers.SetValue(value, "Player", playerRef.GetConstructor(new[] { typeof(ushort) })!.Invoke(new object[] { (ushort)2 }));
            object item = ReflectionHelpers.GetValue(value, "Item")!;
            ReflectionHelpers.SetValue(item, "Id", 42);
            ReflectionHelpers.SetValue(value, "Item", item);
            object label = ReflectionHelpers.GetValue(value, "Label")!;
            Invoke(label, "Set", "CYG");
            ReflectionHelpers.SetValue(value, "Label", label);
            for (int i = 0; i < 3; i++)
            {
                SetElement(value, "Cooldowns", i, ReflectionHelpers.NewFP(fp, i + 1));
            }
            for (int i = 0; i < 3; i++)
            {
                AddToList(value, "Players", playerRef.GetConstructor(new[] { typeof(ushort) })!.Invoke(new object[] { (ushort)(i + 1) }));
            }
            SetDictionary(value, "Tuning", 1, ReflectionHelpers.NewFP(fp, 2));
            SetDictionary(value, "Tuning", 8, ReflectionHelpers.NewFP(fp, 9));
            ReflectionHelpers.SetValue(value, "Mode", "One");
            ReflectionHelpers.SetValue(value, "Flags", "First, Third");
            object pick = ReflectionHelpers.GetValue(value, "Pick")!;
            ReflectionHelpers.SetValue(pick, "Tag", (byte)1);
            object pickOne = ReflectionHelpers.GetValue(pick, "One")!;
            ReflectionHelpers.SetValue(pickOne, "Value", ReflectionHelpers.NewFP(fp, 77));
            ReflectionHelpers.SetValue(pick, "One", pickOne);
            ReflectionHelpers.SetValue(value, "Pick", pick);

            ReflectionHelpers.RoundtripResult result = ReflectionHelpers.Roundtrip(value, 7);
            object restored = result.Restored;
            TestAssert.Equal(42, Convert.ToInt32(ReflectionHelpers.GetValue(ReflectionHelpers.GetValue(restored, "Item"), "Id"), System.Globalization.CultureInfo.InvariantCulture), "FullState.Item.Id");
            TestAssert.Equal("CYG", Invoke(ReflectionHelpers.GetValue(restored, "Label")!, "AsString"), "FullState.Label");
            object restoredPlayers = ReflectionHelpers.GetValue(restored, "Players")!;
            TestAssert.Equal(3, Convert.ToInt32(ReflectionHelpers.GetValue(restoredPlayers, "_count"), System.Globalization.CultureInfo.InvariantCulture), "FullState.Players.Count");
            object tuning = TryGetDictionary(restored, "Tuning", 8);
            TestAssert.Require(tuning != null && ReflectionHelpers.ReadFPRaw(tuning) == RawOfInt(fp, 9), "FullState.Tuning[8]");
            object restoredPick = ReflectionHelpers.GetValue(restored, "Pick")!;
            TestAssert.Equal((byte)1, Convert.ToByte(ReflectionHelpers.GetValue(restoredPick, "Tag"), System.Globalization.CultureInfo.InvariantCulture), "FullState.Pick.Tag");
            TestAssert.Equal(RawOfInt(fp, 77), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(ReflectionHelpers.GetValue(restoredPick, "One"), "Value")!), "FullState.Pick.One.Value");
            TestAssert.Equal("One", ReflectionHelpers.GetValue(restored, "Mode")!.ToString(), "FullState.Mode");
            TestAssert.Equal(5, Convert.ToInt32(ReflectionHelpers.GetValue(restored, "Flags"), System.Globalization.CultureInfo.InvariantCulture), "FullState.Flags");
        }

        private static void TestWideInput(Assembly assembly, Type fp)
        {
            Type inputType = Require(assembly, "CYGInput");
            object input = ReflectionHelpers.New(inputType);
            for (int i = 0; i < 9; i++)
            {
                ReflectionHelpers.SetButton(input, "B" + i);
            }
            ReflectionHelpers.SetValue(input, "Axis", ReflectionHelpers.NewFP(fp, -12));

            TestAssert.Equal(0x1FFUL, ReflectionHelpers.ReadButtons(input), "wide input button mask");
            FieldInfo buttonsField = inputType.GetField("_buttons", BindingFlags.Instance | BindingFlags.NonPublic)!;
            TestAssert.Equal(typeof(ushort), buttonsField.FieldType, "9 buttons use ushort storage");

            object restored = ReflectionHelpers.Roundtrip(input, 0).Restored;
            TestAssert.Equal(0x1FFUL, ReflectionHelpers.ReadButtons(restored), "wide input button mask after roundtrip");
            TestAssert.Equal(RawOfInt(fp, -12), ReflectionHelpers.ReadFPRaw(ReflectionHelpers.GetValue(restored, "Axis")!), "wide input Axis");
            for (int i = 0; i < 9; i++)
            {
                TestAssert.Require(ReflectionHelpers.GetButton(restored, "B" + i), "wide input B" + i);
            }
        }

        private static void TestFlags(Assembly assembly)
        {
            Type bits = Require(assembly, "Bits");
            TestAssert.Equal(1, Convert.ToInt32(ReflectionHelpers.ParseEnum(bits, "First"), System.Globalization.CultureInfo.InvariantCulture), "Bits.First");
            TestAssert.Equal(2, Convert.ToInt32(ReflectionHelpers.ParseEnum(bits, "Second"), System.Globalization.CultureInfo.InvariantCulture), "Bits.Second");
            TestAssert.Equal(4, Convert.ToInt32(ReflectionHelpers.ParseEnum(bits, "Third"), System.Globalization.CultureInfo.InvariantCulture), "Bits.Third");
            TestAssert.Equal(0, Convert.ToInt32(ReflectionHelpers.ParseEnum(bits, "None"), System.Globalization.CultureInfo.InvariantCulture), "Bits.None");

            Type small = Require(assembly, "Small");
            TestAssert.Equal(typeof(byte), Enum.GetUnderlyingType(small), "Small underlying type");
            TestAssert.Equal(typeof(ushort), Enum.GetUnderlyingType(bits), "Bits underlying type");
        }

        private static void TestDiagnostics()
        {
            string samples = Path.Combine(_repoRoot, "sdk/unity/Samples/CYG");

            CygModule badFloat = ParseFile(Path.Combine(samples, "bad_float.cyg"));
            TestAssert.Equal(1, ErrorCount(badFloat), "bad_float error count");
            ExpectError(badFloat, CygDiagnosticIds.Float);

            CygModule badReference = ParseFile(Path.Combine(samples, "bad_reference.cyg"));
            TestAssert.Equal(2, ErrorCount(badReference), "bad_reference error count");
            ExpectError(badReference, CygDiagnosticIds.ReferenceType);
            ExpectError(badReference, CygDiagnosticIds.StandardCollection);

            CygModule badDuplicate = ParseFile(Path.Combine(samples, "bad_duplicate_id.cyg"));
            ExpectError(badDuplicate, CygDiagnosticIds.ComponentId);

            CygModule synced = ParseSource("namespace Test; event E : 1000001 { Int32 Synced; }");
            ExpectError(synced, CygDiagnosticIds.ReservedName);

            CygModule builtin = ParseSource("namespace Test; struct FP { Int32 A; }");
            ExpectError(builtin, CygDiagnosticIds.ReservedName);

            CygModule tagVariant = ParseSource("namespace Test; union U { struct Tag { Int32 A; } struct B { Int32 B; } }");
            ExpectError(tagVariant, CygDiagnosticIds.ReservedName);

            CygModule singleVariant = ParseSource("namespace Test; union U { struct A { Int32 A; } }");
            ExpectError(singleVariant, CygDiagnosticIds.Union);

            CygModule unsupported = ParseSource("namespace Test; component C : 1001 { Int32 A; } rpc DoIt(Int32 value);");
            ExpectError(unsupported, CygDiagnosticIds.UnsupportedType);

            CygModule badEventId = ParseSource("namespace Test; event E : 1000001 { Int32 A; } event F : 999 { Int32 B; }");
            ExpectError(badEventId, CygDiagnosticIds.EventId);

            const string derivedSource = "namespace Test; component Derived { Int32 A; } synced_event Changed { Int32 B; }";
            CygModule first = ParseSource(derivedSource);
            CygModule second = ParseSource(derivedSource);
            TestAssert.Require(!first.HasErrors, "derived ids source must not have errors");
            TestAssert.Equal(first.Components.First().Id, second.Components.First().Id, "derived component id stability");
            TestAssert.Equal(first.Events.First().Id, second.Events.First().Id, "derived event id stability");
            TestAssert.Require(first.Diagnostics.Any(d => d.Id == CygDiagnosticIds.MissingExplicitId && d.Severity == CygSeverity.Warning),
                "derived ids must warn with CYGD116");

            var emitter = new CygCSharpEmitter();
            CygEmitResult refused = emitter.Emit(badFloat, new CygEmitOptions());
            TestAssert.Equal(0, refused.Files.Count, "emitter must refuse modules with errors");
            TestAssert.Require(refused.Diagnostics.Any(d => d.Id == CygDiagnosticIds.Float), "emitter must forward front-end errors");
        }

        private static Assembly CompileSample(string fileName, string assemblyName)
        {
            string filePath = Path.Combine(_repoRoot, "sdk/unity/Samples/CYG", fileName);
            CygModule module = ParseFile(filePath);
            TestAssert.Require(!module.HasErrors,
                fileName + " must parse cleanly, got: " + string.Join(" | ", module.Diagnostics.Select(d => d.ToString())));

            var emitter = new CygCSharpEmitter();
            CygEmitResult emitted = emitter.Emit(module, new CygEmitOptions());
            TestAssert.Require(!emitted.Diagnostics.Any(d => d.Severity == CygSeverity.Error),
                fileName + " must emit cleanly, got: " + string.Join(" | ", emitted.Diagnostics.Select(d => d.ToString())));
            TestAssert.Equal(1, emitted.Files.Count, fileName + " emitted file count");

            string generated = emitted.Files[0].Content;
            string debugPath = Path.Combine(Path.GetTempPath(), assemblyName + ".g.cs");
            File.WriteAllText(debugPath, generated);

            CSharpParseOptions parseOptions = new CSharpParseOptions(LanguageVersion.CSharp9);
            List<SyntaxTree> trees = LoadRuntimeSources(parseOptions);
            trees.Add(CSharpSyntaxTree.ParseText(generated, parseOptions, path: filePath + ".g.cs"));

            IEnumerable<MetadataReference> references = TrustedPlatformReferences();
            CSharpCompilation compilation = CSharpCompilation.Create(
                assemblyName,
                trees,
                references,
                new CSharpCompilationOptions(
                    OutputKind.DynamicallyLinkedLibrary,
                    optimizationLevel: OptimizationLevel.Debug,
                    nullableContextOptions: NullableContextOptions.Disable));

            GeneratorDriver driver = CSharpGeneratorDriver.Create(
                new ISourceGenerator[] { new CYGComponentGenerator() },
                parseOptions: parseOptions);
            driver = driver.RunGeneratorsAndUpdateCompilation(compilation, out Compilation output, out ImmutableArray<Diagnostic> generatorDiagnostics);

            Diagnostic[] errors = output.GetDiagnostics()
                .Concat(generatorDiagnostics)
                .Where(d => d.Severity == DiagnosticSeverity.Error)
                .ToArray();
            if (errors.Length > 0)
            {
                throw new CygTestException(
                    fileName + " generated C# does not compile:\n"
                    + FormatErrors(errors, output)
                    + "\nGenerated source: " + debugPath);
            }

            using (var stream = new MemoryStream())
            {
                Microsoft.CodeAnalysis.Emit.EmitResult result = output.Emit(stream);
                if (!result.Success)
                {
                    throw new CygTestException(
                        fileName + " emitted assembly failed: "
                        + string.Join(" | ", result.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error)));
                }
                return Assembly.Load(stream.ToArray());
            }
        }

        private static string FormatErrors(Diagnostic[] errors, Compilation output)
        {
            var sb = new StringBuilder();
            foreach (Diagnostic error in errors.Take(15))
            {
                sb.AppendLine("  " + error);
                FileLinePositionSpan span = error.Location.GetLineSpan();
                if (error.Location.SourceTree != null && span.IsValid)
                {
                    SourceText text = error.Location.SourceTree.GetText();
                    int line = span.StartLinePosition.Line;
                    int first = Math.Max(0, line - 2);
                    int last = Math.Min(text.Lines.Count - 1, line + 2);
                    for (int i = first; i <= last; i++)
                    {
                        sb.AppendLine("    " + (i + 1).ToString().PadLeft(4) + ": " + text.Lines[i]);
                    }
                }
            }
            _ = output;
            return sb.ToString();
        }

        private static List<SyntaxTree> LoadRuntimeSources(CSharpParseOptions parseOptions)
        {
            string runtime = Path.Combine(_repoRoot, "sdk/unity/Runtime/Sim");
            var directories = new[]
            {
                Path.Combine(runtime, "Core"),
                Path.Combine(runtime, "Math"),
                Path.Combine(runtime, "Physics3D")
            };

            var trees = new List<SyntaxTree>();
            foreach (string directory in directories)
            {
                foreach (string file in Directory.GetFiles(directory, "*.cs").OrderBy(f => f, StringComparer.Ordinal))
                {
                    trees.Add(CSharpSyntaxTree.ParseText(File.ReadAllText(file), parseOptions, path: file));
                }
            }

            string world = Path.Combine(runtime, "World");
            foreach (string file in Directory.GetFiles(world, "*.cs").OrderBy(f => f, StringComparer.Ordinal))
            {
                trees.Add(CSharpSyntaxTree.ParseText(File.ReadAllText(file), parseOptions, path: file));
            }

            return trees;
        }

        private static IEnumerable<MetadataReference> TrustedPlatformReferences()
        {
            string trusted = (string?)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") ?? string.Empty;
            return trusted
                .Split(Path.PathSeparator)
                .Where(File.Exists)
                .Select(path => (MetadataReference)MetadataReference.CreateFromFile(path));
        }

        private static CygModule ParseFile(string path)
        {
            return new CygFrontend().Parse(File.ReadAllText(path), path);
        }

        private static CygModule ParseSource(string source)
        {
            return new CygFrontend().Parse(source, "test.cyg");
        }

        private static int ErrorCount(CygModule module)
        {
            return module.Diagnostics.Count(d => d.Severity == CygSeverity.Error);
        }

        private static void ExpectError(CygModule module, string id)
        {
            TestAssert.Require(
                module.Diagnostics.Any(d => d.Id == id && d.Severity == CygSeverity.Error),
                "expected error " + id + ", got: "
                + (module.Diagnostics.Count == 0 ? "<none>" : string.Join(" | ", module.Diagnostics.Select(d => d.ToString()))));
        }

        private static Type Require(Assembly assembly, string name)
        {
            return ReflectionHelpers.RequireType(assembly, name, name);
        }

        private static uint ReadStaticUInt(Type type, string name)
        {
            FieldInfo field = type.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static)!;
            if (field == null)
            {
                throw new CygTestException(type.FullName + " has no static field " + name);
            }
            return Convert.ToUInt32(field.GetValue(null), System.Globalization.CultureInfo.InvariantCulture);
        }

        private static bool ReadStaticBool(Type type, string name)
        {
            FieldInfo field = type.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static)!;
            if (field == null)
            {
                throw new CygTestException(type.FullName + " has no static field " + name);
            }
            return Convert.ToBoolean(field.GetValue(null), System.Globalization.CultureInfo.InvariantCulture);
        }

        private static long RawOfInt(Type fpType, int value)
        {
            return ReflectionHelpers.ReadFPRaw(ReflectionHelpers.NewFP(fpType, value));
        }

        private static object NewEntityRef(Type type, int index, int version)
        {
            object value = ReflectionHelpers.New(type);
            ReflectionHelpers.SetValue(value, "Index", index);
            ReflectionHelpers.SetValue(value, "Version", version);
            return value;
        }

        private static object NewMatrix(Type matrixType, Type fpType, int seed)
        {
            object matrix = ReflectionHelpers.New(matrixType);
            PropertyInfo indexer = matrixType.GetProperties()
                .First(p => p.GetIndexParameters().Length == 1 && p.GetIndexParameters()[0].ParameterType == typeof(int));
            for (int i = 0; i < 16; i++)
            {
                indexer.SetValue(matrix, ReflectionHelpers.NewFP(fpType, (seed * 100) + i), new object[] { i });
            }
            return matrix;
        }

        private static object NewAABB(Type aabbType, Type fpType, Type vector3Type)
        {
            object aabb = ReflectionHelpers.New(aabbType);
            ReflectionHelpers.SetValue(aabb, "Min", ReflectionHelpers.NewVector3(vector3Type, fpType, 1, 2, 3));
            ReflectionHelpers.SetValue(aabb, "Max", ReflectionHelpers.NewVector3(vector3Type, fpType, 4, 5, 6));
            return aabb;
        }

        private static object NewBounds(Type boundsType, Type fpType, Type vector3Type)
        {
            object bounds = ReflectionHelpers.New(boundsType);
            ReflectionHelpers.SetValue(bounds, "Center", ReflectionHelpers.NewVector3(vector3Type, fpType, 7, 8, 9));
            ReflectionHelpers.SetValue(bounds, "Extents", ReflectionHelpers.NewVector3(vector3Type, fpType, 1, 2, 3));
            return bounds;
        }

        private static object GetIndexed(object container, int index)
        {
            PropertyInfo indexer = container.GetType().GetProperties()
                .First(p => p.GetIndexParameters().Length == 1 && p.GetIndexParameters()[0].ParameterType == typeof(int));
            return indexer.GetValue(container, new object[] { index })!;
        }

        private static void SetElement(object owner, string fieldName, int index, object value)
        {
            if (!ReflectionHelpers.TrySetIndexed(owner, fieldName, index, value))
            {
                throw new CygTestException(owner.GetType().Name + "." + fieldName + " has no writable indexer.");
            }
        }

        private static void AddToList(object owner, string fieldName, object value)
        {
            object list = ReflectionHelpers.GetValue(owner, fieldName)!;
            MethodInfo add = list.GetType().GetMethod("Add", new[] { value.GetType() })!;
            object result = add.Invoke(list, new[] { value })!;
            TestAssert.Equal(true, Convert.ToBoolean(result, System.Globalization.CultureInfo.InvariantCulture),
                owner.GetType().Name + "." + fieldName + ".Add");
            ReflectionHelpers.SetValue(owner, fieldName, list);
        }

        private static void SetDictionary(object owner, string fieldName, object key, object value)
        {
            object dictionary = ReflectionHelpers.GetValue(owner, fieldName)!;
            MethodInfo set = dictionary.GetType().GetMethods()
                .First(m => m.Name == "Set" && m.GetParameters().Length == 2);
            ParameterInfo[] parameters = set.GetParameters();
            object convertedKey = Convert.ChangeType(key, parameters[0].ParameterType, System.Globalization.CultureInfo.InvariantCulture);
            set.Invoke(dictionary, new[] { convertedKey, value });
            ReflectionHelpers.SetValue(owner, fieldName, dictionary);
        }

        private static object? TryGetDictionary(object owner, string fieldName, object key)
        {
            object dictionary = ReflectionHelpers.GetValue(owner, fieldName)!;
            MethodInfo tryGet = dictionary.GetType().GetMethods()
                .First(m => m.Name == "TryGetValue" && m.GetParameters().Length == 2);
            ParameterInfo[] parameters = tryGet.GetParameters();
            object convertedKey = Convert.ChangeType(key, parameters[0].ParameterType, System.Globalization.CultureInfo.InvariantCulture);
            Type valueType = parameters[1].ParameterType;
            if (valueType.IsByRef)
            {
                valueType = valueType.GetElementType()!;
            }
            object[] args = { convertedKey, Activator.CreateInstance(valueType)! };
            object found = tryGet.Invoke(dictionary, args)!;
            return Convert.ToBoolean(found, System.Globalization.CultureInfo.InvariantCulture) ? args[1] : null;
        }

        private static object? Invoke(object target, string methodName, params object?[] args)
        {
            MethodInfo method = target.GetType().GetMethods()
                .FirstOrDefault(m => m.Name == methodName && !m.IsStatic && m.GetParameters().Length == args.Length)
                ?? throw new CygTestException(target.GetType().FullName + " has no method " + methodName);
            return method.Invoke(target, args);
        }

        private static void CopyViaCodec(object source, object destination)
        {
            int size = ReflectionHelpers.ReadSize(source.GetType());
            byte[] buffer = new byte[size];
            ReflectionHelpers.Write(source, buffer, 0);
            ReflectionHelpers.Read(destination, buffer, 0);
        }

        private static string FindRepoRoot()
        {
            DirectoryInfo? directory = new DirectoryInfo(AppContext.BaseDirectory);
            while (directory != null)
            {
                if (Directory.Exists(Path.Combine(directory.FullName, "sdk/unity/Runtime/Sim")))
                {
                    return directory.FullName;
                }
                directory = directory.Parent;
            }

            throw new CygTestException("could not find the repository root from " + AppContext.BaseDirectory);
        }
    }
}
