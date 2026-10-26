class_name CYGKeyCode
extends RefCounted
## Browser `KeyCode` values used by the JS SDK, plus the `KeyCodeHelper`
## predicates. Sending these instead of Godot's `KEY_*` keeps input flags
## identical across web, Unity and native clients.

const NONE := 0

const MOUSE_LEFT := 1
const MOUSE_MIDDLE := 2
const MOUSE_RIGHT := 3
const MOUSE_BACK := 4
const MOUSE_FORWARD := 5

const BACKSPACE := 8
const TAB := 9
const ENTER := 13
const SHIFT := 16
const CONTROL := 17
const ALT := 18
const PAUSE := 19
const CAPS_LOCK := 20
const ESCAPE := 27
const SPACE := 32

const PAGE_UP := 33
const PAGE_DOWN := 34
const END := 35
const HOME := 36
const ARROW_LEFT := 37
const ARROW_UP := 38
const ARROW_RIGHT := 39
const ARROW_DOWN := 40
const LEFT := 37
const UP := 38
const RIGHT := 39
const DOWN := 40
const PRINT_SCREEN := 44
const INSERT := 45
const DELETE := 46

const DIGIT_0 := 48
const DIGIT_1 := 49
const DIGIT_2 := 50
const DIGIT_3 := 51
const DIGIT_4 := 52
const DIGIT_5 := 53
const DIGIT_6 := 54
const DIGIT_7 := 55
const DIGIT_8 := 56
const DIGIT_9 := 57

const KEY_A := 65
const KEY_B := 66
const KEY_C := 67
const KEY_D := 68
const KEY_E := 69
const KEY_F := 70
const KEY_G := 71
const KEY_H := 72
const KEY_I := 73
const KEY_J := 74
const KEY_K := 75
const KEY_L := 76
const KEY_M := 77
const KEY_N := 78
const KEY_O := 79
const KEY_P := 80
const KEY_Q := 81
const KEY_R := 82
const KEY_S := 83
const KEY_T := 84
const KEY_U := 85
const KEY_V := 86
const KEY_W := 87
const KEY_X := 88
const KEY_Y := 89
const KEY_Z := 90

const META_LEFT := 91
const META_RIGHT := 92
const CONTEXT_MENU := 93

const NUMPAD_0 := 96
const NUMPAD_1 := 97
const NUMPAD_2 := 98
const NUMPAD_3 := 99
const NUMPAD_4 := 100
const NUMPAD_5 := 101
const NUMPAD_6 := 102
const NUMPAD_7 := 103
const NUMPAD_8 := 104
const NUMPAD_9 := 105
const NUMPAD_MULTIPLY := 106
const NUMPAD_ADD := 107
const NUMPAD_SEPARATOR := 108
const NUMPAD_SUBTRACT := 109
const NUMPAD_DECIMAL := 110
const NUMPAD_DIVIDE := 111

const F1 := 112
const F2 := 113
const F3 := 114
const F4 := 115
const F5 := 116
const F6 := 117
const F7 := 118
const F8 := 119
const F9 := 120
const F10 := 121
const F11 := 122
const F12 := 123
const F13 := 124
const F14 := 125
const F15 := 126
const F16 := 127
const F17 := 128
const F18 := 129
const F19 := 130
const F20 := 131
const F21 := 132
const F22 := 133
const F23 := 134
const F24 := 135

const NUM_LOCK := 144
const SCROLL_LOCK := 145

const SHIFT_LEFT := 160
const SHIFT_RIGHT := 161
const CONTROL_LEFT := 162
const CONTROL_RIGHT := 163
const ALT_LEFT := 164
const ALT_RIGHT := 165

const SEMICOLON := 186
const EQUAL := 187
const COMMA := 188
const MINUS := 189
const PERIOD := 190
const SLASH := 191
const BACKQUOTE := 192
const BRACKET_LEFT := 219
const BACKSLASH := 220
const BRACKET_RIGHT := 221
const QUOTE := 222


static func is_letter(key: int) -> bool:
	return key >= 65 and key <= 90


static func is_digit(key: int) -> bool:
	return (key >= 48 and key <= 57) or (key >= 96 and key <= 105)


static func is_arrow(key: int) -> bool:
	return key >= 37 and key <= 40


static func is_function_key(key: int) -> bool:
	return key >= 112 and key <= 123


static func is_modifier(key: int) -> bool:
	return key == 16 or key == 17 or key == 18 or (key >= 160 and key <= 165)


static func is_mouse_button(key: int) -> bool:
	return key >= 1 and key <= 5
