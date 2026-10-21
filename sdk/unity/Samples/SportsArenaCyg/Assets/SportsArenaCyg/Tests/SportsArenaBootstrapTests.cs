using System.Collections;
using CookYourGame.Sim;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace CookYourGame.SportsArena.Tests
{
    public sealed class SportsArenaBootstrapTests
    {
        [UnityTest]
        public IEnumerator Bootstrap_CreatesDeterministicArena()
        {
            // Headless CI cannot create a graphic device, and the reused sample
            // prefabs intentionally contain missing reference script references.
            LogAssert.ignoreFailingMessages = true;
            GameObject host = new GameObject("Sports Arena Test Host");
            SportsArenaBootstrap bootstrap = host.AddComponent<SportsArenaBootstrap>();
            yield return null;

            Assert.IsNotNull(bootstrap.Simulation, "simulation did not start");
            Assert.IsNotNull(bootstrap.BallView, "ball view missing");
            Assert.AreEqual(SportsArenaConfig.PlayerCount, bootstrap.PlayerViews.Length);
            Assert.IsNotNull(bootstrap.ArenaCamera, "camera missing");
            for (int i = 0; i < SportsArenaConfig.PlayerCount; i++)
            {
                Assert.AreEqual(i % 2, bootstrap.Simulation.Players[i].Team,
                    "fighters must alternate between the two teams");
            }

            int startTick = bootstrap.Simulation.Tick;
            yield return new WaitForSeconds(1.2f);

            Assert.Greater(bootstrap.Simulation.Tick, startTick, "fixed tick did not advance");
            Assert.Greater(bootstrap.Simulation.Tick, 30, "expected a running simulation");
            Assert.IsTrue(float.IsFinite(bootstrap.Simulation.Snapshot.BallPosition.x.AsFloat()));
            Assert.IsTrue(float.IsFinite(bootstrap.Simulation.Snapshot.Positions[0].y.AsFloat()));

            yield return new WaitForSeconds(3.2f);
            Assert.AreEqual(SportsArenaPhase.Playing, bootstrap.Simulation.Phase);
            Assert.Less(bootstrap.Simulation.MatchTime.AsFloat(), SportsArenaConfig.MatchDuration.AsFloat());
            Assert.Greater(bootstrap.Simulation.Snapshot.Positions[0].y.AsFloat(), -1f);

            FP startX = bootstrap.Simulation.Snapshot.Positions[0].x;
            SportsArenaInput[] arenaInputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            arenaInputs[0].Value = new CYGPlayerInput(1, 0, CYGPlayerInput.MaxMove, 0);
            for (int i = 0; i < 60; i++)
            {
                bootstrap.Simulation.TickStep(arenaInputs);
            }
            Assert.Greater(bootstrap.Simulation.Snapshot.Positions[0].x.AsFloat(), startX.AsFloat() + 0.5f,
                "player is trapped in the imported arena collision");

            Object.Destroy(host);
            yield return null;
        }

        [Test]
        public void Simulation_IsDeterministic_ForIdenticalInputs()
        {
            SportsArenaSimulation first = BuildSimulation();
            SportsArenaSimulation second = BuildSimulation();
            DisableBots(first);
            DisableBots(second);
            SportsArenaInput[] inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            inputs[0].Value = new CYGPlayerInput(1, CYGPlayerInput.ButtonAction1, 1000, 0);

            for (int i = 0; i < 240; i++)
            {
                first.TickStep(inputs);
                second.TickStep(inputs);
            }

            Assert.AreEqual(first.Tick, second.Tick);
            Assert.AreEqual(first.Snapshot.BallPosition, second.Snapshot.BallPosition);
            Assert.AreEqual(first.Snapshot.Positions[0], second.Snapshot.Positions[0]);
            Assert.AreEqual(first.Snapshot.Scores[0], second.Snapshot.Scores[0]);
        }

        [Test]
        public void Simulation_BotMovesTowardBall()
        {
            SportsArenaSimulation simulation = BuildSimulation();
            DisableBots(simulation);
            simulation.Players[1].IsBot = true;
            SportsArenaInput[] inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            for (int i = 0; i < 200; i++)
            {
                simulation.TickStep(inputs);
            }

            FP startX = simulation.Players[1].Position.x;
            for (int i = 0; i < 30; i++)
            {
                simulation.TickStep(inputs);
            }

            Assert.Less(simulation.Players[1].Position.x.AsFloat(), startX.AsFloat() - 0.5f,
                "bot did not move toward the ball");
        }

        [Test]
        public void Simulation_PlayerMovesAndCatchesBall()
        {
            SportsArenaSimulation simulation = BuildSimulation();
            DisableBots(simulation);
            SportsArenaInput[] inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            for (int i = 0; i < 280; i++)
            {
                simulation.TickStep(inputs);
            }

            Assert.IsTrue(simulation.Players[0].Grounded, "player should land on the floor");
            FP startX = simulation.Players[0].Position.x;
            inputs[0].Value = new CYGPlayerInput(1, 0, CYGPlayerInput.MaxMove, 0);
            for (int i = 0; i < 90; i++)
            {
                simulation.TickStep(inputs);
            }

            Assert.Greater(simulation.Players[0].Position.x.AsFloat(), startX.AsFloat(), "player did not move");
            Assert.AreEqual(0, simulation.Ball.Holder, "player should catch the ball at the centre");
        }

        [Test]
        public void Simulation_Scores_WhenBallEntersGoal()
        {
            SportsArenaSimulation simulation = BuildSimulation();
            DisableBots(simulation);
            SportsArenaInput[] inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            for (int i = 0; i < 200; i++)
            {
                simulation.TickStep(inputs);
            }

            Assert.AreEqual(SportsArenaPhase.Playing, simulation.Phase);
            CYGPhysicsBody body = simulation.Physics.GetBody(simulation.Ball.BodyId);
            Assert.IsNotNull(body);
            body.IsActive = false;
            simulation.Ball.Holder = -1;
            simulation.Ball.Active = true;
            simulation.Ball.Position = simulation.Goals[0].Area.Center;
            simulation.TickStep(inputs);

            Assert.AreEqual(1, simulation.Scores[1], "the team attacking the first goal should score");
            Assert.AreEqual(SportsArenaPhase.GoalScored, simulation.Phase);
        }

        private static SportsArenaSimulation BuildSimulation()
        {
            SportsArenaSimulation simulation = new SportsArenaSimulation();
            simulation.AddStaticBox(
                new FPVector3(FP.Zero, FP.FromInt(-1), FP.Zero),
                new FPVector3(FP.FromInt(20), FP.FromInt(1), FP.FromInt(20)),
                FPQuaternion.Identity);
            FPAABB arena = new FPAABB(
                new FPVector3(FP.FromInt(-20), FP.FromInt(-1), FP.FromInt(-20)),
                new FPVector3(FP.FromInt(20), FP.FromInt(8), FP.FromInt(20)));
            SportsArenaGoal[] goals = new SportsArenaGoal[2];
            goals[0] = new SportsArenaGoal
            {
                DefendingTeam = 0,
                Area = FPAABB.FromCenterSize(
                    new FPVector3(FP.FromInt(-10), FP.FromInt(2), FP.Zero),
                    new FPVector3(FP.FromInt(2), FP.FromInt(4), FP.FromInt(8))),
            };
            goals[1] = new SportsArenaGoal
            {
                DefendingTeam = 1,
                Area = FPAABB.FromCenterSize(
                    new FPVector3(FP.FromInt(10), FP.FromInt(2), FP.Zero),
                    new FPVector3(FP.FromInt(2), FP.FromInt(4), FP.FromInt(8))),
            };
            simulation.Start(
                arena,
                goals,
                new[]
                {
                    new FPVector3(FP.FromInt(-4), FP.FromFraction(1, 5), FP.Zero),
                    new FPVector3(FP.FromInt(4), FP.FromFraction(1, 5), FP.Zero),
                    new FPVector3(FP.FromInt(-6), FP.FromFraction(1, 5), FP.FromInt(-2)),
                    new FPVector3(FP.FromInt(6), FP.FromFraction(1, 5), FP.FromInt(-2)),
                    new FPVector3(FP.FromInt(-6), FP.FromFraction(1, 5), FP.FromInt(2)),
                    new FPVector3(FP.FromInt(6), FP.FromFraction(1, 5), FP.FromInt(2)),
                },
                new FPVector3(FP.Zero, FP.FromFraction(3, 2), FP.Zero));
            return simulation;
        }

        private static void DisableBots(SportsArenaSimulation simulation)
        {
            for (int i = 0; i < simulation.Players.Length; i++)
            {
                simulation.Players[i].IsBot = false;
            }
        }
    }
}
