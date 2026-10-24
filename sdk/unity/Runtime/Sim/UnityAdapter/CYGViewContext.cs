using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// ScriptableObject that carries scene references into the presentation
    /// layer. Entity views and view components read it through
    /// <see cref="CYGEntityView.ViewContext"/> instead of calling Camera.main
    /// or searching the scene, which keeps views decoupled from scene layout
    /// and reusable across scenes.
    /// </summary>
    [CreateAssetMenu(
        fileName = "CYGViewContext",
        menuName = "CookYourGame/Sim/View Context",
        order = 10)]
    public class CYGViewContext : ScriptableObject
    {
        [SerializeField] private Camera mainCamera;
        [SerializeField] private Transform viewRoot;
        [SerializeField] private Canvas uiCanvas;

        [Tooltip("Additional scene references (spawn markers, audio sources, UI roots) resolved by type.")]
        [SerializeField] private List<UnityEngine.Object> references = new List<UnityEngine.Object>();

        public Camera MainCamera
        {
            get
            {
                if (mainCamera == null)
                {
                    mainCamera = Camera.main;
                }
                return mainCamera;
            }
        }

        public Transform ViewRoot => viewRoot;
        public Canvas UICanvas => uiCanvas;
        public int ReferenceCount => references.Count;

        /// <summary>Returns the first registered reference assignable to T.</summary>
        public T Get<T>() where T : UnityEngine.Object
        {
            for (int i = 0; i < references.Count; i++)
            {
                if (references[i] is T typed)
                {
                    return typed;
                }
            }
            return null;
        }

        /// <summary>Non-throwing lookup used by generic view components.</summary>
        public bool TryGet<T>(out T value) where T : UnityEngine.Object
        {
            value = Get<T>();
            return value != null;
        }

        public void AddReference(UnityEngine.Object reference)
        {
            if (reference != null && !references.Contains(reference))
            {
                references.Add(reference);
            }
        }

        public bool RemoveReference(UnityEngine.Object reference)
        {
            return reference != null && references.Remove(reference);
        }

        public void SetMainCamera(Camera camera)
        {
            mainCamera = camera;
        }

        public void SetViewRoot(Transform root)
        {
            viewRoot = root;
        }

        public void SetUICanvas(Canvas canvas)
        {
            uiCanvas = canvas;
        }
    }
}
