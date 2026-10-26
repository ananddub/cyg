using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Records a collider's world-pose history for server-side rewind queries.</summary>
    public sealed class CYGRollbackBuffer : MonoBehaviour
    {
        /// <summary>One world-space pose sample of the collider.</summary>
        public struct Snapshot
        {
            /// <summary>Local capture time in seconds.</summary>
            public double Timestamp;
            /// <summary>Collider centre in world space.</summary>
            public Vector3 Center;
            /// <summary>Full world-space extents of the collider.</summary>
            public Vector3 Extents;
            /// <summary>True when the collider is a SphereCollider (exact ray-sphere rewind).</summary>
            public bool IsSphere;
        }

        /// <summary>Number of stored samples; 120 covers roughly 2 seconds at 60 Hz.</summary>
        public int capacity = 120;

        double[] timestamps;
        Vector3[] centers;
        Vector3[] extents;
        bool[] spheres;
        int head;
        int count;
        double lastCaptureTime;
        Collider collider;

        void Awake()
        {
            collider = GetComponent<Collider>();
            if (collider == null)
            {
                Debug.LogError($"CYGRollbackBuffer on '{gameObject.name}' requires a Collider; disabled.", this);
                enabled = false;
                return;
            }

            if (capacity < 2)
            {
                capacity = 2;
            }

            timestamps = new double[capacity];
            centers = new Vector3[capacity];
            extents = new Vector3[capacity];
            spheres = new bool[capacity];
        }

        void OnEnable()
        {
            if (collider != null)
            {
                CYGLagCompensation.RegisterBuffer(collider, this);
            }
        }

        void OnDisable()
        {
            if (collider != null)
            {
                CYGLagCompensation.UnregisterBuffer(collider);
            }
        }

        void FixedUpdate()
        {
            if (collider == null)
            {
                return;
            }

            double now = Time.time + CYGLagCompensation.NowOffset;
            if (now - lastCaptureTime < CaptureIntervalSeconds)
            {
                return;
            }

            lastCaptureTime = now;

            Bounds bounds = collider.bounds;
            timestamps[head] = now;
            centers[head] = bounds.center;
            extents[head] = bounds.extents;
            spheres[head] = collider is SphereCollider;

            head = (head + 1) % capacity;
            if (count < capacity)
            {
                count++;
            }
        }

        /// <summary>Rebuilds the collider pose at a past time, interpolating the bracketing samples.</summary>
        public bool TrySampleInterpolated(double time, out Snapshot sample)
        {
            sample = default;
            if (count == 0)
            {
                return false;
            }

            int start = (head - count + capacity) % capacity;
            int newest = (head + capacity - 1) % capacity;

            // WHY: rewinding before recorded history would be guesswork, so out-of-range-early fails;
            // slightly newer than the last sample clamps to it (capture trails the live clock by a tick).
            if (time < timestamps[start])
            {
                return false;
            }

            if (time >= timestamps[newest])
            {
                FillSample(newest, newest, 0f, time, ref sample);
                return true;
            }

            int second = start;
            for (int i = 0; i < count; i++)
            {
                int index = (start + i) % capacity;
                if (timestamps[index] >= time)
                {
                    second = index;
                    break;
                }
            }

            if (second == start)
            {
                FillSample(start, start, 0f, time, ref sample);
                return true;
            }

            int first = (second + capacity - 1) % capacity;
            double span = timestamps[second] - timestamps[first];
            float t = span <= 0.0 ? 0f : (float)((time - timestamps[first]) / span);
            FillSample(first, second, t, time, ref sample);
            return true;
        }

        void FillSample(int first, int second, float t, double time, ref Snapshot sample)
        {
            sample.Timestamp = time;
            sample.Center = Vector3.Lerp(centers[first], centers[second], t);
            sample.Extents = Vector3.Lerp(extents[first], extents[second], t);
            sample.IsSphere = spheres[first] || spheres[second];
        }

        // WHY: 1/60 s capture ceiling so history stays compact regardless of the project's fixed timestep.
        const double CaptureIntervalSeconds = 1.0 / 60.0;
    }

    /// <summary>Client-side hit-claim capture helpers for lag compensation.</summary>
    public static class CYGLagCompensation
    {
        /// <summary>Rewinds older than this are rejected outright.</summary>
        public const float MaxRewindWindowSeconds = 0.3f;

        /// <summary>Offset added to local Time.time to approximate the authority clock.</summary>
        public static double NowOffset;

        static readonly Dictionary<Collider, CYGRollbackBuffer> buffers = new Dictionary<Collider, CYGRollbackBuffer>();
        static CYGBitWriter s_writer = new CYGBitWriter();

        /// <summary>Adds a collider's history buffer to the rewind index.</summary>
        public static void RegisterBuffer(Collider collider, CYGRollbackBuffer buffer)
        {
            if (collider != null && buffer != null)
            {
                buffers[collider] = buffer;
            }
        }

        /// <summary>Removes a collider from the rewind index.</summary>
        public static void UnregisterBuffer(Collider collider)
        {
            if (collider != null)
            {
                buffers.Remove(collider);
            }
        }

        /// <summary>Clears the rewind index, e.g. between rooms.</summary>
        public static void ClearBuffers()
        {
            buffers.Clear();
        }

        /// <summary>Shoots a ray against colliders reconstructed at a past time, layer-masked, allocation-free.</summary>
        /// <remarks>RaycastHit exposes no writable collider or transform, so the hit collider is returned beside it.</remarks>
        public static bool RaycastRewind(Ray ray, float maxDistance, double clientTime, int layerMask,
            out RaycastHit hit, out Collider hitCollider, out double hitTimestamp)
        {
            hit = default;
            hitCollider = null;
            hitTimestamp = clientTime;

            double now = Time.time + NowOffset;
            if (clientTime > now || now - clientTime > MaxRewindWindowSeconds)
            {
                return false;
            }

            Vector3 origin = ray.origin;
            Vector3 direction = ray.direction.normalized;

            float bestDistance = float.MaxValue;
            Collider bestCollider = null;
            Vector3 bestPoint = Vector3.zero;
            Vector3 bestNormal = Vector3.zero;
            double bestTime = clientTime;

            foreach (KeyValuePair<Collider, CYGRollbackBuffer> pair in buffers)
            {
                Collider collider = pair.Key;
                if (collider == null || !collider.enabled || !collider.gameObject.activeInHierarchy)
                {
                    continue;
                }

                if (layerMask != 0 && (layerMask & (1 << collider.gameObject.layer)) == 0)
                {
                    continue;
                }

                if (!pair.Value.TrySampleInterpolated(clientTime, out CYGRollbackBuffer.Snapshot sample))
                {
                    continue;
                }

                float distance;
                Vector3 normal;
                if (sample.IsSphere)
                {
                    float radius = sample.Extents.x;
                    if (!TryRaySphere(origin, direction, sample.Center, radius, maxDistance, out distance, out normal))
                    {
                        continue;
                    }
                }
                else
                {
                    if (!TryRayAabb(origin, direction, sample.Center - sample.Extents, sample.Center + sample.Extents,
                        maxDistance, out distance, out normal))
                    {
                        continue;
                    }
                }

                if (distance < bestDistance)
                {
                    bestDistance = distance;
                    bestCollider = collider;
                    bestPoint = origin + direction * distance;
                    bestNormal = normal;
                    bestTime = sample.Timestamp;
                }
            }

            if (bestCollider == null)
            {
                return false;
            }

            hit = new RaycastHit
            {
                point = bestPoint,
                normal = bestNormal,
                distance = bestDistance,
            };
            hitCollider = bestCollider;
            hitTimestamp = bestTime;
            return true;
        }

        /// <summary>Packs and sends a rewind hit claim for the server to validate.</summary>
        public static void SubmitHitClaim(double clientTime, Vector3 shotOrigin, Vector3 shotDir, ushort targetViewId)
        {
            if (!CYGNetwork.IsConnected)
            {
                return;
            }

            double now = Time.time + NowOffset;
            if (now - clientTime > MaxRewindWindowSeconds)
            {
                Debug.LogWarning($"CYGLagCompensation: hit claim at {clientTime:F3} is {(now - clientTime):F3}s old, beyond the {MaxRewindWindowSeconds}s rewind window; rejected.");
                return;
            }

            // WHY: no WriteDouble in the bit writer, so the absolute clock time travels as a float;
            // it comfortably covers session-length timestamps and matches the 0.3 s rewind tolerance.
            s_writer.Reset();
            s_writer.WriteSingle((float)clientTime);
            s_writer.WriteVector3(shotOrigin);
            s_writer.WriteVector3(shotDir);
            s_writer.WriteInt32(targetViewId);
            CYGNetwork.SendFrame(CYGOpCode.RewindHit, s_writer.ToArraySegment());
        }

        // WHY: slab method - clip the parametric ray against each axis' near/far planes and keep the
        // widest-in / narrowest-out interval; the entry axis whose plane wrote tMin owns the face normal.
        static bool TryRayAabb(Vector3 origin, Vector3 direction, Vector3 min, Vector3 max, float maxDistance,
            out float distance, out Vector3 normal)
        {
            distance = 0f;
            normal = Vector3.zero;

            float tMin = 0f;
            float tMax = maxDistance;
            int entryAxis = -1;
            bool entryMinSide = false;

            for (int axis = 0; axis < 3; axis++)
            {
                float o = GetAxis(origin, axis);
                float d = GetAxis(direction, axis);
                float planeMin = GetAxis(min, axis);
                float planeMax = GetAxis(max, axis);

                if (Mathf.Abs(d) < Epsilon)
                {
                    if (o < planeMin || o > planeMax)
                    {
                        return false;
                    }

                    continue;
                }

                float invD = 1f / d;
                float tNear = (planeMin - o) * invD;
                float tFar = (planeMax - o) * invD;
                bool nearIsMin = tNear < tFar;
                if (!nearIsMin)
                {
                    (tNear, tFar) = (tFar, tNear);
                }

                if (tNear > tMin)
                {
                    tMin = tNear;
                    entryAxis = axis;
                    entryMinSide = nearIsMin;
                }

                if (tFar < tMax)
                {
                    tMax = tFar;
                }

                if (tMin > tMax)
                {
                    return false;
                }
            }

            distance = tMin;
            if (entryAxis < 0)
            {
                normal = -direction;
            }
            else
            {
                float sign = entryMinSide ? -1f : 1f;
                normal = new Vector3(entryAxis == 0 ? sign : 0f, entryAxis == 1 ? sign : 0f, entryAxis == 2 ? sign : 0f);
            }

            return true;
        }

        static bool TryRaySphere(Vector3 origin, Vector3 direction, Vector3 center, float radius, float maxDistance,
            out float distance, out Vector3 normal)
        {
            distance = 0f;
            normal = Vector3.zero;

            Vector3 toCenter = origin - center;
            float b = Vector3.Dot(direction, toCenter);
            float c = Vector3.Dot(toCenter, toCenter) - radius * radius;
            float discriminant = b * b - c;
            if (discriminant < 0f)
            {
                return false;
            }

            float t = -b - Mathf.Sqrt(discriminant);
            if (t < 0f)
            {
                // A negative near root has two causes and they need different answers: c is
                // negative exactly when the origin is already inside the radius, otherwise both
                // roots are behind the shooter and the shape must be ignored, not reported as a
                // hit at the muzzle.
                if (c >= 0f)
                {
                    return false;
                }

                t = 0f;
            }

            if (t > maxDistance)
            {
                return false;
            }

            distance = t;
            Vector3 point = origin + direction * t;
            Vector3 radial = point - center;
            normal = radial.sqrMagnitude > Epsilon ? radial / radial.magnitude : -direction;
            return true;
        }

        static float GetAxis(Vector3 value, int axis)
        {
            return axis == 0 ? value.x : (axis == 1 ? value.y : value.z);
        }

        const float Epsilon = 1e-6f;
    }
}
