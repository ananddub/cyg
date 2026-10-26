using CookYourGame.Sim;
using NUnit.Framework;
using UnityEngine;

namespace CookYourGame.Tests
{
    /// <summary>
    /// Unity PlayMode coverage for the deterministic 3D physics path: oriented
    /// boxes, rotated mesh contacts, runtime mesh baking and multi-client
    /// determinism. These tests use the real Unity runtime rather than only the
    /// headless test harness.
    /// </summary>
    public sealed class CYGOrientedPhysicsPlayModeTests
    {
        private static readonly FP Half = FP.FromFraction(1, 2);

        [Test]
        public void OrientedBoxRaycastUsesRotatedExtents()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            CYGPhysicsBody box = CYGPhysicsBody.CreateStatic(CYGCollider.Box(new FPVector3(Half, Half, Half)));
            box.Rotation = FPQuaternion.AngleAxis(FP.FromInt(45), FPVector3.Up);
            world.AddBody(box);

            CYGPhysicsHit hit = world.Raycast(new FPRay(new FPVector3(FP.FromInt(3), FP.Zero, FP.Zero), FPVector3.Left), FP.FromInt(10));

            Assert.IsTrue(hit.Hit);
            Assert.Less((float)FP.Abs(hit.Distance - (FP.FromInt(3) - FP.Sqrt(Half))), 0.01f);
            Assert.IsTrue(hit.Normal.x > FP.Zero);
        }

        [Test]
        public void DynamicBoxRestsOnRotatedStaticMesh()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld(new CYGPhysicsSettings
            {
                EnableSleeping = false,
                EnableCcd = true,
            });

            FPVector3[] vertices =
            {
                new FPVector3(-FP.FromInt(5), FP.Zero, -FP.FromInt(5)),
                new FPVector3(FP.FromInt(5), FP.Zero, -FP.FromInt(5)),
                new FPVector3(FP.FromInt(5), FP.Zero, FP.FromInt(5)),
                new FPVector3(-FP.FromInt(5), FP.Zero, FP.FromInt(5)),
            };
            CYGStaticMesh mesh = new CYGStaticMesh(9101, "PlayModeFloor", vertices, new[] { 0, 2, 1, 0, 3, 2 });
            world.AddStaticMesh(mesh);

            CYGPhysicsBody box = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Box(new FPVector3(Half, Half, Half)), FP.One);
            box.Position = new FPVector3(FP.FromInt(-2), FP.FromInt(2), FP.Zero);
            box.Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Up);
            box.Friction = FP.FromFraction(7, 10);
            int boxId = world.AddBody(box);

            for (int i = 0; i < 300; i++)
                world.Step();

            CYGPhysicsBody result = world.GetBody(boxId);
            Assert.Less((float)FP.Abs(result.Position.y - Half), 0.05f);
            Assert.Less((float)FP.Abs(result.Position.x + FP.FromInt(2)), 0.1f);
        }

        [Test]
        public void YawedBoxStacksOnYawedBox()
        {
            CYGPhysicsWorld world = new CYGPhysicsWorld();
            CYGPhysicsBody baseBox = CYGPhysicsBody.CreateStatic(CYGCollider.Box(new FPVector3(Half, Half, Half)));
            baseBox.Rotation = FPQuaternion.AngleAxis(FP.FromInt(30), FPVector3.Up);
            baseBox.Friction = FP.FromFraction(4, 5);
            world.AddBody(baseBox);

            CYGPhysicsBody box = CYGPhysicsBody.CreateDynamic(
                CYGCollider.Box(new FPVector3(Half, Half, Half)), FP.One);
            box.Position = new FPVector3(FP.Zero, FP.FromInt(2), FP.Zero);
            box.Rotation = FPQuaternion.AngleAxis(FP.FromInt(10), FPVector3.Up);
            box.Friction = FP.FromFraction(7, 10);
            int boxId = world.AddBody(box);

            for (int i = 0; i < 300; i++)
                world.Step();

            CYGPhysicsBody result = world.GetBody(boxId);
            Assert.Less((float)FP.Abs(result.Position.y - FP.One), 0.05f);
            Assert.Less((float)FP.Abs(result.Position.x), 0.05f);
            Assert.Less((float)FP.Abs(result.Position.z), 0.05f);
        }

        [Test]
        public void StaticMeshAuthoringBakesReadableUnityMesh()
        {
            GameObject root = new GameObject("CYG PlayMode Bake");
            Mesh mesh = new Mesh();
            mesh.name = "RuntimeFloor";
            mesh.vertices = new[]
            {
                new Vector3(-1f, 0f, -1f),
                new Vector3(1f, 0f, -1f),
                new Vector3(1f, 0f, 1f),
                new Vector3(-1f, 0f, 1f),
            };
            mesh.triangles = new[] { 0, 2, 1, 0, 3, 2 };
            mesh.RecalculateNormals();

            MeshFilter filter = root.AddComponent<MeshFilter>();
            filter.sharedMesh = mesh;
            MeshCollider collider = root.AddComponent<MeshCollider>();
            collider.sharedMesh = mesh;
            CYGStaticMeshAuthoring authoring = root.AddComponent<CYGStaticMeshAuthoring>();

            CYGStaticMesh baked = authoring.Bake(false);

            Assert.IsNotNull(baked);
            Assert.AreEqual(4, baked.VertexCount);
            Assert.AreEqual(2, baked.TriangleCount);
            Assert.AreEqual(FP.Zero, baked.Bounds.Min.y);
            Assert.AreEqual(FP.Zero, baked.Bounds.Max.y);

            Object.DestroyImmediate(root);
            Object.DestroyImmediate(mesh);
        }

        [Test]
        public void MultiClientMatchStaysDeterministicInPlayMode()
        {
            CYGMultiClientMatch match = new CYGMultiClientMatch(new CYGMultiClientOptions
            {
                ClientCount = 2,
                PlayersPerClient = 1,
                Ticks = 60,
                Seed = 4242UL,
            });

            match.RunToEnd();

            Assert.IsFalse(match.Diverged, match.Divergence);
            Assert.AreEqual(60, match.Tick);
        }
    }
}
