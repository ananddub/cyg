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
                Assert.AreEqual(first.ComputeStateHash(), second.ComputeStateHash(),
                    "state hash diverged at step " + i);
            }

            Assert.AreEqual(first.Tick, second.Tick);
            Assert.AreEqual(first.Snapshot.BallPosition, second.Snapshot.BallPosition);
            Assert.AreEqual(first.Snapshot.Positions[0], second.Snapshot.Positions[0]);
            Assert.AreEqual(first.Snapshot.Scores[0], second.Snapshot.Scores[0]);
        }

        [Test]
        public void Simulation_StateHash_DetectsDifferentInput()
        {
            SportsArenaSimulation first = BuildSimulation();
            SportsArenaSimulation second = BuildSimulation();
            DisableBots(first);
            DisableBots(second);
            SportsArenaInput[] neutral = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            SportsArenaInput[] moving = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            moving[0].Value = new CYGPlayerInput(1, 0, CYGPlayerInput.MaxMove, 0);
            for (int i = 0; i < 300; i++)
            {
                first.TickStep(neutral);
                second.TickStep(moving);
            }

            Assert.AreNotEqual(first.ComputeStateHash(), second.ComputeStateHash());
        }

        [Test]
        public void Simulation_ResetForMatch_ReturnsToSameInitialHash()
        {
            SportsArenaSimulation first = BuildSimulation();
            SportsArenaSimulation second = BuildSimulation();
            DisableBots(first);
            DisableBots(second);
            SportsArenaInput[] inputs = new SportsArenaInput[SportsArenaConfig.PlayerCount];
            inputs[0].Value = new CYGPlayerInput(1, 0, CYGPlayerInput.MaxMove, 0);
            for (int i = 0; i < 240; i++)
            {
                first.TickStep(inputs);
            }
            for (int i = 0; i < 60; i++)
            {
                second.TickStep(inputs);
            }

            first.ResetForMatch();
            second.ResetForMatch();
            Assert.AreEqual(0, first.Tick);
            Assert.AreEqual(0, second.Tick);
            Assert.AreEqual(first.ComputeStateHash(), second.ComputeStateHash(),
                "both peers must produce the same tick-0 state after reset");
        }

        [Test]
        public void Lockstep_GateRequiresBothInputsForExactTick()
        {
            SportsArenaLockstep lockstep = new SportsArenaLockstep();
            lockstep.ResetForStart(1, 2, 0, 1, 1, 2);

            Assert.IsFalse(lockstep.TryGetTickInputs(0, out _, out _));
            Assert.IsTrue(lockstep.StoreLocal(0, new CYGPlayerInput(0, 0, 1, 0)));
            Assert.IsFalse(lockstep.TryGetTickInputs(0, out _, out _));
            Assert.IsTrue(lockstep.StoreRemote(1, new CYGPlayerInput(1, 0, 0, 0)));
            Assert.IsFalse(lockstep.TryGetTickInputs(0, out _, out _),
                "a future remote input must not satisfy the current tick");
            Assert.IsTrue(lockstep.StoreRemote(0, new CYGPlayerInput(0, 0, 0, 0)));
            Assert.IsTrue(lockstep.TryGetTickInputs(0, out CYGPlayerInput local, out CYGPlayerInput remote));
            Assert.AreEqual(1, local.MoveX);
            Assert.AreEqual(0, remote.MoveX);
        }

        [Test]
        public void Lockstep_RejectsRemoteTicksOutsideWindow()
        {
            SportsArenaLockstep lockstep = new SportsArenaLockstep();
            lockstep.ResetForStart(1, 2, 0, 1, 1, 2);

            Assert.IsFalse(lockstep.StoreRemote(-1, new CYGPlayerInput(-1, 0, 0, 0)));
            Assert.IsFalse(lockstep.StoreRemote(1000, new CYGPlayerInput(1000, 0, 0, 0)));
            Assert.IsFalse(lockstep.StoreRemote(65, new CYGPlayerInput(65, 0, 0, 0)));
            Assert.IsTrue(lockstep.StoreRemote(64, new CYGPlayerInput(64, 0, 0, 0)));
            Assert.IsTrue(lockstep.StoreRemote(3, new CYGPlayerInput(3, 0, 0, 0)));
        }

        [Test]
        public void Lockstep_DetectsHashMismatch()
        {
            SportsArenaLockstep lockstep = new SportsArenaLockstep();
            lockstep.ResetForStart(1, 2, 0, 1, 1, 2);

            Assert.IsTrue(lockstep.RecordLocalHash(60, 0xAAAA0001u));
            Assert.IsFalse(lockstep.RecordRemoteHash(60, 0xBBBB0002u));
            Assert.IsTrue(lockstep.HasDesync);
            Assert.AreEqual(SportsArenaLockstepState.Desync, lockstep.State);
            Assert.AreEqual(60, lockstep.DesyncTick);
            Assert.AreEqual(1, lockstep.MismatchCount);
        }

        [Test]
        public void Lockstep_VerifiesMatchingHashArrivingBeforeLocalStep()
        {
            SportsArenaLockstep lockstep = new SportsArenaLockstep();
            lockstep.ResetForStart(1, 2, 0, 1, 1, 2);

            Assert.IsTrue(lockstep.RecordRemoteHash(120, 0x12345678u));
            Assert.IsTrue(lockstep.RecordLocalHash(120, 0x12345678u));
            Assert.IsFalse(lockstep.HasDesync);
            Assert.AreEqual(1, lockstep.VerifiedHashCount);
            Assert.AreEqual(120, lockstep.LastVerifiedTick);
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
            body.IsActive = true;
            body.Position = simulation.Goals[0].Area.Center;
            body.LinearVelocity = FPVector3.Zero;
            body.AngularVelocity = FPVector3.Zero;
            body.Wake();
            simulation.Ball.Holder = -1;
            simulation.Ball.Active = true;
            simulation.Ball.Position = body.Position;
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
