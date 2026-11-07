/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeHost_h
#define widget_bande_BandeHost_h

#include <stdint.h>

struct wl_display;
struct wl_surface;

namespace mozilla::widget {

enum class BandeEventKind : uint32_t {
  Wake = 1,
  PointerEnter = 2,
  PointerLeave = 3,
  PointerMove = 4,
  PointerDown = 5,
  PointerUp = 6,
  Scroll = 7,
  Key = 8,
  Focus = 9,
  Close = 10,
  ToplevelState = 11,
  Resized = 12,
  Appearance = 13,
};

enum BandeModifier : uint32_t {
  BandeModifierShift = 1,
  BandeModifierControl = 2,
  BandeModifierAlt = 4,
  BandeModifierLogo = 8,
};

enum BandeToplevelState : uint32_t {
  BandeStateMaximized = 1,
  BandeStateFullscreen = 2,
  BandeStateActivated = 4,
  BandeStateTiled = 8,
  BandeStateSuspended = 16,
};

struct BandeEvent {
  BandeEventKind kind;
  uint64_t window;
  double x;
  double y;
  double dx;
  double dy;
  uint32_t a;
  uint32_t b;
  uint32_t c;
  uint32_t d;
  uint32_t modifiers;
  const char* text;
};

class BandeHost final {
 public:
  static bool Start();
  static bool IsActive();
  static wl_display* Display();
  static int WakeWriteFd();

  static uint64_t Open(const char* aTitle, const char* aAppId, uint32_t aWidth,
                       uint32_t aHeight);
  static void Close(uint64_t aWindow);
  static wl_surface* Surface(uint64_t aWindow);
  static void SetTitle(uint64_t aWindow, const char* aTitle);
  static void SetSize(uint64_t aWindow, uint32_t aWidth, uint32_t aHeight);
  static void Size(uint64_t aWindow, uint32_t* aWidth, uint32_t* aHeight);
  static void SetLimits(uint64_t aWindow, uint32_t aMinWidth,
                        uint32_t aMinHeight, uint32_t aMaxWidth,
                        uint32_t aMaxHeight);
  static double Scale(uint64_t aWindow);
  static void StartMove(uint64_t aWindow);
  static void StartResize(uint64_t aWindow, uint32_t aEdge);
  static void Minimize(uint64_t aWindow);
  static void ToggleZoom(uint64_t aWindow);
  static void SetFullScreen(uint64_t aWindow, bool aFullScreen);
  static void SetCursor(uint64_t aWindow, uint32_t aKind);
  static void Flush();
  static bool Step(bool aBlock);
  static bool NextEvent(BandeEvent* aEvent);
};

}  // namespace mozilla::widget

#endif
