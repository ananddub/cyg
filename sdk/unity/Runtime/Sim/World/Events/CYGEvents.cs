using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// reference-style decoupled Event Broker for View / MonoBehaviour layers.
    /// Systems in pure simulation publish via world.Events.Publish(new MyEvent(...)).
    /// MonoBehaviours in Unity view subscribe cleanly via CYGEvents.Subscribe&lt;MyEvent&gt;(handler).
    /// </summary>
    public static class CYGEvents
    {
        private static readonly Dictionary<Type, List<Delegate>> _subscribers =
            new Dictionary<Type, List<Delegate>>();
        private static readonly Dictionary<object, List<IDisposable>> _owners =
            new Dictionary<object, List<IDisposable>>();

        public static void Subscribe<T>(Action<T> handler) where T : CYGEvent
        {
            if (handler == null) return;
            AddSubscriber(handler);
        }

        /// <summary>
        /// Subscribes a handler owned by a view, component or arbitrary
        /// object. <see cref="UnsubscribeAll"/> releases every subscription
        /// registered for that owner, which is how a despawned entity view
        /// avoids leaking handlers into the next entity that reuses its slot.
        /// </summary>
        public static IDisposable Subscribe<T>(object owner, Action<T> handler) where T : CYGEvent
        {
            if (handler == null)
            {
                throw new ArgumentNullException(nameof(handler));
            }

            AddSubscriber(handler);
            CYGEventSubscription<T> subscription = new CYGEventSubscription<T>(handler);

            if (owner != null)
            {
                if (!_owners.TryGetValue(owner, out List<IDisposable> list))
                {
                    list = new List<IDisposable>(4);
                    _owners[owner] = list;
                }
                list.Add(subscription);
            }

            return subscription;
        }

        private static void AddSubscriber<T>(Action<T> handler) where T : CYGEvent
        {
            Type t = typeof(T);
            if (!_subscribers.TryGetValue(t, out var list))
            {
                list = new List<Delegate>();
                _subscribers[t] = list;
            }
            if (!list.Contains(handler))
            {
                list.Add(handler);
            }
        }

        public static void Unsubscribe<T>(Action<T> handler) where T : CYGEvent
        {
            if (handler == null) return;
            if (_subscribers.TryGetValue(typeof(T), out var list))
            {
                list.Remove(handler);
            }
        }

        /// <summary>Disposes every subscription registered under <paramref name="owner"/>.</summary>
        public static void UnsubscribeAll(object owner)
        {
            if (owner == null) return;
            if (!_owners.TryGetValue(owner, out List<IDisposable> list))
            {
                return;
            }

            _owners.Remove(owner);
            for (int i = 0; i < list.Count; i++)
            {
                list[i].Dispose();
            }
        }

        public static void Dispatch(CYGEvent evt)
        {
            if (evt == null) return;
            if (_subscribers.TryGetValue(evt.GetType(), out var list))
            {
                int count = list.Count;
                for (int i = 0; i < count; i++)
                {
                    if (i < list.Count)
                    {
                        list[i].DynamicInvoke(evt);
                    }
                }
            }
        }

        public static void Clear()
        {
            _subscribers.Clear();
            _owners.Clear();
        }
    }

    /// <summary>
    /// Disposable handle for one typed event subscription. Disposing removes
    /// only this handler; owner-scoped handles are also released en masse by
    /// <see cref="CYGEvents.UnsubscribeAll"/> when a view despawns.
    /// </summary>
    public sealed class CYGEventSubscription<T> : IDisposable where T : CYGEvent
    {
        private Action<T> _handler;

        internal CYGEventSubscription(Action<T> handler)
        {
            _handler = handler;
        }

        public bool IsDisposed => _handler == null;

        public void Dispose()
        {
            Action<T> handler = _handler;
            if (handler == null)
            {
                return;
            }
            _handler = null;
            CYGEvents.Unsubscribe(handler);
        }
    }
}
