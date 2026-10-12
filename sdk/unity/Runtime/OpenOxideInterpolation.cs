using UnityEngine;

namespace OpenOxide
{
    /// <summary>
    /// Smooth interpolation helper for network entity transforms in Unity.
    /// Eliminates jitter and network snap between 60Hz server tick packets.
    /// </summary>
    public static class OpenOxideInterpolation
    {
        public static Vector3 InterpolatePosition(Vector3 current, Vector3 target, float interpolationSpeed, float deltaTime)
        {
            return Vector3.Lerp(current, target, Mathf.Clamp01(interpolationSpeed * deltaTime));
        }

        public static Quaternion InterpolateRotation(Quaternion current, Quaternion target, float interpolationSpeed, float deltaTime)
        {
            return Quaternion.Slerp(current, target, Mathf.Clamp01(interpolationSpeed * deltaTime));
        }

        public static float LerpAngle(float current, float target, float factor)
        {
            float diff = Mathf.DeltaAngle(current, target);
            return current + diff * Mathf.Clamp01(factor);
        }
    }
}
