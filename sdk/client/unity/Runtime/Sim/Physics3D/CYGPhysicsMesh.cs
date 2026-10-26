//-----------------------------------------------------------------------------
// CYGPhysicsMesh.cs
// Static triangle-mesh bodies for the deterministic 3D physics world.
// Zero UnityEngine dependencies.
//-----------------------------------------------------------------------------

using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    public sealed partial class CYGPhysicsWorld
    {
        private readonly Dictionary<int, int> _staticMeshBodies = new Dictionary<int, int>();

        /// <summary>
        /// Number of baked static meshes currently registered as physics bodies.
        /// </summary>
        public int StaticMeshBodyCount
        {
            get { return _staticMeshBodies.Count; }
        }

        /// <summary>
        /// Adds a baked mesh as a static body. The mesh is registered in
        /// <see cref="CYGMeshRegistry"/> so narrowphase can resolve its triangles,
        /// and the returned body id can be used for queries and removal. Calling
        /// this twice with the same mesh returns the existing body id.
        /// </summary>
        public int AddStaticMesh(CYGStaticMesh mesh)
        {
            if (mesh == null)
                throw new ArgumentNullException(nameof(mesh));
            if (mesh.Id == 0)
                throw new ArgumentException("Static meshes need a non-zero stable id.", nameof(mesh));

            int existingId;
            if (_staticMeshBodies.TryGetValue(mesh.Id, out existingId))
            {
                CYGPhysicsBody existing = GetBody(existingId);
                if (existing != null && existing.IsActive)
                    return existingId;
            }

            CYGMeshRegistry.Register(mesh);

            CYGPhysicsBody body = CYGPhysicsBody.CreateStatic(CYGCollider.Mesh(mesh));
            body.Position = mesh.Bounds.Center;
            body.Friction = FP.FromFraction(1, 2);
            body.Restitution = FP.Zero;

            int id = AddBody(body);
            _staticMeshBodies[mesh.Id] = id;
            return id;
        }

        /// <summary>
        /// Removes the body created by <see cref="AddStaticMesh"/>. The mesh stays
        /// in the registry because it is immutable content shared by other worlds.
        /// </summary>
        public bool RemoveStaticMesh(int meshId)
        {
            int bodyId;
            if (!_staticMeshBodies.TryGetValue(meshId, out bodyId))
                return false;

            _staticMeshBodies.Remove(meshId);
            return RemoveBody(bodyId);
        }
    }
}
