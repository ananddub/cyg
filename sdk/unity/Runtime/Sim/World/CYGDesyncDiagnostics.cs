using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Turns a snapshot mismatch into an actionable report: the first divergent
    /// entity slot, component store and RNG state. Diagnostics only; it
    /// allocates strings and must not run on the normal tick path.
    /// </summary>
    public static class CYGDesyncDiagnostics
    {
        public static string Compare(
            int expectedTick,
            CYGWorldSnapshot expected,
            int actualTick,
            CYGWorldSnapshot actual)
        {
            if (expected == null)
            {
                throw new ArgumentNullException(nameof(expected));
            }
            if (actual == null)
            {
                throw new ArgumentNullException(nameof(actual));
            }

            string prefix = $"[CYG DESYNC] tick {expectedTick}/{actualTick}";
            if (expected.Rng != actual.Rng)
            {
                return prefix + $": rng state differs (expected 0x{expected.Rng.State:X16}/0x{expected.Rng.Increment:X16}, "
                    + $"actual 0x{actual.Rng.State:X16}/0x{actual.Rng.Increment:X16})";
            }
            if (expected.NextIndex != actual.NextIndex)
            {
                return prefix + $": slot high-water mark differs (expected {expected.NextIndex}, actual {actual.NextIndex})";
            }

            int slots = expected.NextIndex;
            for (int i = 0; i < slots; i++)
            {
                bool expectedAlive = i < expected.Alive.Length && expected.Alive[i];
                bool actualAlive = i < actual.Alive.Length && actual.Alive[i];
                if (expectedAlive != actualAlive)
                {
                    return prefix + $": slot {i} alive differs (expected {expectedAlive}, actual {actualAlive})";
                }
                if (expectedAlive)
                {
                    int expectedVersion = i < expected.Versions.Length ? expected.Versions[i] : 0;
                    int actualVersion = i < actual.Versions.Length ? actual.Versions[i] : 0;
                    if (expectedVersion != actualVersion)
                    {
                        return prefix + $": slot {i} version differs (expected {expectedVersion}, actual {actualVersion})";
                    }
                }
            }

            if (expected.Count != actual.Count)
            {
                return prefix + $": entity count differs (expected {expected.Count}, actual {actual.Count})";
            }
            if (expected.StructuralVersion != actual.StructuralVersion)
            {
                return prefix + $": structural version differs (expected {expected.StructuralVersion}, actual {actual.StructuralVersion})";
            }

            if (expected.FreeCount != actual.FreeCount)
            {
                return prefix + $": free-list length differs (expected {expected.FreeCount}, actual {actual.FreeCount})";
            }
            for (int i = 0; i < expected.FreeCount; i++)
            {
                int expectedSlot = i < expected.Free.Length ? expected.Free[i] : -1;
                int actualSlot = i < actual.Free.Length ? actual.Free[i] : -1;
                if (expectedSlot != actualSlot)
                {
                    return prefix + $": free-list position {i} differs (expected slot {expectedSlot}, actual slot {actualSlot})";
                }
            }

            if (expected.Stores.Count != actual.Stores.Count)
            {
                return prefix + $": store count differs (expected {expected.Stores.Count}, actual {actual.Stores.Count})";
            }

            for (int i = 0; i < expected.Stores.Count; i++)
            {
                CYGComponentStoreSnapshot expectedStore = expected.Stores[i];
                CYGComponentStoreSnapshot actualStore = actual.Stores[i];
                if (expectedStore == null || actualStore == null)
                {
                    if (!ReferenceEquals(expectedStore, actualStore))
                    {
                        return prefix + $": store {i} presence differs";
                    }
                    continue;
                }
                if (expectedStore.TypeId != actualStore.TypeId
                    || expectedStore.SchemaHash != actualStore.SchemaHash)
                {
                    return prefix + $": store {i} schema differs (expected {expectedStore.TypeId}/0x{expectedStore.SchemaHash:X8}, "
                        + $"actual {actualStore.TypeId}/0x{actualStore.SchemaHash:X8})";
                }
                if (expectedStore.Count != actualStore.Count)
                {
                    return prefix + $": component {expectedStore.TypeId} count differs (expected {expectedStore.Count}, actual {actualStore.Count})";
                }
                if (expectedStore.ComputeChecksum() == actualStore.ComputeChecksum())
                {
                    continue;
                }

                int slot = expectedStore.FindFirstDifference(actualStore);
                if (slot >= 0)
                {
                    return prefix + $": component {expectedStore.TypeId} first differs at slot {slot}";
                }
                return prefix + $": component {expectedStore.TypeId} checksum differs (value checksums collided)";
            }

            return null;
        }
    }
}
