using System;
using System.Runtime.InteropServices;

public static class Program
{
    private static float posX = 100.0f;
    private static float posY = 100.0f;
    private static int score = 0;

    public static void Main()
    {
        // Standalone main entrypoint
    }

    [UnmanagedCallersOnly(EntryPoint = "init")]
    public static void Init()
    {
        posX = 100.0f;
        posY = 100.0f;
        score = 0;
    }

    [UnmanagedCallersOnly(EntryPoint = "update")]
    public static void Update(float dt, float speedX, float speedY)
    {
        posX += speedX * dt;
        posY += speedY * dt;
        score += 1;
    }

    [UnmanagedCallersOnly(EntryPoint = "getScore")]
    public static int GetScore() => score;

    [UnmanagedCallersOnly(EntryPoint = "getPositionX")]
    public static float GetPositionX() => posX;

    [UnmanagedCallersOnly(EntryPoint = "getPositionY")]
    public static float GetPositionY() => posY;

    [UnmanagedCallersOnly(EntryPoint = "calculateDamage")]
    public static int CalculateDamage(int baseDamage, float multiplier)
    {
        return (int)(baseDamage * multiplier);
    }
}
