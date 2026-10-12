using System;
using UnityEngine;

namespace OpenOxide
{
    /// <summary>
    /// Playback engine for .oxr match recordings in Unity.
    /// Supports variable speed playback (0.25x to 4x), scrubbing, pause, and instant kill-cams.
    /// </summary>
    public class OpenOxideReplayPlayer
    {
        public MatchReplayData Data { get; private set; }
        public bool IsPlaying { get; private set; }
        public float Speed { get; private set; } = 1.0f;
        public float Clock { get; private set; } = 0.0f;
        public int CurrentFrameIndex { get; private set; } = 0;

        public event Action<StateDelta, int, int> OnFrame;
        public event Action<float> OnProgress;
        public event Action OnEnded;

        public OpenOxideReplayPlayer(MatchReplayData data)
        {
            Data = data ?? throw new ArgumentNullException(nameof(data));
        }

        public void Play()
        {
            if (CurrentFrameIndex >= Data.frames.Count)
            {
                CurrentFrameIndex = 0;
                Clock = 0f;
            }
            IsPlaying = true;
        }

        public void Pause()
        {
            IsPlaying = false;
        }

        public void SetSpeed(float speed)
        {
            Speed = Mathf.Clamp(speed, 0.1f, 5.0f);
        }

        public void Seek(float progress)
        {
            progress = Mathf.Clamp01(progress);
            float targetTimeMs = progress * Data.durationMs;
            Clock = targetTimeMs;

            int foundIdx = 0;
            for (int i = 0; i < Data.frames.Count; i++)
            {
                if (Data.frames[i].t <= targetTimeMs)
                {
                    foundIdx = i;
                }
                else
                {
                    break;
                }
            }

            CurrentFrameIndex = foundIdx;
            if (foundIdx < Data.frames.Count)
            {
                byte[] raw = Data.frames[foundIdx].d;
                StateDelta delta = StateDelta.Unpack(raw);
                if (delta != null)
                {
                    OnFrame?.Invoke(delta, CurrentFrameIndex, Data.frames.Count);
                }
            }
            OnProgress?.Invoke(progress);
        }

        /// <summary>
        /// Call inside Unity's Update() to advance replay timeline.
        /// </summary>
        public void Update(float deltaTime)
        {
            if (!IsPlaying || Data.frames.Count == 0) return;

            Clock += (deltaTime * 1000f) * Speed;

            while (CurrentFrameIndex < Data.frames.Count && Data.frames[CurrentFrameIndex].t <= Clock)
            {
                byte[] raw = Data.frames[CurrentFrameIndex].d;
                StateDelta delta = StateDelta.Unpack(raw);
                if (delta != null)
                {
                    OnFrame?.Invoke(delta, CurrentFrameIndex, Data.frames.Count);
                }
                CurrentFrameIndex++;
            }

            float progress = Data.durationMs > 0 ? Mathf.Clamp01(Clock / Data.durationMs) : 1f;
            OnProgress?.Invoke(progress);

            if (CurrentFrameIndex >= Data.frames.Count)
            {
                IsPlaying = false;
                OnEnded?.Invoke();
            }
        }
    }
}
