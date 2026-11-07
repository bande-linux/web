/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandeKeys.h"

#include <xkbcommon/xkbcommon.h>
#include <xkbcommon/xkbcommon-keysyms.h>

#include "mozilla/TextEvents.h"
#include "mozilla/dom/KeyboardEventBinding.h"

using namespace mozilla::dom;

namespace mozilla::widget {

static const uint32_t kEvdevToXkb = 8;

CodeNameIndex CodeNameFromEvdev(uint32_t aEvdevCode) {
  const uint32_t native = aEvdevCode + kEvdevToXkb;
#define NS_NATIVE_KEY_TO_DOM_CODE_NAME_INDEX(aNativeKey, aCodeNameIndex) \
  if (native == (aNativeKey)) {                                          \
    return aCodeNameIndex;                                               \
  }
#include "NativeKeyToDOMCodeName.inc"
#undef NS_NATIVE_KEY_TO_DOM_CODE_NAME_INDEX
  return CODE_NAME_INDEX_UNKNOWN;
}

KeyNameIndex KeyNameFromKeysym(uint32_t aKeysym) {
  switch (aKeysym) {
    case XKB_KEY_BackSpace: return KEY_NAME_INDEX_Backspace;
    case XKB_KEY_Tab:
    case XKB_KEY_ISO_Left_Tab:
    case XKB_KEY_KP_Tab: return KEY_NAME_INDEX_Tab;
    case XKB_KEY_Return:
    case XKB_KEY_KP_Enter: return KEY_NAME_INDEX_Enter;
    case XKB_KEY_Escape: return KEY_NAME_INDEX_Escape;
    case XKB_KEY_Delete:
    case XKB_KEY_KP_Delete: return KEY_NAME_INDEX_Delete;
    case XKB_KEY_Insert:
    case XKB_KEY_KP_Insert: return KEY_NAME_INDEX_Insert;
    case XKB_KEY_Home:
    case XKB_KEY_KP_Home: return KEY_NAME_INDEX_Home;
    case XKB_KEY_End:
    case XKB_KEY_KP_End: return KEY_NAME_INDEX_End;
    case XKB_KEY_Page_Up:
    case XKB_KEY_KP_Page_Up: return KEY_NAME_INDEX_PageUp;
    case XKB_KEY_Page_Down:
    case XKB_KEY_KP_Page_Down: return KEY_NAME_INDEX_PageDown;
    case XKB_KEY_Left:
    case XKB_KEY_KP_Left: return KEY_NAME_INDEX_ArrowLeft;
    case XKB_KEY_Right:
    case XKB_KEY_KP_Right: return KEY_NAME_INDEX_ArrowRight;
    case XKB_KEY_Up:
    case XKB_KEY_KP_Up: return KEY_NAME_INDEX_ArrowUp;
    case XKB_KEY_Down:
    case XKB_KEY_KP_Down: return KEY_NAME_INDEX_ArrowDown;
    case XKB_KEY_Shift_L:
    case XKB_KEY_Shift_R: return KEY_NAME_INDEX_Shift;
    case XKB_KEY_Control_L:
    case XKB_KEY_Control_R: return KEY_NAME_INDEX_Control;
    case XKB_KEY_Alt_L:
    case XKB_KEY_Alt_R: return KEY_NAME_INDEX_Alt;
    case XKB_KEY_ISO_Level3_Shift: return KEY_NAME_INDEX_AltGraph;
    case XKB_KEY_Super_L:
    case XKB_KEY_Super_R:
    case XKB_KEY_Meta_L:
    case XKB_KEY_Meta_R: return KEY_NAME_INDEX_Meta;
    case XKB_KEY_Caps_Lock: return KEY_NAME_INDEX_CapsLock;
    case XKB_KEY_Num_Lock: return KEY_NAME_INDEX_NumLock;
    case XKB_KEY_Scroll_Lock: return KEY_NAME_INDEX_ScrollLock;
    case XKB_KEY_Menu: return KEY_NAME_INDEX_ContextMenu;
    case XKB_KEY_Pause: return KEY_NAME_INDEX_Pause;
    case XKB_KEY_Print: return KEY_NAME_INDEX_PrintScreen;
    case XKB_KEY_F1: return KEY_NAME_INDEX_F1;
    case XKB_KEY_F2: return KEY_NAME_INDEX_F2;
    case XKB_KEY_F3: return KEY_NAME_INDEX_F3;
    case XKB_KEY_F4: return KEY_NAME_INDEX_F4;
    case XKB_KEY_F5: return KEY_NAME_INDEX_F5;
    case XKB_KEY_F6: return KEY_NAME_INDEX_F6;
    case XKB_KEY_F7: return KEY_NAME_INDEX_F7;
    case XKB_KEY_F8: return KEY_NAME_INDEX_F8;
    case XKB_KEY_F9: return KEY_NAME_INDEX_F9;
    case XKB_KEY_F10: return KEY_NAME_INDEX_F10;
    case XKB_KEY_F11: return KEY_NAME_INDEX_F11;
    case XKB_KEY_F12: return KEY_NAME_INDEX_F12;
    default: return KEY_NAME_INDEX_USE_STRING;
  }
}

uint32_t CharacterFromKeysym(uint32_t aKeysym) {
  return xkb_keysym_to_utf32(aKeysym);
}

uint32_t LegacyKeyCode(uint32_t aKeysym, uint32_t aBaseKeysym) {
  switch (aKeysym) {
    case XKB_KEY_BackSpace: return KeyboardEvent_Binding::DOM_VK_BACK_SPACE;
    case XKB_KEY_Tab:
    case XKB_KEY_ISO_Left_Tab: return KeyboardEvent_Binding::DOM_VK_TAB;
    case XKB_KEY_Return:
    case XKB_KEY_KP_Enter: return KeyboardEvent_Binding::DOM_VK_RETURN;
    case XKB_KEY_Escape: return KeyboardEvent_Binding::DOM_VK_ESCAPE;
    case XKB_KEY_Delete:
    case XKB_KEY_KP_Delete: return KeyboardEvent_Binding::DOM_VK_DELETE;
    case XKB_KEY_Insert:
    case XKB_KEY_KP_Insert: return KeyboardEvent_Binding::DOM_VK_INSERT;
    case XKB_KEY_Home:
    case XKB_KEY_KP_Home: return KeyboardEvent_Binding::DOM_VK_HOME;
    case XKB_KEY_End:
    case XKB_KEY_KP_End: return KeyboardEvent_Binding::DOM_VK_END;
    case XKB_KEY_Page_Up:
    case XKB_KEY_KP_Page_Up: return KeyboardEvent_Binding::DOM_VK_PAGE_UP;
    case XKB_KEY_Page_Down:
    case XKB_KEY_KP_Page_Down: return KeyboardEvent_Binding::DOM_VK_PAGE_DOWN;
    case XKB_KEY_Left:
    case XKB_KEY_KP_Left: return KeyboardEvent_Binding::DOM_VK_LEFT;
    case XKB_KEY_Right:
    case XKB_KEY_KP_Right: return KeyboardEvent_Binding::DOM_VK_RIGHT;
    case XKB_KEY_Up:
    case XKB_KEY_KP_Up: return KeyboardEvent_Binding::DOM_VK_UP;
    case XKB_KEY_Down:
    case XKB_KEY_KP_Down: return KeyboardEvent_Binding::DOM_VK_DOWN;
    case XKB_KEY_Shift_L:
    case XKB_KEY_Shift_R: return KeyboardEvent_Binding::DOM_VK_SHIFT;
    case XKB_KEY_Control_L:
    case XKB_KEY_Control_R: return KeyboardEvent_Binding::DOM_VK_CONTROL;
    case XKB_KEY_Alt_L:
    case XKB_KEY_Alt_R: return KeyboardEvent_Binding::DOM_VK_ALT;
    case XKB_KEY_ISO_Level3_Shift: return KeyboardEvent_Binding::DOM_VK_ALTGR;
    case XKB_KEY_Super_L:
    case XKB_KEY_Super_R:
    case XKB_KEY_Meta_L:
    case XKB_KEY_Meta_R: return KeyboardEvent_Binding::DOM_VK_WIN;
    case XKB_KEY_Caps_Lock: return KeyboardEvent_Binding::DOM_VK_CAPS_LOCK;
    case XKB_KEY_Num_Lock: return KeyboardEvent_Binding::DOM_VK_NUM_LOCK;
    case XKB_KEY_Scroll_Lock: return KeyboardEvent_Binding::DOM_VK_SCROLL_LOCK;
    case XKB_KEY_Menu: return KeyboardEvent_Binding::DOM_VK_CONTEXT_MENU;
    case XKB_KEY_Pause: return KeyboardEvent_Binding::DOM_VK_PAUSE;
    case XKB_KEY_Print: return KeyboardEvent_Binding::DOM_VK_PRINTSCREEN;
    default: break;
  }
  if (aKeysym >= XKB_KEY_F1 && aKeysym <= XKB_KEY_F24) {
    return KeyboardEvent_Binding::DOM_VK_F1 + (aKeysym - XKB_KEY_F1);
  }
  uint32_t character = xkb_keysym_to_utf32(aBaseKeysym);
  if (character >= 'a' && character <= 'z') {
    return character - 'a' + 'A';
  }
  if ((character >= 'A' && character <= 'Z') ||
      (character >= '0' && character <= '9') || character == ' ') {
    return character;
  }
  switch (character) {
    case ';': return KeyboardEvent_Binding::DOM_VK_SEMICOLON;
    case '=': return KeyboardEvent_Binding::DOM_VK_EQUALS;
    case ',': return KeyboardEvent_Binding::DOM_VK_COMMA;
    case '-': return KeyboardEvent_Binding::DOM_VK_HYPHEN_MINUS;
    case '.': return KeyboardEvent_Binding::DOM_VK_PERIOD;
    case '/': return KeyboardEvent_Binding::DOM_VK_SLASH;
    case '`': return KeyboardEvent_Binding::DOM_VK_BACK_QUOTE;
    case '[': return KeyboardEvent_Binding::DOM_VK_OPEN_BRACKET;
    case '\\': return KeyboardEvent_Binding::DOM_VK_BACK_SLASH;
    case ']': return KeyboardEvent_Binding::DOM_VK_CLOSE_BRACKET;
    case '\'': return KeyboardEvent_Binding::DOM_VK_QUOTE;
    default: return 0;
  }
}

}  // namespace mozilla::widget
