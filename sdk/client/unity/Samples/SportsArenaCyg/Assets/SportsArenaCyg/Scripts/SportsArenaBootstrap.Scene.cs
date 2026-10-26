using System;
using System.Collections.Generic;
using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaBootstrap
    {
        private static SportsArenaGoal[] FindGoals(GameObject root, Bounds bounds, FPAABB arena)
        {
            List<Vector3> centers = new List<Vector3>();
            Transform[] transforms = root.GetComponentsInChildren<Transform>(true);
            for (int i = 0; i < transforms.Length; i++)
            {
                if (transforms[i].name.IndexOf("Goal Collider", StringComparison.OrdinalIgnoreCase) < 0)
                {
                    continue;
                }
                Vector3 point = transforms[i].position;
                bool duplicate = false;
                for (int j = 0; j < centers.Count; j++)
                {
                    if (Vector3.SqrMagnitude(centers[j] - point) < 1f)
                    {
                        duplicate = true;
                        break;
                    }
                }
                if (!duplicate)
                {
                    centers.Add(point);
                }
            }

            if (centers.Count < 2)
            {
                centers.Clear();
                float distance = Mathf.Max(6f, bounds.extents.x * 0.75f);
                centers.Add(new Vector3(-distance, 0f, 0f));
                centers.Add(new Vector3(distance, 0f, 0f));
            }

            centers.Sort((a, b) => a.x.CompareTo(b.x));
            float depth = 2f;
            float width = Mathf.Max(3f, Mathf.Min(bounds.extents.z * 0.8f, 7f));
            SportsArenaGoal[] goals = new SportsArenaGoal[2];
            for (int i = 0; i < 2; i++)
            {
                Vector3 c = centers[i];
                FPAABB area = new FPAABB(
                    SportsArenaConfig.ToFixed(new Vector3(c.x - depth, 0f, c.z - width)),
                    SportsArenaConfig.ToFixed(new Vector3(c.x + depth, 5f, c.z + width)));
                goals[i] = new SportsArenaGoal
                {
                    Area = area,
                    DefendingTeam = i,
                };
            }
            return goals;
        }

        private static FPVector3[] FindSpawners(Transform[] transforms, SportsArenaGoal[] goals, FPAABB arena)
        {
            List<Transform> left = new List<Transform>();
            List<Transform> right = new List<Transform>();
            for (int i = 0; i < transforms.Length; i++)
            {
                Transform transform = transforms[i];
                if (!transform.name.StartsWith("Player Spawner", StringComparison.Ordinal)
                    || transform.name.IndexOf("Spawners", StringComparison.Ordinal) >= 0)
                {
                    continue;
                }
                if (transform.position.x < 0f)
                {
                    left.Add(transform);
                }
                else
                {
                    right.Add(transform);
                }
            }
            left.Sort((a, b) => a.position.z.CompareTo(b.position.z));
            right.Sort((a, b) => a.position.z.CompareTo(b.position.z));

            FPVector3[] fallback = BuildSpawns(goals, arena);
            FPVector3[] spawns = new FPVector3[SportsArenaConfig.PlayerCount];
            for (int slot = 0; slot < SportsArenaConfig.PlayerCount / 2; slot++)
            {
                int blue = slot * 2;
                int red = blue + 1;
                spawns[blue] = slot < left.Count
                    ? SportsArenaConfig.ToFixed(left[slot].position + Vector3.up * 0.2f)
                    : fallback[blue];
                spawns[red] = slot < right.Count
                    ? SportsArenaConfig.ToFixed(right[slot].position + Vector3.up * 0.2f)
                    : fallback[red];
            }
            return spawns;
        }

        private static FPVector3 FindBallSpawn(Transform[] transforms, FPAABB arena)
        {
            for (int i = 0; i < transforms.Length; i++)
            {
                if (!string.Equals(transforms[i].name, "Ball Spawner", StringComparison.Ordinal))
                {
                    continue;
                }
                Vector3 position = transforms[i].position;
                position.y = Mathf.Max(position.y, 1.5f);
                return SportsArenaConfig.ToFixed(position);
            }
            return new FPVector3(arena.Center.x, FP.FromFraction(3, 2), arena.Center.z);
        }

        private static FPVector3[] BuildSpawns(SportsArenaGoal[] goals, FPAABB arena)
        {
            FPVector3[] spawns = new FPVector3[SportsArenaConfig.PlayerCount];
            FPVector3 left = goals[0].Area.Center;
            FPVector3 right = goals[1].Area.Center;
            FPVector3 axis = (right - left).Normalized;
            FP distance = FPVector3.Distance(left, right) * FP.FromFraction(1, 5);
            FPVector3 center = arena.Center;
            center.y = FP.FromFraction(1, 5);
            FPVector3 perpendicular = new FPVector3(-axis.z, FP.Zero, axis.x);
            for (int slot = 0; slot < SportsArenaConfig.PlayerCount / 2; slot++)
            {
                FP offset = FP.FromInt(slot - 1) * FP.FromInt(4);
                spawns[slot * 2] = center - axis * distance + perpendicular * offset;
                spawns[slot * 2 + 1] = center + axis * distance + perpendicular * offset;
            }
            return spawns;
        }
    }
}
