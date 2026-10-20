using System;

namespace CookYourGame.Dsl.Tests
{
    internal sealed class CygTestException : Exception
    {
        public CygTestException(string message) : base(message)
        {
        }
    }

    internal static class TestAssert
    {
        public static int Count { get; private set; }

        public static void Require(bool condition, string message)
        {
            Count++;
            if (!condition)
            {
                throw new CygTestException(message);
            }
        }

        public static void Equal<T>(T expected, T actual, string what)
        {
            Count++;
            if (!Equals(expected, actual))
            {
                throw new CygTestException($"{what}: expected {Format(expected)}, got {Format(actual)}");
            }
        }

        public static void Contains(string text, string expected, string what)
        {
            Count++;
            if (text == null || text.IndexOf(expected, StringComparison.Ordinal) < 0)
            {
                throw new CygTestException($"{what}: generated output does not contain '{expected}'");
            }
        }

        private static string Format<T>(T value)
        {
            return value == null ? "<null>" : value.ToString();
        }
    }
}
