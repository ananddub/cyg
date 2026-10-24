//-----------------------------------------------------------------------------
// CYGPhysics2DWorld.cs
// Deterministic fixed-point 2D rigid body world with orientation, sleeping,
// joints, spatial queries and rollback snapshots.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic 2D physics world with fixed-step semi-implicit Euler,
    /// sweep-and-prune broadphase, sequential impulses, positional correction
    /// and stable-id events.
    ///
    /// All persisted simulation state is fixed point and all pair traversal is
    /// ordered by monotonic body id. Dynamic bodies sleep after a configurable
    /// number of slow frames, which removes resting jitter completely.
    ///
    /// Limited CCD: this MVP does not perform continuous collision detection.
    /// Fast circles can tunnel through thin static boxes; use a smaller fixed
    /// timestep for high-speed gameplay.
    /// </summary>
    public sealed partial class CYGPhysics2DWorld : ICollisionWorld2D
    {
        private const int InitialBodyCapacity = 16;

        private CYGPhysics2DBody[] _bodies;
        private CYGPhysics2DAABB[] _aabbs;
        private int _bodySlotCount;
        private int _activeBodyCount;
        private int _nextBodyId;

        private int[] _sortOrder;
        private int _sortCount;

        private CYGPhysics2DPair[] _pairs;
        private int _pairCount;

        private CYGPhysics2DPair[] _activeCollisionPairs;
        private int _activeCollisionCount;
        private CYGPhysics2DPair[] _stepCollisionPairs;
        private int _stepCollisionCount;

        private CYGPhysics2DPair[] _activeTriggerPairs;
        private int _activeTriggerCount;
        private CYGPhysics2DPair[] _stepTriggerPairs;
        private int _stepTriggerCount;

        private CYGPhysics2DJoint[] _joints;
        private int _jointCount;
        private int _activeJointCount;
        private int _nextJointId;

        private FPVector2 _gravity;
        private FP _fixedDeltaTime;
        private int _velocityIterations;
        private int _positionIterations;
        private FP _positionCorrection;
        private FP _penetrationSlop;
        private FP _restitutionThreshold;

        private bool _enableSleeping;
        private FP _sleepLinearThreshold;
        private FP _sleepAngularThreshold;
        private int _sleepFrameCount;

        public CYGPhysics2DWorld()
            : this(new FPVector2(FP.Zero, FP.FromInt(-10)))
        {
        }

        public CYGPhysics2DWorld(FPVector2 gravity)
        {
            _bodies = new CYGPhysics2DBody[InitialBodyCapacity];
            _aabbs = new CYGPhysics2DAABB[InitialBodyCapacity];
            _sortOrder = new int[InitialBodyCapacity];
            _pairs = new CYGPhysics2DPair[InitialBodyCapacity];
            _activeCollisionPairs = new CYGPhysics2DPair[InitialBodyCapacity];
            _stepCollisionPairs = new CYGPhysics2DPair[InitialBodyCapacity];
            _activeTriggerPairs = new CYGPhysics2DPair[InitialBodyCapacity];
            _stepTriggerPairs = new CYGPhysics2DPair[InitialBodyCapacity];
            _joints = new CYGPhysics2DJoint[InitialBodyCapacity];
            _contacts = new Contact[InitialBodyCapacity];

            _nextBodyId = 1;
            _nextJointId = 1;
            _gravity = gravity;
            _fixedDeltaTime = FP.FromFraction(1, 60);
            _velocityIterations = 8;
            _positionIterations = 3;
            _positionCorrection = FP.FromFraction(7, 10);
            _penetrationSlop = FP.FromFraction(1, 200);
            _restitutionThreshold = FP.FromFraction(1, 5);

            _enableSleeping = true;
            _sleepLinearThreshold = FP.FromFraction(1, 20);
            _sleepAngularThreshold = FP.FromFraction(1, 20);
            _sleepFrameCount = 30;
        }

        /// <summary>Gravity applied to awake dynamic bodies each step.</summary>
        public FPVector2 Gravity
        {
            get { return _gravity; }
            set { _gravity = value; }
        }

        /// <summary>Fixed simulation timestep in seconds.</summary>
        public FP FixedDeltaTime
        {
            get { return _fixedDeltaTime; }
            set
            {
                if (value <= FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(value), "FixedDeltaTime must be positive.");
                }

                _fixedDeltaTime = value;
            }
        }

        /// <summary>Number of sequential impulse iterations. Fixed for a simulation.</summary>
        public int VelocityIterations
        {
            get { return _velocityIterations; }
            set
            {
                if (value < 0)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _velocityIterations = value;
            }
        }

        /// <summary>Number of positional correction passes. Fixed for a simulation.</summary>
        public int PositionIterations
        {
            get { return _positionIterations; }
            set
            {
                if (value < 0)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _positionIterations = value;
            }
        }

        /// <summary>Fraction of remaining penetration removed per position iteration.</summary>
        public FP PositionCorrection
        {
            get { return _positionCorrection; }
            set
            {
                if (value < FP.Zero || value > FP.One)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _positionCorrection = value;
            }
        }

        /// <summary>Penetration tolerated by positional correction.</summary>
        public FP PenetrationSlop
        {
            get { return _penetrationSlop; }
            set
            {
                if (value < FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _penetrationSlop = value;
            }
        }

        /// <summary>Approach speed below which restitution is suppressed.</summary>
        public FP RestitutionThreshold
        {
            get { return _restitutionThreshold; }
            set
            {
                if (value < FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _restitutionThreshold = value;
            }
        }

        /// <summary>Enables fixed-step sleeping of slow dynamic bodies.</summary>
        public bool EnableSleeping
        {
            get { return _enableSleeping; }
            set { _enableSleeping = value; }
        }

        /// <summary>Linear speed below which a dynamic body counts as resting.</summary>
        public FP SleepLinearThreshold
        {
            get { return _sleepLinearThreshold; }
            set
            {
                if (value < FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _sleepLinearThreshold = value;
            }
        }

        /// <summary>Angular speed below which a dynamic body counts as resting.</summary>
        public FP SleepAngularThreshold
        {
            get { return _sleepAngularThreshold; }
            set
            {
                if (value < FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _sleepAngularThreshold = value;
            }
        }

        /// <summary>Consecutive resting frames required before a body sleeps.</summary>
        public int SleepFrameCount
        {
            get { return _sleepFrameCount; }
            set
            {
                if (value < 1)
                {
                    throw new ArgumentOutOfRangeException(nameof(value));
                }

                _sleepFrameCount = value;
            }
        }

        /// <summary>Optional deterministic collision/trigger event sink.</summary>
        public ICYGPhysics2DCallbacks Callbacks { get; set; }

        /// <summary>Number of completed simulation steps.</summary>
        public int Tick { get; private set; }

        /// <summary>Number of live bodies.</summary>
        public int BodyCount
        {
            get { return _activeBodyCount; }
        }

        /// <summary>Number of live joints.</summary>
        public int JointCount
        {
            get { return _activeJointCount; }
        }

        #region Body Management

        /// <summary>Adds a falling dynamic circle with unit mass.</summary>
        public int CreateCircle(FPVector2 position, FP radius)
        {
            return AddBody(CYGPhysics2DBody.Circle(position, radius));
        }

        /// <summary>Adds a circle with explicit mass.</summary>
        public int CreateCircle(FPVector2 position, FP radius, FP mass)
        {
            return AddBody(CYGPhysics2DBody.Circle(position, radius, CYGPhysics2DBodyType.Dynamic, mass));
        }

        /// <summary>Adds a falling dynamic box with unit mass.</summary>
        public int CreateBox(FPVector2 position, FPVector2 halfExtents)
        {
            return AddBody(CYGPhysics2DBody.Box(position, halfExtents));
        }

        /// <summary>Adds a rotated dynamic box with unit mass.</summary>
        public int CreateRotatedBox(FPVector2 position, FPVector2 halfExtents, FP angle)
        {
            return AddBody(CYGPhysics2DBody.Box(position, halfExtents, angle));
        }

        /// <summary>Adds a box with explicit mass.</summary>
        public int CreateBox(FPVector2 position, FPVector2 halfExtents, FP mass)
        {
            return AddBody(CYGPhysics2DBody.Box(position, halfExtents, CYGPhysics2DBodyType.Dynamic, mass));
        }

        /// <summary>
        /// Adds a body and assigns its stable id. The id grows monotonically and is
        /// never reused, so ordering remains deterministic after removals.
        /// </summary>
        public int AddBody(CYGPhysics2DBody body)
        {
            NormalizeBody(ref body);

            if (_bodySlotCount == _bodies.Length)
            {
                GrowArray(ref _bodies, _bodySlotCount + 1);
                GrowArray(ref _aabbs, _bodySlotCount + 1);
            }

            body.Id = _nextBodyId++;
            body.Active = true;
            _bodies[_bodySlotCount++] = body;
            _activeBodyCount++;

            EnsureCapacitiesForBodyCount(_activeBodyCount);
            return body.Id;
        }

        /// <summary>Replaces all configuration and state of a body, preserving its id.</summary>
        public bool UpdateBody(int bodyId, CYGPhysics2DBody body)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            NormalizeBody(ref body);
            body.Id = bodyId;
            body.Active = true;
            _bodies[slot] = body;
            return true;
        }

        /// <summary>Removes a body and deactivates every joint that referenced it.</summary>
        public bool DestroyBody(int bodyId)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            if (!body.Active)
            {
                return false;
            }

            body.Active = false;
            body.Velocity = FPVector2.Zero;
            body.AngularVelocity = FP.Zero;
            body.IsAwake = false;
            _activeBodyCount--;

            for (int i = 0; i < _jointCount; i++)
            {
                if (_joints[i].Active && (_joints[i].BodyA == bodyId || _joints[i].BodyB == bodyId))
                {
                    _joints[i].Active = false;
                    _activeJointCount--;
                }
            }

            return true;
        }

        public bool TryGetBody(int bodyId, out CYGPhysics2DBody body)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                body = default(CYGPhysics2DBody);
                return false;
            }

            body = _bodies[slot];
            return body.Active;
        }

        public bool SetPosition(int bodyId, FPVector2 position)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].Position = position;
            WakeSlot(slot);
            return true;
        }

        public bool SetAngle(int bodyId, FP angle)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].Angle = CYGPhysics2DMath.NormalizeAngle(angle);
            WakeSlot(slot);
            return true;
        }

        public bool SetVelocity(int bodyId, FPVector2 velocity)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            body.Velocity = body.IsStatic ? FPVector2.Zero : velocity;
            if (velocity != FPVector2.Zero)
            {
                WakeSlot(slot);
            }

            return true;
        }

        public bool SetAngularVelocity(int bodyId, FP angularVelocity)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            body.AngularVelocity = body.IsStatic ? FP.Zero : angularVelocity;
            if (angularVelocity != FP.Zero)
            {
                WakeSlot(slot);
            }

            return true;
        }

        public bool SetGravityScale(int bodyId, FP gravityScale)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].GravityScale = gravityScale;
            WakeSlot(slot);
            return true;
        }

        public bool SetMaterial(int bodyId, FP friction, FP restitution)
        {
            if (friction < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(friction));
            }

            if (restitution < FP.Zero || restitution > FP.One)
            {
                throw new ArgumentOutOfRangeException(nameof(restitution));
            }

            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].Friction = friction;
            _bodies[slot].Restitution = restitution;
            return true;
        }

        public bool SetDamping(int bodyId, FP linearDamping, FP angularDamping)
        {
            if (linearDamping < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(linearDamping));
            }

            if (angularDamping < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(angularDamping));
            }

            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].LinearDamping = linearDamping;
            _bodies[slot].AngularDamping = angularDamping;
            return true;
        }

        public bool SetLayer(int bodyId, int layer, int collisionMask)
        {
            ValidateLayer(layer);
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].Layer = layer;
            _bodies[slot].CollisionMask = collisionMask;
            return true;
        }

        public bool SetTrigger(int bodyId, bool isTrigger)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            _bodies[slot].IsTrigger = isTrigger;
            return true;
        }

        /// <summary>Wakes or sleeps a dynamic body. Sleeping zeros its velocity.</summary>
        public bool SetAwake(int bodyId, bool awake)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            if (!body.IsDynamic)
            {
                return true;
            }

            body.IsAwake = awake;
            body.SleepFrames = 0;
            if (!awake)
            {
                body.Velocity = FPVector2.Zero;
                body.AngularVelocity = FP.Zero;
            }

            return true;
        }

        /// <summary>Applies a world-space linear impulse to a dynamic body.</summary>
        public bool ApplyLinearImpulse(int bodyId, FPVector2 impulse)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            if (!body.IsDynamic)
            {
                return false;
            }

            body.Velocity += impulse * body.InverseMass;
            WakeSlot(slot);
            return true;
        }

        /// <summary>Applies a world-space impulse at a world-space point.</summary>
        public bool ApplyImpulseAtPoint(int bodyId, FPVector2 impulse, FPVector2 point)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0)
            {
                return false;
            }

            ref CYGPhysics2DBody body = ref _bodies[slot];
            if (!body.IsDynamic)
            {
                return false;
            }

            FPVector2 lever = point - body.Position;
            body.Velocity += impulse * body.InverseMass;
            body.AngularVelocity += body.InverseInertia * CYGPhysics2DMath.Cross(lever, impulse);
            WakeSlot(slot);
            return true;
        }

        /// <summary>Copies live body ids in ascending order. Returns the number written.</summary>
        public int GetBodyIds(int[] buffer)
        {
            if (buffer == null)
            {
                return 0;
            }

            int written = 0;
            for (int i = 0; i < _bodySlotCount; i++)
            {
                if (!_bodies[i].Active)
                {
                    continue;
                }

                if (written < buffer.Length)
                {
                    buffer[written] = _bodies[i].Id;
                }

                written++;
            }

            return written;
        }

        #endregion

        #region Joint Management

        /// <summary>Creates a rigid distance joint between the body centers.</summary>
        public int CreateDistanceJoint(int bodyA, int bodyB)
        {
            return CreateDistanceJoint(bodyA, bodyB, FPVector2.Zero, FPVector2.Zero);
        }

        /// <summary>Creates a rigid distance joint using local anchors and their current distance.</summary>
        public int CreateDistanceJoint(
            int bodyA,
            int bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB)
        {
            int slotA = RequireJointBody(bodyA);
            int slotB = RequireJointBody(bodyB);
            FPVector2 worldA = _bodies[slotA].TransformPoint(localAnchorA);
            FPVector2 worldB = _bodies[slotB].TransformPoint(localAnchorB);
            FP restLength = FPVector2.Distance(worldA, worldB);
            return AddJoint(CYGPhysics2DJointType.Distance, bodyA, bodyB, localAnchorA, localAnchorB, restLength, FP.Zero, FP.Zero);
        }

        /// <summary>Creates a rigid distance joint with an explicit rest length.</summary>
        public int CreateDistanceJoint(
            int bodyA,
            int bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB,
            FP restLength)
        {
            RequireJointBody(bodyA);
            RequireJointBody(bodyB);
            if (restLength < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(restLength));
            }

            return AddJoint(CYGPhysics2DJointType.Distance, bodyA, bodyB, localAnchorA, localAnchorB, restLength, FP.Zero, FP.Zero);
        }

        /// <summary>Creates a soft spring joint between the body centers.</summary>
        public int CreateSpringJoint(int bodyA, int bodyB, FP restLength, FP stiffness, FP damping)
        {
            return CreateSpringJoint(bodyA, bodyB, FPVector2.Zero, FPVector2.Zero, restLength, stiffness, damping);
        }

        /// <summary>Creates a soft spring joint using local anchors.</summary>
        public int CreateSpringJoint(
            int bodyA,
            int bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB,
            FP restLength,
            FP stiffness,
            FP damping)
        {
            RequireJointBody(bodyA);
            RequireJointBody(bodyB);
            if (restLength < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(restLength));
            }

            if (stiffness < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(stiffness));
            }

            if (damping < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(damping));
            }

            return AddJoint(CYGPhysics2DJointType.Spring, bodyA, bodyB, localAnchorA, localAnchorB, restLength, stiffness, damping);
        }

        /// <summary>Removes a joint.</summary>
        public bool DestroyJoint(int jointId)
        {
            int slot = FindJointSlot(jointId);
            if (slot < 0 || !_joints[slot].Active)
            {
                return false;
            }

            _joints[slot].Active = false;
            _activeJointCount--;
            return true;
        }

        public bool TryGetJoint(int jointId, out CYGPhysics2DJoint joint)
        {
            int slot = FindJointSlot(jointId);
            if (slot < 0 || !_joints[slot].Active)
            {
                joint = default(CYGPhysics2DJoint);
                return false;
            }

            joint = _joints[slot];
            return true;
        }

        private int AddJoint(
            CYGPhysics2DJointType type,
            int bodyA,
            int bodyB,
            FPVector2 localAnchorA,
            FPVector2 localAnchorB,
            FP restLength,
            FP stiffness,
            FP damping)
        {
            if (_jointCount == _joints.Length)
            {
                GrowArray(ref _joints, _jointCount + 1);
            }

            CYGPhysics2DJoint joint = new CYGPhysics2DJoint();
            joint.Id = _nextJointId++;
            joint.Active = true;
            joint.Type = type;
            joint.BodyA = bodyA;
            joint.BodyB = bodyB;
            joint.LocalAnchorA = localAnchorA;
            joint.LocalAnchorB = localAnchorB;
            joint.RestLength = restLength;
            joint.Stiffness = stiffness;
            joint.Damping = damping;

            _joints[_jointCount++] = joint;
            _activeJointCount++;
            return joint.Id;
        }

        private int RequireJointBody(int bodyId)
        {
            int slot = FindSlot(bodyId);
            if (slot < 0 || !_bodies[slot].Active)
            {
                throw new ArgumentException("Joint body " + bodyId + " does not exist.");
            }

            return slot;
        }

        private int FindJointSlot(int jointId)
        {
            int low = 0;
            int high = _jointCount - 1;
            while (low <= high)
            {
                int middle = low + ((high - low) >> 1);
                int middleId = _joints[middle].Id;
                if (middleId == jointId)
                {
                    return middle;
                }

                if (middleId < jointId)
                {
                    low = middle + 1;
                }
                else
                {
                    high = middle - 1;
                }
            }

            return -1;
        }

        #endregion

        #region Simulation

        /// <summary>
        /// Advances the world by one fixed step. Contact and trigger callbacks are
        /// dispatched after integration and solving. No managed allocations occur
        /// once body, joint and pair capacities have grown to the scene high-water mark.
        /// </summary>
        public void Step()
        {
            IntegrateBodies();
            BuildBroadphase();
            WakeJointBodies();
            ApplySpringJoints();
            BuildContacts();
            SolveVelocities();
            SolvePositions();
            UpdateSleepState();

            Tick++;
            DispatchCallbacks();
            SwapPairBuffers();
        }

        private void IntegrateBodies()
        {
            FP dt = _fixedDeltaTime;
            FPVector2 gravity = _gravity;

            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active)
                {
                    continue;
                }

                if (body.Type == CYGPhysics2DBodyType.Dynamic)
                {
                    if (!body.IsAwake)
                    {
                        continue;
                    }

                    body.Velocity += gravity * (body.GravityScale * dt);
                    if (body.LinearDamping > FP.Zero)
                    {
                        body.Velocity = body.Velocity / (FP.One + body.LinearDamping * dt);
                    }

                    body.Position += body.Velocity * dt;

                    if (body.AngularDamping > FP.Zero)
                    {
                        body.AngularVelocity = body.AngularVelocity / (FP.One + body.AngularDamping * dt);
                    }

                    body.Angle = CYGPhysics2DMath.NormalizeAngle(body.Angle + body.AngularVelocity * dt);
                }
                else if (body.Type == CYGPhysics2DBodyType.Kinematic)
                {
                    body.Position += body.Velocity * dt;
                    body.Angle = CYGPhysics2DMath.NormalizeAngle(body.Angle + body.AngularVelocity * dt);
                }
                else
                {
                    body.Velocity = FPVector2.Zero;
                    body.AngularVelocity = FP.Zero;
                }
            }
        }

        private void BuildBroadphase()
        {
            _sortCount = 0;
            for (int i = 0; i < _bodySlotCount; i++)
            {
                if (_bodies[i].Active)
                {
                    _aabbs[i] = _bodies[i].GetAABB();
                    _sortOrder[_sortCount++] = i;
                }
            }

            ShellSortByMinX(_sortOrder, _sortCount);
            _pairCount = 0;

            for (int i = 0; i < _sortCount; i++)
            {
                int slotA = _sortOrder[i];
                ref CYGPhysics2DBody bodyA = ref _bodies[slotA];
                CYGPhysics2DAABB aabbA = _aabbs[slotA];

                for (int j = i + 1; j < _sortCount; j++)
                {
                    int slotB = _sortOrder[j];
                    CYGPhysics2DAABB aabbB = _aabbs[slotB];

                    // Sorted by min x, so every later body starts further right.
                    if (aabbB.Min.x > aabbA.Max.x)
                    {
                        break;
                    }

                    if (aabbB.Max.x < aabbA.Min.x || aabbB.Min.y > aabbA.Max.y || aabbB.Max.y < aabbA.Min.y)
                    {
                        continue;
                    }

                    ref CYGPhysics2DBody bodyB = ref _bodies[slotB];
                    if (!LayersCollide(bodyA, bodyB))
                    {
                        continue;
                    }

                    _pairs[_pairCount++] = new CYGPhysics2DPair(bodyA.Id, bodyB.Id);
                }
            }

            ShellSortPairs(_pairs, _pairCount);
        }

        private void UpdateSleepState()
        {
            if (!_enableSleeping)
            {
                for (int i = 0; i < _bodySlotCount; i++)
                {
                    if (_bodies[i].Active && _bodies[i].IsDynamic)
                    {
                        _bodies[i].SleepFrames = 0;
                    }
                }

                return;
            }

            FP linearLimit = _sleepLinearThreshold * _sleepLinearThreshold;
            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active || !body.IsDynamic || !body.IsAwake)
                {
                    continue;
                }

                bool resting = body.Velocity.SqrMagnitude <= linearLimit
                    && FP.Abs(body.AngularVelocity) <= _sleepAngularThreshold;

                if (resting)
                {
                    body.SleepFrames++;
                    if (body.SleepFrames >= _sleepFrameCount)
                    {
                        body.IsAwake = false;
                        body.SleepFrames = 0;
                        body.Velocity = FPVector2.Zero;
                        body.AngularVelocity = FP.Zero;
                    }
                }
                else
                {
                    body.SleepFrames = 0;
                }
            }
        }

        private void SwapPairBuffers()
        {
            CYGPhysics2DPair[] collisionSwap = _activeCollisionPairs;
            _activeCollisionPairs = _stepCollisionPairs;
            _stepCollisionPairs = collisionSwap;
            _activeCollisionCount = _stepCollisionCount;
            _stepCollisionCount = 0;

            CYGPhysics2DPair[] triggerSwap = _activeTriggerPairs;
            _activeTriggerPairs = _stepTriggerPairs;
            _stepTriggerPairs = triggerSwap;
            _activeTriggerCount = _stepTriggerCount;
            _stepTriggerCount = 0;
        }

        private void DispatchCallbacks()
        {
            ICYGPhysics2DCallbacks callbacks = Callbacks;
            if (callbacks == null)
            {
                return;
            }

            DispatchPairEvents(
                _activeCollisionPairs, _activeCollisionCount,
                _stepCollisionPairs, _stepCollisionCount,
                callbacks, false);

            DispatchPairEvents(
                _activeTriggerPairs, _activeTriggerCount,
                _stepTriggerPairs, _stepTriggerCount,
                callbacks, true);
        }

        private static void DispatchPairEvents(
            CYGPhysics2DPair[] previous,
            int previousCount,
            CYGPhysics2DPair[] current,
            int currentCount,
            ICYGPhysics2DCallbacks callbacks,
            bool trigger)
        {
            int i = 0;
            int j = 0;

            while (i < previousCount || j < currentCount)
            {
                if (j >= currentCount)
                {
                    NotifyExit(callbacks, previous[i++], trigger);
                }
                else if (i >= previousCount)
                {
                    NotifyEnter(callbacks, current[j++], trigger);
                }
                else
                {
                    int order = previous[i].CompareTo(current[j]);
                    if (order < 0)
                    {
                        NotifyExit(callbacks, previous[i++], trigger);
                    }
                    else if (order > 0)
                    {
                        NotifyEnter(callbacks, current[j++], trigger);
                    }
                    else
                    {
                        if (trigger)
                        {
                            callbacks.OnTriggerStay(previous[i].BodyA, previous[i].BodyB);
                        }
                        else
                        {
                            callbacks.OnCollisionStay(previous[i].BodyA, previous[i].BodyB);
                        }

                        i++;
                        j++;
                    }
                }
            }
        }

        private static void NotifyEnter(ICYGPhysics2DCallbacks callbacks, CYGPhysics2DPair pair, bool trigger)
        {
            if (trigger)
            {
                callbacks.OnTriggerEnter(pair.BodyA, pair.BodyB);
            }
            else
            {
                callbacks.OnCollisionEnter(pair.BodyA, pair.BodyB);
            }
        }

        private static void NotifyExit(ICYGPhysics2DCallbacks callbacks, CYGPhysics2DPair pair, bool trigger)
        {
            if (trigger)
            {
                callbacks.OnTriggerExit(pair.BodyA, pair.BodyB);
            }
            else
            {
                callbacks.OnCollisionExit(pair.BodyA, pair.BodyB);
            }
        }

        #endregion

        #region Snapshot And Checksum

        /// <summary>Captures complete simulation state for rollback.</summary>
        public CYGPhysics2DWorldSnapshot CaptureSnapshot()
        {
            CYGPhysics2DBody[] bodies = new CYGPhysics2DBody[_activeBodyCount];
            int bodyIndex = 0;
            for (int i = 0; i < _bodySlotCount; i++)
            {
                if (_bodies[i].Active)
                {
                    bodies[bodyIndex++] = _bodies[i];
                }
            }

            CYGPhysics2DPair[] collisionPairs = new CYGPhysics2DPair[_activeCollisionCount];
            Array.Copy(_activeCollisionPairs, collisionPairs, _activeCollisionCount);
            CYGPhysics2DPair[] triggerPairs = new CYGPhysics2DPair[_activeTriggerCount];
            Array.Copy(_activeTriggerPairs, triggerPairs, _activeTriggerCount);

            return new CYGPhysics2DWorldSnapshot(
                Tick,
                _nextBodyId,
                bodies,
                collisionPairs,
                _activeCollisionCount,
                triggerPairs,
                _activeTriggerCount);
        }

        /// <summary>
        /// Restores bodies (including awake state), tick and persistent event-pair
        /// state. World configuration and joints are not captured.
        /// </summary>
        public void RestoreSnapshot(CYGPhysics2DWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                throw new ArgumentNullException(nameof(snapshot));
            }

            int bodyCount = snapshot.Bodies.Length;
            if (_bodies.Length < bodyCount)
            {
                GrowArray(ref _bodies, bodyCount);
            }

            if (_aabbs.Length < bodyCount)
            {
                GrowArray(ref _aabbs, bodyCount);
            }

            Array.Copy(snapshot.Bodies, _bodies, bodyCount);
            _bodySlotCount = bodyCount;
            _activeBodyCount = bodyCount;
            _nextBodyId = snapshot.NextBodyId;
            Tick = snapshot.Tick;

            EnsureCapacitiesForBodyCount(bodyCount);

            _activeCollisionCount = snapshot.CollisionCount;
            Array.Copy(snapshot.CollisionPairs, _activeCollisionPairs, _activeCollisionCount);
            _activeTriggerCount = snapshot.TriggerCount;
            Array.Copy(snapshot.TriggerPairs, _activeTriggerPairs, _activeTriggerCount);
            _stepCollisionCount = 0;
            _stepTriggerCount = 0;

            _sortCount = 0;
            _pairCount = 0;
            _contactCount = 0;
        }

        /// <summary>
        /// CRC32 of tick, body state and persistent contact pair sets in stable
        /// body-id order. Zero-allocation. Two worlds stepped with identical
        /// inputs produce identical checksums.
        /// </summary>
        public uint ComputeChecksum()
        {
            uint crc = 0xFFFFFFFFu;

            CYGChecksum.HashInt(ref crc, Tick);
            CYGChecksum.HashInt(ref crc, _activeBodyCount);
            CYGChecksum.HashInt(ref crc, _nextBodyId);

            for (int i = 0; i < _bodySlotCount; i++)
            {
                if (_bodies[i].Active)
                {
                    _bodies[i].ComputeChecksum(ref crc);
                }
            }

            CYGChecksum.HashInt(ref crc, _activeCollisionCount);
            for (int i = 0; i < _activeCollisionCount; i++)
            {
                CYGChecksum.HashInt(ref crc, _activeCollisionPairs[i].BodyA);
                CYGChecksum.HashInt(ref crc, _activeCollisionPairs[i].BodyB);
            }

            CYGChecksum.HashInt(ref crc, _activeTriggerCount);
            for (int i = 0; i < _activeTriggerCount; i++)
            {
                CYGChecksum.HashInt(ref crc, _activeTriggerPairs[i].BodyA);
                CYGChecksum.HashInt(ref crc, _activeTriggerPairs[i].BodyB);
            }

            return crc ^ 0xFFFFFFFFu;
        }

        #endregion

        #region Queries

        /// <summary>Raycast against all layers. Includes triggers. Nearest hit wins.</summary>
        public bool Raycast(FPVector2 origin, FPVector2 direction, FP maxDistance, out CYGPhysics2DRaycastHit hit)
        {
            return Raycast(origin, direction, maxDistance, -1, out hit);
        }

        /// <summary>
        /// Raycast against bodies whose layer is in <paramref name="layerMask"/>.
        /// Ties at equal distance are resolved by lower body id.
        /// </summary>
        public bool Raycast(
            FPVector2 origin,
            FPVector2 direction,
            FP maxDistance,
            int layerMask,
            out CYGPhysics2DRaycastHit hit)
        {
            hit = CYGPhysics2DRaycastHit.None;
            if (maxDistance < FP.Zero)
            {
                return false;
            }

            FP directionLengthSq = direction.SqrMagnitude;
            if (directionLengthSq <= FP.Zero)
            {
                return false;
            }

            FPVector2 dir = direction / FP.Sqrt(directionLengthSq);
            bool found = false;
            FP bestDistance = FP.Zero;
            int bestId = 0;

            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active || (layerMask & (1 << body.Layer)) == 0)
                {
                    continue;
                }

                FP distance;
                FPVector2 normal;
                if (!CYGPhysics2DNarrowphase.RaycastBody(body, origin, dir, maxDistance, out distance, out normal))
                {
                    continue;
                }

                if (found && (distance > bestDistance || (distance == bestDistance && body.Id >= bestId)))
                {
                    continue;
                }

                found = true;
                bestDistance = distance;
                bestId = body.Id;

                hit.Hit = true;
                hit.BodyId = body.Id;
                hit.Distance = distance;
                hit.Point = origin + dir * distance;
                hit.Normal = normal;
            }

            return found;
        }

        /// <summary>
        /// Finds bodies overlapping a circle, nearest first then ascending body id.
        /// Returns the total match count even if the buffer is too small.
        /// </summary>
        public int OverlapCircle(FPVector2 center, FP radius, int layerMask, int[] results)
        {
            if (radius < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(radius));
            }

            int capacity = _activeBodyCount;
            int[] ids = new int[capacity];
            FP[] distances = new FP[capacity];
            int matches = 0;

            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active || (layerMask & (1 << body.Layer)) == 0)
                {
                    continue;
                }

                if (!CYGPhysics2DNarrowphase.OverlapCircleBody(body, center, radius))
                {
                    continue;
                }

                ids[matches] = body.Id;
                distances[matches] = FPVector2.DistanceSquared(center, body.Position);
                matches++;
            }

            SortQueryResults(ids, distances, matches);
            CopyQueryResults(ids, results, matches);
            return matches;
        }

        /// <summary>
        /// Finds bodies overlapping the axis-aligned box [min, max], nearest first
        /// then ascending body id. Returns the total match count.
        /// </summary>
        public int OverlapAABB(FPVector2 min, FPVector2 max, int layerMask, int[] results)
        {
            if (max.x < min.x || max.y < min.y)
            {
                throw new ArgumentOutOfRangeException(nameof(max));
            }

            int capacity = _activeBodyCount;
            int[] ids = new int[capacity];
            FP[] distances = new FP[capacity];
            FPVector2 queryCenter = (min + max) * FP.Half;
            int matches = 0;

            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active || (layerMask & (1 << body.Layer)) == 0)
                {
                    continue;
                }

                if (!CYGPhysics2DNarrowphase.OverlapAABBBody(body, min, max))
                {
                    continue;
                }

                ids[matches] = body.Id;
                distances[matches] = FPVector2.DistanceSquared(queryCenter, body.Position);
                matches++;
            }

            SortQueryResults(ids, distances, matches);
            CopyQueryResults(ids, results, matches);
            return matches;
        }

        /// <summary>Convenience wrapper around <see cref="OverlapAABB"/>.</summary>
        public int OverlapBox(FPVector2 center, FPVector2 halfExtents, int layerMask, int[] results)
        {
            if (halfExtents.x < FP.Zero || halfExtents.y < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(halfExtents));
            }

            return OverlapAABB(center - halfExtents, center + halfExtents, layerMask, results);
        }

        /// <summary>True if any body (including triggers) contains the point on the given layers.</summary>
        public bool OverlapPoint(FPVector2 point, int layerMask)
        {
            for (int i = 0; i < _bodySlotCount; i++)
            {
                ref CYGPhysics2DBody body = ref _bodies[i];
                if (!body.Active || (layerMask & (1 << body.Layer)) == 0)
                {
                    continue;
                }

                if (body.Shape == CYGPhysics2DShapeType.Circle)
                {
                    if (FPVector2.DistanceSquared(point, body.Position) <= body.Radius * body.Radius)
                    {
                        return true;
                    }
                }
                else
                {
                    FPVector2 local = body.InverseTransformPoint(point);
                    if (FP.Abs(local.x) <= body.HalfExtents.x && FP.Abs(local.y) <= body.HalfExtents.y)
                    {
                        return true;
                    }
                }
            }

            return false;
        }

        private static void SortQueryResults(int[] ids, FP[] distances, int count)
        {
            for (int i = 1; i < count; i++)
            {
                int keyId = ids[i];
                FP keyDistance = distances[i];
                int j = i - 1;

                while (j >= 0 && (distances[j] > keyDistance || (distances[j] == keyDistance && ids[j] > keyId)))
                {
                    ids[j + 1] = ids[j];
                    distances[j + 1] = distances[j];
                    j--;
                }

                ids[j + 1] = keyId;
                distances[j + 1] = keyDistance;
            }
        }

        private static void CopyQueryResults(int[] ids, int[] results, int count)
        {
            if (results == null)
            {
                return;
            }

            int copyCount = count < results.Length ? count : results.Length;
            for (int i = 0; i < copyCount; i++)
            {
                results[i] = ids[i];
            }
        }

        #endregion

        #region Helpers

        private void NormalizeBody(ref CYGPhysics2DBody body)
        {
            if (body.Shape != CYGPhysics2DShapeType.Circle && body.Shape != CYGPhysics2DShapeType.Box)
            {
                throw new ArgumentException("Unsupported shape.");
            }

            if (body.Type != CYGPhysics2DBodyType.Static &&
                body.Type != CYGPhysics2DBodyType.Kinematic &&
                body.Type != CYGPhysics2DBodyType.Dynamic)
            {
                throw new ArgumentException("Unsupported body type.");
            }

            if (body.Friction < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(body), "Friction must be non-negative.");
            }

            if (body.Restitution < FP.Zero || body.Restitution > FP.One)
            {
                throw new ArgumentOutOfRangeException(nameof(body), "Restitution must be in [0, 1].");
            }

            if (body.LinearDamping < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(body), "Linear damping must be non-negative.");
            }

            if (body.AngularDamping < FP.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(body), "Angular damping must be non-negative.");
            }

            ValidateLayer(body.Layer);

            if (body.Shape == CYGPhysics2DShapeType.Circle)
            {
                if (body.Radius <= FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(body), "Circle radius must be positive.");
                }

                body.HalfExtents = FPVector2.Zero;
            }
            else
            {
                if (body.HalfExtents.x <= FP.Zero || body.HalfExtents.y <= FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(body), "Box half extents must be positive.");
                }

                body.Radius = FP.Zero;
            }

            body.Angle = CYGPhysics2DMath.NormalizeAngle(body.Angle);

            if (body.Type == CYGPhysics2DBodyType.Dynamic)
            {
                if (body.Mass <= FP.Zero)
                {
                    throw new ArgumentOutOfRangeException(nameof(body), "Dynamic bodies need a positive mass.");
                }

                body.InverseMass = FP.One / body.Mass;
                if (body.Shape == CYGPhysics2DShapeType.Circle)
                {
                    body.Inertia = FP.Half * body.Mass * body.Radius * body.Radius;
                }
                else
                {
                    body.Inertia = body.Mass * (body.HalfExtents.x * body.HalfExtents.x
                        + body.HalfExtents.y * body.HalfExtents.y) / FP.FromInt(3);
                }

                body.InverseInertia = body.Inertia > FP.Zero ? FP.One / body.Inertia : FP.Zero;
                body.IsAwake = true;
                body.SleepFrames = 0;
            }
            else
            {
                body.Mass = FP.Zero;
                body.InverseMass = FP.Zero;
                body.Inertia = FP.Zero;
                body.InverseInertia = FP.Zero;
                body.IsAwake = true;
                body.SleepFrames = 0;

                if (body.Type == CYGPhysics2DBodyType.Static)
                {
                    body.Velocity = FPVector2.Zero;
                    body.AngularVelocity = FP.Zero;
                }
            }
        }

        private static void ValidateLayer(int layer)
        {
            if (layer < 0 || layer > 31)
            {
                throw new ArgumentOutOfRangeException(nameof(layer), "Layer must be in [0, 31].");
            }
        }

        private void WakeSlot(int slot)
        {
            ref CYGPhysics2DBody body = ref _bodies[slot];
            if (body.IsDynamic)
            {
                body.IsAwake = true;
                body.SleepFrames = 0;
            }
        }

        private int FindSlot(int bodyId)
        {
            int low = 0;
            int high = _bodySlotCount - 1;

            while (low <= high)
            {
                int middle = low + ((high - low) >> 1);
                int middleId = _bodies[middle].Id;
                if (middleId == bodyId)
                {
                    return middle;
                }

                if (middleId < bodyId)
                {
                    low = middle + 1;
                }
                else
                {
                    high = middle - 1;
                }
            }

            return -1;
        }

        private static bool LayersCollide(in CYGPhysics2DBody a, in CYGPhysics2DBody b)
        {
            return (a.CollisionMask & (1 << b.Layer)) != 0
                && (b.CollisionMask & (1 << a.Layer)) != 0;
        }

        private void EnsureCapacitiesForBodyCount(int bodyCount)
        {
            int slotCount = bodyCount > _bodySlotCount ? bodyCount : _bodySlotCount;

            if (_sortOrder.Length < slotCount)
            {
                GrowArray(ref _sortOrder, slotCount);
            }

            if (_aabbs.Length < slotCount)
            {
                GrowArray(ref _aabbs, slotCount);
            }

            int maxPairs = MaxPairCount(bodyCount);
            if (_pairs.Length < maxPairs)
            {
                GrowArray(ref _pairs, maxPairs);
            }

            if (_contacts.Length < maxPairs)
            {
                GrowArray(ref _contacts, maxPairs);
            }

            if (_activeCollisionPairs.Length < maxPairs)
            {
                GrowArray(ref _activeCollisionPairs, maxPairs);
            }

            if (_stepCollisionPairs.Length < maxPairs)
            {
                GrowArray(ref _stepCollisionPairs, maxPairs);
            }

            if (_activeTriggerPairs.Length < maxPairs)
            {
                GrowArray(ref _activeTriggerPairs, maxPairs);
            }

            if (_stepTriggerPairs.Length < maxPairs)
            {
                GrowArray(ref _stepTriggerPairs, maxPairs);
            }
        }

        private static int MaxPairCount(int bodyCount)
        {
            if (bodyCount < 2)
            {
                return 0;
            }

            long pairs = (long)bodyCount * (bodyCount - 1) / 2;
            return pairs > int.MaxValue ? int.MaxValue : (int)pairs;
        }

        private static void GrowArray<T>(ref T[] array, int required)
        {
            if (array.Length >= required)
            {
                return;
            }

            int capacity = array.Length < 1 ? 1 : array.Length;
            while (capacity < required)
            {
                if (capacity > int.MaxValue / 2)
                {
                    capacity = required;
                    break;
                }

                capacity *= 2;
            }

            T[] grown = new T[capacity];
            Array.Copy(array, grown, array.Length);
            array = grown;
        }

        private void ShellSortByMinX(int[] order, int count)
        {
            int gap = 1;
            while (gap < count / 3)
            {
                gap = gap * 3 + 1;
            }

            for (; gap > 0; gap /= 3)
            {
                for (int i = gap; i < count; i++)
                {
                    int key = order[i];
                    int j = i - gap;

                    while (j >= 0 && CompareBodyOrder(order[j], key) > 0)
                    {
                        order[j + gap] = order[j];
                        j -= gap;
                    }

                    order[j + gap] = key;
                }
            }
        }

        private int CompareBodyOrder(int slotA, int slotB)
        {
            ref CYGPhysics2DBody a = ref _bodies[slotA];
            ref CYGPhysics2DBody b = ref _bodies[slotB];

            int byX = _aabbs[slotA].Min.x.CompareTo(_aabbs[slotB].Min.x);
            return byX != 0 ? byX : a.Id.CompareTo(b.Id);
        }

        private static void ShellSortPairs(CYGPhysics2DPair[] pairs, int count)
        {
            int gap = 1;
            while (gap < count / 3)
            {
                gap = gap * 3 + 1;
            }

            for (; gap > 0; gap /= 3)
            {
                for (int i = gap; i < count; i++)
                {
                    CYGPhysics2DPair key = pairs[i];
                    int j = i - gap;

                    while (j >= 0 && pairs[j].CompareTo(key) > 0)
                    {
                        pairs[j + gap] = pairs[j];
                        j -= gap;
                    }

                    pairs[j + gap] = key;
                }
            }
        }

        #endregion
    }
}
