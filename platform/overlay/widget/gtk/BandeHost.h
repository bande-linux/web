/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_BandeHost_h
#define widget_bande_BandeHost_h

#include <stddef.h>
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
  FdReady = 14,
  ClipboardData = 15,
  ClipboardChanged = 16,
  PopupMoved = 17,
  PopupDismissed = 18,
  DragEnter = 19,
  DragMove = 20,
  DragLeave = 21,
  DragDrop = 22,
  DragFinished = 23,
  ImePreedit = 24,
  ImeCommit = 25,
  FilesChosen = 26,
  ScreenCast = 27,
  ScreenCastClosed = 28,
  Camera = 29,
  Remote = 30,
  MediaKey = 31,
  Notification = 32,
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
  const uint8_t* data;
  size_t len;
};

struct BandePalette {
  uint32_t chassis;
  uint32_t frame;
  uint32_t sheet;
  uint32_t overlay;
  uint32_t panel;
  uint32_t well;
  uint32_t field;
  uint32_t raised;
  uint32_t hairline;
  uint32_t edge;
  uint32_t ink;
  uint32_t ink2;
  uint32_t ink3;
  uint32_t accent;
  uint32_t accentText;
  uint32_t onAccent;
  uint32_t accentSoft;
  uint32_t focus;
  uint32_t danger;
  uint32_t dangerText;
  uint32_t onDanger;
  uint32_t warning;
  uint32_t success;
  uint32_t scrim;
};

struct BandeAppearance {
  uint32_t dark;
  uint32_t reduceMotion;
  uint32_t reduceTransparency;
  uint32_t increasedContrast;
  uint32_t translucent;
  BandePalette light;
  BandePalette darkPalette;
};

class BandeHost final {
 public:
  static bool Load();
  static bool Start();
  static bool IsActive();
  static wl_display* Display();
  static int WakeWriteFd();

  static uint64_t Open(const char* aTitle, const char* aAppId, uint32_t aWidth,
                       uint32_t aHeight);
  static uint64_t OpenPopup(uint64_t aParent, double aX, double aY,
                            double aWidth, double aHeight, bool aGrab);
  static void MovePopup(uint64_t aWindow, double aX, double aY, double aWidth,
                        double aHeight);
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
  static void Copy(bool aPrimary, bool aConcealed, size_t aCount,
                   const char* const* aMimes, const uint8_t* const* aData,
                   const size_t* aLengths);
  static size_t ClipboardTypes(char* aOut, size_t aCapacity);
  static bool Paste(const char* aMime);
  static bool StartDrag(uint64_t aWindow, uint32_t aActions, size_t aCount,
                        const char* const* aMimes, const uint8_t* const* aData,
                        const size_t* aLengths);
  static void AcceptDrag(uint64_t aWindow, uint32_t aAction);
  static void PrintPdf(const char* aTitle, const char* aPath);
  static bool GetAppearance(BandeAppearance* aOut);
  static bool RemoteSend(const char* aName, const char* aPath,
                         const char* aInterface, const uint8_t* aCommand,
                         size_t aLength);
  static bool RemoteServe(const char* aName, const char* aPath,
                          const char* aInterface,
                          void (*aHandler)(const uint8_t*, size_t));
  static void RemoteStop();
  static void HandleRemote(const uint8_t* aCommand, size_t aLength);
  static void Activate(uint64_t aWindow, const char* aToken);
  static uint64_t Inhibit(uint32_t aFlags, const char* aReason);
  static bool MprisOpen(const char* aName, const char* aIdentity,
                        const char* aDesktopEntry);
  static void MprisClose();
  static bool Notify(const char* aId, const char* aTitle, const char* aBody,
                     const uint8_t* aIcon, size_t aIconLength,
                     const char* aButtons, bool aSilent, bool aPersistent);
  static void Unnotify(const char* aId);
  static void MprisStatus(uint32_t aStatus);
  static void MprisTrack(const char* aTitle, const char* aArtist,
                         const char* aAlbum, const uint8_t* aArt,
                         size_t aArtLength);
  static void MprisKeys(uint32_t aKeys);
  static void MprisPosition(double aDuration, double aPosition, double aRate);
  static void Uninhibit(uint64_t aInhibitor);
  static void ScreenCast(uint64_t aRequest, uint32_t aSources,
                         uint32_t aCursorMode, uint32_t aPersistMode,
                         const char* aRestoreToken);
  static void Camera(uint64_t aRequest);
  static void PortalStop(uint64_t aRequest);
  static void SetBlur(uint64_t aWindow, double aX, double aY, double aWidth,
                      double aHeight);
  static void ChooseFiles(uint64_t aWindow, uint32_t aRequest, uint32_t aMode,
                          const char* aTitle, const char* aFilters,
                          uint32_t aFilterIndex, const char* aName,
                          const char* aFolder);
  static void SetTextInput(uint64_t aWindow, bool aEnabled, double aX,
                           double aY, double aWidth, double aHeight);
  static void WatchFd(int aFd);
  static void UnwatchFd(int aFd);
  static bool Step(bool aBlock);
  static bool NextEvent(BandeEvent* aEvent);
};

void HandleBandeMediaAction(uint32_t aAction, double aValue);

}  // namespace mozilla::widget

#endif
