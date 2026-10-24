using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Scene authoring marker for one deterministic entity. The scene is baked
    /// at startup into ECS entities in a stable order (bake order, then
    /// hierarchy path), so every client that loads the same scene creates the
    /// same slot layout.
    /// </summary>
    [DisallowMultipleComponent]
    [AddComponentMenu("CookYourGame/Sim/CYG Entity Authoring")]
    public sealed class CYGEntityAuthoring : MonoBehaviour
    {
        [Header("Identity")]
        [Tooltip("Relative bake order. Ties are broken by hierarchy path.")]
        [SerializeField] private int bakeOrder = 0;

        [Tooltip("Deterministic player id. 0 for props that no player controls.")]
        [SerializeField] private int playerId = 0;

        [Tooltip("Marks the entity driven by the local input collector. Its player id is replaced by the network actor id on connect.")]
        [SerializeField] private bool localPlayer = false;

        [SerializeField] private int health = 100;

        [Header("Movement")]
        [Tooltip("Adds the built-in kinematic movement state (CYGSimAgent).")]
        [SerializeField] private bool addSimAgent = true;

        [Header("Visual")]
        [Tooltip("Prefab instantiated for this entity. Leave empty to use a prefab key, the updater default, or skip static props.")]
        [SerializeField] private GameObject viewPrefab;

        [Tooltip("Key resolved through the runner's CYGEntityViewPrefabRegistry.")]
        [SerializeField] private string viewPrefabKey;

        [Header("Physics 3D")]
        [SerializeField] private bool addPhysics3D;
        [SerializeField] private CYGBodyType bodyType3D = CYGBodyType.Dynamic;
        [SerializeField] private CYGShapeType shape3D = CYGShapeType.Box;
        [SerializeField] private Vector3 halfExtents3D = new Vector3(0.5f, 0.5f, 0.5f);
        [SerializeField] private float radius3D = 0.5f;
        [SerializeField] private float height3D = 1f;
        [SerializeField] private float mass3D = 1f;

        [Header("Physics 2D")]
        [SerializeField] private bool addPhysics2D;
        [SerializeField] private CYGPhysics2DBodyType bodyType2D = CYGPhysics2DBodyType.Dynamic;
        [SerializeField] private CYGPhysics2DShapeType shape2D = CYGPhysics2DShapeType.Box;
        [SerializeField] private Vector2 halfExtents2D = new Vector2(0.5f, 0.5f);
        [SerializeField] private float radius2D = 0.5f;
        [SerializeField] private float mass2D = 1f;

        [Header("Navigation")]
        [SerializeField] private bool addNavAgent;
        [SerializeField] private float agentRadius = 0.5f;
        [SerializeField] private float agentMaxSpeed = 6f;
        [SerializeField] private float agentAcceleration = 20f;

        public int BakeOrder => bakeOrder;
        public int PlayerId => playerId;
        public bool IsLocalPlayer => localPlayer;
        public GameObject ViewPrefab => viewPrefab;
        public string ViewPrefabKey => viewPrefabKey;

        /// <summary>Creates this entity's components in the given world.</summary>
        public CYGEntityRef Bake(CYGWorld world, int effectivePlayerId)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }

            int id = localPlayer ? effectivePlayerId : playerId;
            FPVector3 position = ToFixed(transform.position);
            FPQuaternion rotation = ToFixed(transform.rotation);

            CYGEntityRef entity = world.CreateEntity();

            if (addSimAgent)
            {
                CYGSimAgent agent = CYGSimAgent.Create(id, position, health);
                agent.Rotation = rotation;
                world.Set(entity, agent);
            }

            if (addPhysics3D)
            {
                CYGPhysicsBody3D body = CreateBody3D(id, position);
                body.Rotation = rotation;
                world.Set(entity, body);
            }

            if (addPhysics2D)
            {
                CYGPhysicsBody2D body = CreateBody2D(id, position);
                body.Angle = FP.FromFloat(transform.eulerAngles.z) * FP.Deg2Rad;
                world.Set(entity, body);
            }

            if (addNavAgent)
            {
                CYGNavAgent agent = CYGNavAgent.Create(id, position);
                agent.Radius = FP.FromFloat(agentRadius);
                agent.MaxSpeed = FP.FromFloat(agentMaxSpeed);
                agent.Acceleration = FP.FromFloat(agentAcceleration);
                world.Set(entity, agent);
            }

            return entity;
        }

        private CYGPhysicsBody3D CreateBody3D(int id, FPVector3 position)
        {
            if (bodyType3D == CYGBodyType.Static)
            {
                return shape3D == CYGShapeType.Box
                    ? CYGPhysicsBody3D.StaticBox(position, ToFixed(halfExtents3D))
                    : CYGPhysicsBody3D.StaticSphere(position, FP.FromFloat(radius3D));
            }

            switch (shape3D)
            {
                case CYGShapeType.Sphere:
                    return CYGPhysicsBody3D.DynamicSphere(id, position, FP.FromFloat(radius3D), FP.FromFloat(mass3D));
                case CYGShapeType.Capsule:
                    return CYGPhysicsBody3D.DynamicCapsule(
                        id,
                        position,
                        FP.FromFloat(radius3D),
                        FP.FromFloat(height3D),
                        FP.FromFloat(mass3D));
                default:
                    return CYGPhysicsBody3D.DynamicBox(id, position, ToFixed(halfExtents3D), FP.FromFloat(mass3D));
            }
        }

        private CYGPhysicsBody2D CreateBody2D(int id, FPVector3 position)
        {
            FPVector2 flat = new FPVector2(position.x, position.y);
            if (bodyType2D == CYGPhysics2DBodyType.Static)
            {
                return shape2D == CYGPhysics2DShapeType.Box
                    ? CYGPhysicsBody2D.StaticBox(flat, ToFixed(halfExtents2D))
                    : CYGPhysicsBody2D.StaticCircle(flat, FP.FromFloat(radius2D));
            }

            return shape2D == CYGPhysics2DShapeType.Box
                ? CYGPhysicsBody2D.DynamicBox(id, flat, ToFixed(halfExtents2D), FP.FromFloat(mass2D))
                : CYGPhysicsBody2D.DynamicCircle(id, flat, FP.FromFloat(radius2D), FP.FromFloat(mass2D));
        }

        private static FPVector3 ToFixed(Vector3 value)
        {
            return new FPVector3(
                FP.FromFloat(value.x),
                FP.FromFloat(value.y),
                FP.FromFloat(value.z));
        }

        private static FPVector2 ToFixed(Vector2 value)
        {
            return new FPVector2(FP.FromFloat(value.x), FP.FromFloat(value.y));
        }

        private static FPQuaternion ToFixed(Quaternion value)
        {
            return new FPQuaternion(
                FP.FromFloat(value.x),
                FP.FromFloat(value.y),
                FP.FromFloat(value.z),
                FP.FromFloat(value.w));
        }
    }

    /// <summary>Result of baking one scene authoring component.</summary>
    public struct CYGBakedEntity
    {
        public CYGEntityRef Entity;
        public CYGEntityAuthoring Authoring;
    }

    /// <summary>
    /// Deterministic scene bake. Authoring components are ordered by explicit
    /// bake order and then by hierarchy path, never by Unity instance id, so
    /// separate clients create identical entity slots.
    /// </summary>
    public static class CYGSceneBaker
    {
        public static void Bake(
            CYGWorld world,
            IList<CYGEntityAuthoring> authoring,
            int localPlayerId,
            List<CYGBakedEntity> output)
        {
            if (world == null)
            {
                throw new ArgumentNullException(nameof(world));
            }
            if (authoring == null)
            {
                throw new ArgumentNullException(nameof(authoring));
            }
            if (output == null)
            {
                throw new ArgumentNullException(nameof(output));
            }

            output.Clear();

            List<CYGEntityAuthoring> ordered = new List<CYGEntityAuthoring>(authoring.Count);
            for (int i = 0; i < authoring.Count; i++)
            {
                if (authoring[i] != null)
                {
                    ordered.Add(authoring[i]);
                }
            }

            StableSortByPath(ordered);

            for (int i = 0; i < ordered.Count; i++)
            {
                CYGEntityAuthoring entry = ordered[i];
                CYGEntityRef entity = entry.Bake(world, localPlayerId);
                output.Add(new CYGBakedEntity { Entity = entity, Authoring = entry });
            }
        }

        private static void StableSortByPath(List<CYGEntityAuthoring> entries)
        {
            for (int i = 1; i < entries.Count; i++)
            {
                CYGEntityAuthoring current = entries[i];
                int keyOrder = current.BakeOrder;
                string keyPath = PathOf(current.transform);
                int j = i - 1;
                while (j >= 0)
                {
                    int otherOrder = entries[j].BakeOrder;
                    if (otherOrder < keyOrder
                        || (otherOrder == keyOrder
                            && string.CompareOrdinal(PathOf(entries[j].transform), keyPath) <= 0))
                    {
                        break;
                    }
                    entries[j + 1] = entries[j];
                    j--;
                }
                entries[j + 1] = current;
            }
        }

        private static string PathOf(Transform transform)
        {
            string path = transform.name;
            Transform parent = transform.parent;
            while (parent != null)
            {
                path = parent.name + "/" + path;
                parent = parent.parent;
            }
            return path;
        }
    }
}
