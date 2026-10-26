//-----------------------------------------------------------------------------
// FPCharacterController.cs
// Simple 3D kinematic capsule character controller.
// Ground check, step/slope slide, kinematic movement.
// Zero UnityEngine dependencies. Compatible with netstandard2.1 / headless server.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    [Flags]
    public enum CYGCollisionFlags
    {
        None = 0,
        Sides = 1 << 0,
        Above = 1 << 1,
        Below = 1 << 2
    }

    /// <summary>Kinematic platform whose motion is carried by grounded characters.</summary>
    public struct CYGMovingPlatform
    {
        public int Id;
        public FPAABB Box;
        public FPVector3 Velocity;

        public CYGMovingPlatform(int id, FPAABB box, FPVector3 velocity)
        {
            Id = id;
            Box = box;
            Velocity = velocity;
        }
    }

    /// <summary>Optional collision-world extension for moving platforms.</summary>
    public interface ICYGMovingPlatformWorld
    {
        bool TryGetPlatformVelocity(FPVector3 point, out FPVector3 velocity);
    }

    /// <summary>
    /// Interface for collision queries required by deterministic character controllers.
    /// </summary>
    public interface ICollisionWorld
    {
        bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, out FPRaycastHit hit);
        bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, out FPRaycastHit hit);
        bool Raycast(FPRay ray, FP maxDistance, out FPRaycastHit hit);
        bool CheckCapsuleOverlap(FPCapsule capsule);
    }

    /// <summary>
    /// Lightweight collision world implementation holding AABBs, Spheres, and Capsules.
    /// </summary>
    public class SimpleCollisionWorld : ICollisionWorld, ICYGMovingPlatformWorld
    {
        public readonly List<FPAABB> Boxes = new List<FPAABB>();
        public readonly List<FPSphere> Spheres = new List<FPSphere>();
        public readonly List<FPCapsule> Capsules = new List<FPCapsule>();
        public readonly List<CYGMovingPlatform> Platforms = new List<CYGMovingPlatform>();

        /// <summary>Static triangle meshes baked from level MeshColliders.</summary>
        public readonly List<CYGStaticMesh> Meshes = new List<CYGStaticMesh>();

        public void AddBox(FPAABB box) => Boxes.Add(box);
        public void AddSphere(FPSphere sphere) => Spheres.Add(sphere);
        public void AddCapsule(FPCapsule capsule) => Capsules.Add(capsule);
        public void AddMovingPlatform(CYGMovingPlatform platform) => Platforms.Add(platform);

        public void AddMesh(CYGStaticMesh mesh)
        {
            if (mesh != null && !Meshes.Contains(mesh))
            {
                Meshes.Add(mesh);
            }
        }

        /// <summary>Adds every mesh registered under the given ids.</summary>
        public void AddMeshes(IList<CYGStaticMesh> meshes)
        {
            if (meshes == null)
            {
                return;
            }
            for (int i = 0; i < meshes.Count; i++)
            {
                AddMesh(meshes[i]);
            }
        }

        /// <summary>Advances kinematic platforms; call once per simulation tick before characters move.</summary>
        public void AdvancePlatforms(FP deltaTime)
        {
            for (int i = 0; i < Platforms.Count; i++)
            {
                CYGMovingPlatform platform = Platforms[i];
                platform.Box.Translate(platform.Velocity * deltaTime);
                Platforms[i] = platform;
            }
        }

        public bool TryGetPlatformVelocity(FPVector3 point, out FPVector3 velocity)
        {
            bool found = false;
            FP top = FP.MinValue;
            velocity = FPVector3.Zero;

            for (int i = 0; i < Platforms.Count; i++)
            {
                CYGMovingPlatform platform = Platforms[i];
                FPAABB probe = platform.Box;
                probe.Expand((FP)0.05f);
                if (probe.Contains(point) && platform.Box.Max.y > top)
                {
                    top = platform.Box.Max.y;
                    velocity = platform.Velocity;
                    found = true;
                }
            }

            return found;
        }

        public void Clear()
        {
            Boxes.Clear();
            Spheres.Clear();
            Capsules.Clear();
            Platforms.Clear();
            Meshes.Clear();
        }

        public bool CapsuleCast(FPCapsule capsule, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP bestDist = FP.MaxValue;

            for (int i = 0; i < Boxes.Count; i++)
            {
                if (FPRaycast.CapsuleCast(capsule, direction, maxDistance, Boxes[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Spheres.Count; i++)
            {
                if (FPRaycast.CapsuleCast(capsule, direction, maxDistance, Spheres[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Capsules.Count; i++)
            {
                if (FPRaycast.CapsuleCast(capsule, direction, maxDistance, Capsules[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Platforms.Count; i++)
            {
                if (FPRaycast.CapsuleCast(capsule, direction, maxDistance, Platforms[i].Box, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Meshes.Count; i++)
            {
                if (Meshes[i].CapsuleCast(capsule, direction, maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            return hit.Hit;
        }

        public bool SphereCast(FPSphere sphere, FPVector3 direction, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP bestDist = FP.MaxValue;

            for (int i = 0; i < Boxes.Count; i++)
            {
                if (FPRaycast.SphereCast(sphere, direction, maxDistance, Boxes[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Spheres.Count; i++)
            {
                if (FPRaycast.SphereCast(sphere, direction, maxDistance, Spheres[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Capsules.Count; i++)
            {
                if (FPRaycast.SphereCast(sphere, direction, maxDistance, Capsules[i], out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Platforms.Count; i++)
            {
                if (FPRaycast.SphereCast(sphere, direction, maxDistance, Platforms[i].Box, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Meshes.Count; i++)
            {
                if (Meshes[i].SphereCast(sphere, direction, maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            return hit.Hit;
        }

        public bool Raycast(FPRay ray, FP maxDistance, out FPRaycastHit hit)
        {
            hit = FPRaycastHit.None;
            FP bestDist = FP.MaxValue;

            for (int i = 0; i < Boxes.Count; i++)
            {
                if (FPRaycast.RaycastAABB(ray, Boxes[i], maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Spheres.Count; i++)
            {
                if (FPRaycast.RaycastSphere(ray, Spheres[i], maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Capsules.Count; i++)
            {
                if (FPRaycast.RaycastCapsule(ray, Capsules[i], maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Platforms.Count; i++)
            {
                if (FPRaycast.RaycastAABB(ray, Platforms[i].Box, maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            for (int i = 0; i < Meshes.Count; i++)
            {
                if (Meshes[i].Raycast(ray, maxDistance, out var h))
                {
                    if (h.Distance < bestDist) { bestDist = h.Distance; hit = h; }
                }
            }

            return hit.Hit;
        }

        public bool CheckCapsuleOverlap(FPCapsule capsule)
        {
            for (int i = 0; i < Boxes.Count; i++)
            {
                if (Boxes[i].Intersects(capsule)) return true;
            }

            for (int i = 0; i < Spheres.Count; i++)
            {
                if (capsule.Intersects(Spheres[i])) return true;
            }

            for (int i = 0; i < Capsules.Count; i++)
            {
                if (capsule.Intersects(Capsules[i])) return true;
            }

            for (int i = 0; i < Platforms.Count; i++)
            {
                if (Platforms[i].Box.Intersects(capsule)) return true;
            }

            for (int i = 0; i < Meshes.Count; i++)
            {
                if (Meshes[i].Overlaps(capsule)) return true;
            }

            return false;
        }
    }

    /// <summary>
    /// Pure C# Kinematic 3D Character Controller based on a capsule collider.
    /// Handles ground detection, step offset climbing, slope sliding, and obstacle deflection.
    /// Position represents the character's base (feet position).
    /// </summary>
    [Serializable]
    public class FPCharacterController
    {
        public FP Radius = (FP)0.5f;
        public FP Height = (FP)2.0f;
        public FP StepOffset = (FP)0.3f;
        public FP SlopeLimit = (FP)45.0f; // degrees
        public FP SkinWidth = (FP)0.02f;
        public FP MinMoveDistance = (FP)0.001f;

        public FPVector3 Position;
        public FPVector3 Velocity;
        public bool IsGrounded;
        public FPVector3 GroundNormal = FPVector3.Up;

        /// <summary>Surfaces touched by the last Move call.</summary>
        public CYGCollisionFlags CollisionFlags { get; private set; }

        /// <summary>Velocity of the platform under the feet, zero when airborne.</summary>
        public FPVector3 PlatformVelocity { get; private set; }

        /// <summary>Normal of the last blocking contact in the last Move call.</summary>
        public FPVector3 LastHitNormal { get; private set; }

        public FPCharacterController()
        {
            Position = FPVector3.Zero;
            Velocity = FPVector3.Zero;
            IsGrounded = false;
        }

        public FPCharacterController(FPVector3 startPosition, FP radius, FP height)
        {
            Position = startPosition;
            Radius = radius;
            Height = height;
            Velocity = FPVector3.Zero;
            IsGrounded = false;
        }

        /// <summary>
        /// Returns the 3D capsule representing this character at the current position.
        /// </summary>
        public FPCapsule GetCapsule()
        {
            FP halfHeight = Height * FP.Half;
            FP segmentHalf = FP.Max(FP.Zero, halfHeight - Radius);
            FPVector3 center = Position + FPVector3.Up * halfHeight;
            FPVector3 pointA = center - FPVector3.Up * segmentHalf;
            FPVector3 pointB = center + FPVector3.Up * segmentHalf;
            return new FPCapsule(pointA, pointB, Radius);
        }

        /// <summary>
        /// Performs ground check below the character.
        /// </summary>
        public bool CheckGrounded(ICollisionWorld world)
        {
            PlatformVelocity = FPVector3.Zero;
            if (world == null)
            {
                IsGrounded = false;
                GroundNormal = FPVector3.Up;
                return false;
            }

            FPCapsule capsule = GetCapsule();
            FP checkDist = SkinWidth + (FP)0.05f;

            if (world.CapsuleCast(capsule, FPVector3.Down, checkDist, out var hit))
            {
                FP slopeAngle = FPVector3.Angle(hit.Normal, FPVector3.Up);
                GroundNormal = hit.Normal;
                if (slopeAngle <= SlopeLimit)
                {
                    IsGrounded = true;
                    CollisionFlags |= CYGCollisionFlags.Below;

                    ICYGMovingPlatformWorld moving = world as ICYGMovingPlatformWorld;
                    FPVector3 velocity;
                    if (moving != null && moving.TryGetPlatformVelocity(Position + FPVector3.Down * SkinWidth, out velocity))
                    {
                        PlatformVelocity = velocity;
                    }
                    return true;
                }

                IsGrounded = false;
                return false;
            }

            IsGrounded = false;
            GroundNormal = FPVector3.Up;
            return false;
        }

        private static CYGCollisionFlags ClassifyNormal(FPVector3 normal)
        {
            if (normal.y > FP.Half)
            {
                return CYGCollisionFlags.Below;
            }
            if (normal.y < -FP.Half)
            {
                return CYGCollisionFlags.Above;
            }
            return CYGCollisionFlags.Sides;
        }

        /// <summary>
        /// Moves the character by the specified displacement vector using kinematic sweep and slide.
        /// </summary>
        public void Move(FPVector3 displacement, ICollisionWorld world)
        {
            CollisionFlags = CYGCollisionFlags.None;
            PlatformVelocity = FPVector3.Zero;

            if (world == null)
            {
                Position += displacement;
                return;
            }

            if (displacement.SqrMagnitude < MinMoveDistance * MinMoveDistance)
            {
                Position += displacement;
                CheckGrounded(world);
                return;
            }

            // Depenetration: if the capsule starts inside geometry, lift it
            // out before sweeping. Without this a spawn embedded in the floor
            // produces zero-distance hits and the character can never move.
            FPCapsule currentCapsule = GetCapsule();
            if (!IsGrounded && world.CheckCapsuleOverlap(currentCapsule))
            {
                FP liftStep = (StepOffset + SkinWidth) / FP.FromInt(4);
                for (int i = 0; i < 4; i++)
                {
                    Position.y += liftStep;
                    currentCapsule = GetCapsule();
                    if (!world.CheckCapsuleOverlap(currentCapsule))
                    {
                        break;
                    }
                }
            }

            FPVector3 remainingMove = displacement;
            const int MaxSlideIterations = 4;

            for (int iteration = 0; iteration < MaxSlideIterations; iteration++)
            {
                FP moveDist = remainingMove.Magnitude;
                if (moveDist < MinMoveDistance)
                    break;

                FPVector3 moveDir = remainingMove / moveDist;
                bool isHorizontal = FP.Abs(moveDir.y) < (FP)0.3f;

                // Sweep a capsule lifted by SkinWidth. A capsule resting
                // exactly on the floor otherwise reports a zero-distance hit
                // against the floor and stops horizontal movement completely.
                FP sweepLift = isHorizontal ? SkinWidth : FP.Zero;
                FPCapsule capsule = GetCapsule();
                if (sweepLift > FP.Zero)
                {
                    capsule.Translate(FPVector3.Up * sweepLift);
                }

                if (world.CapsuleCast(capsule, moveDir, moveDist + SkinWidth, out var hit))
                {
                    LastHitNormal = hit.Normal;
                    CollisionFlags |= ClassifyNormal(hit.Normal);
                    FP stepDistance = FP.Max(FP.Zero, hit.Distance - SkinWidth);
                    FP remainingDist = moveDist - stepDistance;

                    // Try the full remaining displacement from the pre-slide
                    // capsule. Probing from the stopped corner position would
                    // start inside the step's expanded AABB and fail.
                    bool stepped = false;
                    if (isHorizontal && IsGrounded && StepOffset > FP.Zero)
                    {
                        stepped = TryStep(ref remainingMove, world, capsule, sweepLift);
                    }

                    if (!stepped)
                    {
                        Position += moveDir * stepDistance;

                        // Deflect along the obstacle plane
                        remainingMove = FPVector3.ProjectOnPlane(moveDir * remainingDist, hit.Normal);

                        // If on a steep slope, slide downwards
                        FP slopeAngle = FPVector3.Angle(hit.Normal, FPVector3.Up);
                        if (slopeAngle > SlopeLimit && hit.Normal.y > FP.Zero)
                        {
                            FPVector3 slideDown = FPVector3.ProjectOnPlane(FPVector3.Down, hit.Normal).Normalized;
                            remainingMove += slideDown * (remainingDist * (FP)0.5f);
                        }
                    }
                }
                else
                {
                    // No obstacle hit: advance fully
                    Position += remainingMove;
                    break;
                }
            }

            // Post-move ground detection
            CheckGrounded(world);
        }

        private bool TryStep(
            ref FPVector3 horizontalMove,
            ICollisionWorld world,
            FPCapsule startCapsule,
            FP startLift)
        {
            // 1. Cast up. Start from the already-lifted sweep capsule.
            FP upDist = StepOffset;
            if (world.CapsuleCast(startCapsule, FPVector3.Up, upDist, out var upHit))
            {
                upDist = FP.Max(FP.Zero, upHit.Distance - SkinWidth);
                if (upDist < (FP)0.05f) return false; // not enough head clearance
            }

            FPCapsule elevatedCapsule = startCapsule;
            elevatedCapsule.Translate(FPVector3.Up * upDist);

            // 2. Cast forward at the elevated height.
            FP fwdDist = horizontalMove.Magnitude;
            if (fwdDist < MinMoveDistance) return false;
            FPVector3 fwdDir = horizontalMove / fwdDist;

            if (world.CapsuleCast(elevatedCapsule, fwdDir, fwdDist, out var fwdHit))
            {
                fwdDist = FP.Max(FP.Zero, fwdHit.Distance - SkinWidth);
                if (fwdDist < MinMoveDistance) return false;
            }

            FPCapsule fwdCapsule = elevatedCapsule;
            fwdCapsule.Translate(fwdDir * fwdDist);

            // 3. Cast down to land on the step surface. The start capsule sits
            // startLift above the feet, so the vertical delta must subtract it.
            FP downMax = startLift + upDist + SkinWidth;
            if (world.CapsuleCast(fwdCapsule, FPVector3.Down, downMax, out var downHit))
            {
                FP slopeAngle = FPVector3.Angle(downHit.Normal, FPVector3.Up);
                if (slopeAngle <= SlopeLimit)
                {
                    FP downDist = FP.Min(downHit.Distance, startLift + upDist);
                    Position += FPVector3.Up * (startLift + upDist - downDist)
                              + fwdDir * fwdDist;
                    horizontalMove = FPVector3.Zero;
                    CollisionFlags |= CYGCollisionFlags.Below;
                    return true;
                }
            }

            return false;
        }

        /// <summary>
        /// Simple movement update with gravity and delta time.
        /// </summary>
        public void SimpleMove(FPVector3 targetVelocity, FP deltaTime, ICollisionWorld world, FP gravity)
        {
            FPVector3 carry = IsGrounded ? PlatformVelocity : FPVector3.Zero;

            if (IsGrounded)
            {
                Velocity = targetVelocity;
                Velocity.y = FP.Zero;
            }
            else
            {
                Velocity.x = targetVelocity.x;
                Velocity.z = targetVelocity.z;
                Velocity.y -= gravity * deltaTime;
            }

            FPVector3 motion = Velocity * deltaTime + carry * deltaTime;
            Move(motion, world);
        }
    }
}
