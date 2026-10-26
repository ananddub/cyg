using CookYourGame.Sim;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.Controls;

namespace CookYourGame.SportsArena
{
    public sealed partial class SportsArenaBootstrap
    {
        private Camera FindOrCreateCamera()
        {
            Camera camera = Camera.main;
            if (camera == null)
            {
                GameObject go = new GameObject("Arena Camera");
                camera = go.AddComponent<Camera>();
                go.AddComponent<AudioListener>();
                go.tag = "MainCamera";
            }
            camera.transform.position = new Vector3(0f, 16f, -18f);
            camera.transform.LookAt(new Vector3(0f, 2f, 0f));
            camera.fieldOfView = 60f;
            camera.nearClipPlane = 0.1f;
            camera.farClipPlane = 200f;
            if (FindLight() == null)
            {
                GameObject light = new GameObject("Arena Sun");
                Light component = light.AddComponent<Light>();
                component.type = LightType.Directional;
                component.intensity = 1.2f;
                component.transform.rotation = Quaternion.Euler(48f, -35f, 0f);
            }
            return camera;
        }

        private static Light FindLight()
        {
            Light[] lights = FindObjectsByType<Light>(FindObjectsSortMode.None);
            return lights.Length > 0 ? lights[0] : null;
        }

        private static SportsArenaInput ReadInput(int index, int tick)
        {
            Keyboard keyboard = Keyboard.current;
            if (keyboard == null)
            {
                return default;
            }

            int x;
            int z;
            byte buttons = 0;
            if (index == 0)
            {
                x = (keyboard.dKey.isPressed ? 1 : 0) - (keyboard.aKey.isPressed ? 1 : 0);
                z = (keyboard.wKey.isPressed ? 1 : 0) - (keyboard.sKey.isPressed ? 1 : 0);
                buttons |= Press(keyboard.spaceKey, CYGPlayerInput.ButtonJump);
                buttons |= Press(keyboard.jKey, CYGPlayerInput.ButtonAction1);
                buttons |= Press(keyboard.kKey, CYGPlayerInput.ButtonAction2);
                buttons |= Press(keyboard.eKey, CYGPlayerInput.ButtonAction3);
                buttons |= Press(keyboard.lKey, CYGPlayerInput.ButtonAction4);
            }
            else
            {
                x = (keyboard.rightArrowKey.isPressed ? 1 : 0) - (keyboard.leftArrowKey.isPressed ? 1 : 0);
                z = (keyboard.upArrowKey.isPressed ? 1 : 0) - (keyboard.downArrowKey.isPressed ? 1 : 0);
                buttons |= Press(keyboard.rightCtrlKey, CYGPlayerInput.ButtonJump);
                buttons |= Press(keyboard.numpad1Key, CYGPlayerInput.ButtonAction1);
                buttons |= Press(keyboard.numpad2Key, CYGPlayerInput.ButtonAction2);
                buttons |= Press(keyboard.numpad3Key, CYGPlayerInput.ButtonAction3);
                buttons |= Press(keyboard.numpad0Key, CYGPlayerInput.ButtonAction4);
            }

            short moveX = (short)(x * CYGPlayerInput.MaxMove);
            short moveY = (short)(z * CYGPlayerInput.MaxMove);
            return new SportsArenaInput
            {
                Value = new CYGPlayerInput(tick, buttons, moveX, moveY),
            };
        }

        private static byte Press(KeyControl key, byte flag)
        {
            return key != null && key.isPressed ? flag : (byte)0;
        }
    }
}
