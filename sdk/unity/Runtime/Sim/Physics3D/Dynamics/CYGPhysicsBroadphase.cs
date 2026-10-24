//-----------------------------------------------------------------------------
// CYGPhysicsBroadphase.cs
// Deterministic sweep-and-prune broadphase, manifold generation and ordered
// collision/trigger event emission.
// Zero UnityEngine dependencies. All math is Q32.32 fixed-point.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysicsWorld
    {
        private void BuildBroadphase()
        {
            _orderCount = 0;

            for (int id = 0; id < _bodies.Count; id++)
            {
                CYGPhysicsBody body = _bodies[id];
                if (body == null || !body.IsActive)
                    continue;
                _aabbs[id] = body.GetWorldAABB();
                _order[_orderCount++] = id;
            }

            // Insertion sort is allocation-free and deterministic. The body id is
            // the tie-break for equal minimum X coordinates.
            for (int i = 1; i < _orderCount; i++)
            {
                int item = _order[i];
                FPAABB itemAabb = _aabbs[item];
                int j = i - 1;
                while (j >= 0)
                {
                    int previous = _order[j];
                    FPAABB previousAabb = _aabbs[previous];
                    if (previousAabb.Min.x < itemAabb.Min.x)
                        break;
                    if (previousAabb.Min.x == itemAabb.Min.x && previous < item)
                        break;
                    _order[j + 1] = previous;
                    j--;
                }
                _order[j + 1] = item;
            }

            _pairCount = 0;

            for (int i = 0; i < _orderCount; i++)
            {
                int idA = _order[i];
                CYGPhysicsBody bodyA = _bodies[idA];
                FPAABB aabbA = _aabbs[idA];

                for (int j = i + 1; j < _orderCount; j++)
                {
                    int idB = _order[j];
                    FPAABB aabbB = _aabbs[idB];

                    if (aabbB.Min.x > aabbA.Max.x)
                        break;

                    if (!aabbA.Intersects(aabbB))
                        continue;

                    CYGPhysicsBody bodyB = _bodies[idB];
                    if (!CanInteract(bodyA, bodyB))
                        continue;

                    int keyA = idA < idB ? idA : idB;
                    int keyB = idA < idB ? idB : idA;
                    AppendPair(MakePairKey(keyA, keyB));
                }
            }

            SortPairs();
        }

        private void BuildManifoldsAndTriggerPairs()
        {
            _manifoldCount = 0;
            _contactKeyCount = 0;
            _triggerKeyCount = 0;

            for (int i = 0; i < _pairCount; i++)
            {
                long key = _pairs[i];
                CYGPhysicsBody bodyA = _bodies[PairBodyA(key)];
                CYGPhysicsBody bodyB = _bodies[PairBodyB(key)];

                bool anyTrigger = bodyA.Collider.IsTrigger || bodyB.Collider.IsTrigger;
                if (anyTrigger)
                {
                    if (CYGPhysicsNarrowphase.Overlaps(bodyA, bodyB))
                        AppendTriggerKey(key);
                    continue;
                }

                CYGManifold manifold;
                if (!CYGPhysicsNarrowphase.TryGetManifold(bodyA, bodyB, out manifold))
                    continue;

                EnsureArraySize(ref _manifolds, _manifoldCount + 1);
                _manifolds[_manifoldCount++] = manifold;
                AppendContactKey(key);
            }
        }

        private void WakeBodiesFromManifolds()
        {
            for (int i = 0; i < _manifoldCount; i++)
            {
                CYGPhysicsBody bodyA = _bodies[_manifolds[i].BodyA];
                CYGPhysicsBody bodyB = _bodies[_manifolds[i].BodyB];

                bool moverA = IsActiveMover(bodyA);
                bool moverB = IsActiveMover(bodyB);

                if (moverA && bodyB.Type == CYGBodyType.Dynamic && !bodyB.IsAwake)
                    bodyB.Wake();
                if (moverB && bodyA.Type == CYGBodyType.Dynamic && !bodyA.IsAwake)
                    bodyA.Wake();
            }
        }

        private void EmitContactEnterStayEvents()
        {
            for (int i = 0; i < _manifoldCount; i++)
            {
                long key = _contactKeys[i];
                CYGManifold manifold = _manifolds[i];
                CYGContactPoint point = manifold.PointCount > 0 ? manifold.GetPoint(0) : default(CYGContactPoint);

                bool stayed = ContainsSortedKey(_prevContactKeys, _prevContactKeyCount, key);
                AddEvent(
                    stayed ? CYGPhysicsEventType.CollisionStay : CYGPhysicsEventType.CollisionEnter,
                    manifold.BodyA,
                    manifold.BodyB,
                    point.Point,
                    manifold.Normal,
                    manifold.MaxDepth);
            }
        }

        private void EmitContactExitEvents()
        {
            for (int i = 0; i < _prevContactKeyCount; i++)
            {
                long key = _prevContactKeys[i];
                if (ContainsSortedKey(_contactKeys, _contactKeyCount, key))
                    continue;

                AddEvent(
                    CYGPhysicsEventType.CollisionExit,
                    PairBodyA(key),
                    PairBodyB(key),
                    FPVector3.Zero,
                    FPVector3.Up,
                    FP.Zero);
            }
        }

        private void EmitTriggerEnterStayEvents()
        {
            for (int i = 0; i < _triggerKeyCount; i++)
            {
                long key = _triggerKeys[i];
                bool stayed = ContainsSortedKey(_prevTriggerKeys, _prevTriggerKeyCount, key);
                EmitTriggerEvent(
                    stayed ? CYGPhysicsEventType.TriggerStay : CYGPhysicsEventType.TriggerEnter,
                    key);
            }
        }

        private void EmitTriggerExitEvents()
        {
            for (int i = 0; i < _prevTriggerKeyCount; i++)
            {
                long key = _prevTriggerKeys[i];
                if (ContainsSortedKey(_triggerKeys, _triggerKeyCount, key))
                    continue;
                EmitTriggerEvent(CYGPhysicsEventType.TriggerExit, key);
            }
        }

        private void EmitTriggerEvent(CYGPhysicsEventType type, long key)
        {
            int idA = PairBodyA(key);
            int idB = PairBodyB(key);
            CYGPhysicsBody bodyA = GetBody(idA);
            CYGPhysicsBody bodyB = GetBody(idB);
            if (bodyA == null || bodyB == null)
                return;

            int triggerBody = idA;
            int otherBody = idB;
            if (!bodyA.Collider.IsTrigger && bodyB.Collider.IsTrigger)
            {
                triggerBody = idB;
                otherBody = idA;
            }

            AddEvent(type, triggerBody, otherBody, FPVector3.Zero, FPVector3.Up, FP.Zero);
        }

        private void CommitPairHistory()
        {
            EnsureArraySize(ref _prevContactKeys, _contactKeyCount);
            if (_contactKeyCount > 0)
                Array.Copy(_contactKeys, _prevContactKeys, _contactKeyCount);
            _prevContactKeyCount = _contactKeyCount;

            EnsureArraySize(ref _prevTriggerKeys, _triggerKeyCount);
            if (_triggerKeyCount > 0)
                Array.Copy(_triggerKeys, _prevTriggerKeys, _triggerKeyCount);
            _prevTriggerKeyCount = _triggerKeyCount;
        }

        private void AddEvent(CYGPhysicsEventType type, int bodyA, int bodyB, FPVector3 point, FPVector3 normal, FP depth)
        {
            CYGPhysicsEvent evt = default(CYGPhysicsEvent);
            evt.Type = type;
            evt.BodyA = bodyA;
            evt.BodyB = bodyB;
            evt.Point = point;
            evt.Normal = normal;
            evt.Depth = depth;
            _events.Add(evt);
        }

        private void AppendPair(long key)
        {
            EnsureArraySize(ref _pairs, _pairCount + 1);
            _pairs[_pairCount++] = key;
        }

        private void AppendContactKey(long key)
        {
            EnsureArraySize(ref _contactKeys, _contactKeyCount + 1);
            _contactKeys[_contactKeyCount++] = key;
        }

        private void AppendTriggerKey(long key)
        {
            EnsureArraySize(ref _triggerKeys, _triggerKeyCount + 1);
            _triggerKeys[_triggerKeyCount++] = key;
        }

        private void SortPairs()
        {
            for (int i = 1; i < _pairCount; i++)
            {
                long item = _pairs[i];
                int j = i - 1;
                while (j >= 0 && _pairs[j] > item)
                {
                    _pairs[j + 1] = _pairs[j];
                    j--;
                }
                _pairs[j + 1] = item;
            }
        }

        private bool CanInteract(CYGPhysicsBody a, CYGPhysicsBody b)
        {
            if (a == null || b == null || !a.IsActive || !b.IsActive)
                return false;

            // At least one participant must be dynamic for contacts or triggers.
            if (!a.IsDynamic && !b.IsDynamic)
                return false;

            if ((a.LayerMask & (1u << b.Layer)) == 0)
                return false;
            if ((b.LayerMask & (1u << a.Layer)) == 0)
                return false;

            return true;
        }

        private bool IsActiveMover(CYGPhysicsBody body)
        {
            if (body.Type == CYGBodyType.Static || !body.IsAwake)
                return false;

            // Only bodies actually moving above the sleep thresholds wake their
            // neighbors. Treating every awake body as a mover prevents a resting
            // stack from ever sleeping as an island.
            FP linearThreshold = FP.Max(Settings.SleepLinearThreshold, FP.Zero);
            FP angularThreshold = FP.Max(Settings.SleepAngularThreshold, FP.Zero);
            return body.LinearVelocity.SqrMagnitude > linearThreshold * linearThreshold
                || body.AngularVelocity.SqrMagnitude > angularThreshold * angularThreshold;
        }

        private static bool ContainsSortedKey(long[] keys, int count, long key)
        {
            int low = 0;
            int high = count - 1;
            while (low <= high)
            {
                int middle = low + ((high - low) >> 1);
                long value = keys[middle];
                if (value == key)
                    return true;
                if (value < key)
                    low = middle + 1;
                else
                    high = middle - 1;
            }
            return false;
        }

        private static long MakePairKey(int a, int b)
        {
            int low = a < b ? a : b;
            int high = a < b ? b : a;
            return ((long)low << 32) | (uint)high;
        }

        private static int PairBodyA(long key)
        {
            return (int)(key >> 32);
        }

        private static int PairBodyB(long key)
        {
            return (int)(key & 0xFFFFFFFFL);
        }
    }
}
