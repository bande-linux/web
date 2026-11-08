/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandeHost.h"

#include <dlfcn.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>

#include "mozilla/Logging.h"
#include "nsString.h"

namespace mozilla::widget {

static LazyLogModule sBandeLog("Bande");

#define BANDE_FUNCTIONS(X)                                             \
  X(init, void*, int)                                                  \
  X(open, uint64_t, const char*, const char*, uint32_t, uint32_t)      \
  X(open_popup, uint64_t, uint64_t, double, double, double, double, bool) \
  X(move_popup, void, uint64_t, double, double, double, double)        \
  X(close, void, uint64_t)                                             \
  X(surface, void*, uint64_t)                                          \
  X(set_title, void, uint64_t, const char*)                            \
  X(set_size, void, uint64_t, uint32_t, uint32_t)                      \
  X(size, void, uint64_t, uint32_t*, uint32_t*)                        \
  X(set_limits, void, uint64_t, uint32_t, uint32_t, uint32_t, uint32_t) \
  X(scale, double, uint64_t)                                           \
  X(start_move, void, uint64_t)                                        \
  X(start_resize, void, uint64_t, uint32_t)                            \
  X(minimize, void, uint64_t)                                          \
  X(toggle_zoom, void, uint64_t)                                       \
  X(set_full_screen, void, uint64_t, bool)                             \
  X(set_cursor, void, uint64_t, uint32_t)                              \
  X(watch_fd, void, int)                                               \
  X(unwatch_fd, void, int)                                             \
  X(copy, void, bool, bool, size_t, const char* const*, const uint8_t* const*, const size_t*) \
  X(clipboard_types, size_t, char*, size_t)                            \
  X(start_drag, bool, uint64_t, uint32_t, size_t, const char* const*, const uint8_t* const*, const size_t*) \
  X(set_text_input, void, uint64_t, bool, double, double, double, double) \
  X(choose_files, void, uint64_t, uint32_t, uint32_t, const char*, const char*, uint32_t, const char*, const char*) \
  X(print_pdf, void, const char*, const char*)                         \
  X(appearance, bool, BandeAppearance*)                                \
  X(screencast, void, uint64_t, uint32_t, uint32_t, uint32_t, const char*) \
  X(camera, void, uint64_t)                                            \
  X(portal_stop, void, uint64_t)                                       \
  X(remote_send, bool, const char*, const char*, const char*, const uint8_t*, size_t) \
  X(remote_serve, bool, const char*, const char*, const char*)         \
  X(activate, void, uint64_t, const char*)                             \
  X(inhibit, uint64_t, uint32_t, const char*)                          \
  X(uninhibit, void, uint64_t)                                         \
  X(mpris_open, bool, const char*, const char*, const char*)           \
  X(mpris_close, void)                                                 \
  X(notify, bool, const char*, const char*, const char*, const uint8_t*, size_t, const char*, bool, bool) \
  X(unnotify, void, const char*)                                       \
  X(mpris_status, void, uint32_t)                                      \
  X(mpris_track, void, const char*, const char*, const char*, const uint8_t*, size_t) \
  X(mpris_keys, void, uint32_t)                                        \
  X(mpris_position, void, double, double, double)                      \
  X(set_blur, void, uint64_t, double, double, double, double)          \
  X(accept_drag, void, uint64_t, uint32_t)                             \
  X(paste, bool, const char*)                                          \
  X(flush, void)                                                       \
  X(step, bool, bool)                                                  \
  X(next_event, bool, BandeEvent*)

#define BANDE_DECLARE(name, ret, ...) static ret (*s_##name)(__VA_ARGS__);
BANDE_FUNCTIONS(BANDE_DECLARE)

static wl_display* sDisplay = nullptr;
static int sWakeWrite = -1;
static bool sTried = false;

static void* OpenLibrary() {
  Dl_info info;
  if (!dladdr(reinterpret_cast<void*>(&OpenLibrary), &info) ||
      !info.dli_fname) {
    return nullptr;
  }
  nsAutoCString path(info.dli_fname);
  int32_t slash = path.RFindChar('/');
  if (slash < 0) {
    return nullptr;
  }
  path.Truncate(slash + 1);
  path.AppendLiteral("libbande_gecko.so");
  void* library = dlopen(path.get(), RTLD_NOW | RTLD_LOCAL);
  if (!library) {
    MOZ_LOG(sBandeLog, LogLevel::Error, ("%s", dlerror()));
  }
  return library;
}

static bool LoadFunctions() {
  void* library = OpenLibrary();
  if (!library) {
    return false;
  }
#define BANDE_LOOKUP(name, ret, ...)                              \
  s_##name = reinterpret_cast<ret (*)(__VA_ARGS__)>(              \
      dlsym(library, "bande_gecko_" #name));                      \
  if (!s_##name) {                                                \
    return false;                                                 \
  }
  BANDE_FUNCTIONS(BANDE_LOOKUP)
  return true;
}

bool BandeHost::Load() {
  static const bool sLoaded = LoadFunctions();
  return sLoaded;
}

bool BandeHost::Start() {
  if (sTried) {
    return !!sDisplay;
  }
  sTried = true;
  if (!Load()) {
    return false;
  }
  int fds[2];
  if (pipe2(fds, O_NONBLOCK | O_CLOEXEC)) {
    return false;
  }
  sWakeWrite = fds[1];
  sDisplay = static_cast<wl_display*>(s_init(fds[0]));
  return !!sDisplay;
}

bool BandeHost::IsActive() { return !!sDisplay; }
wl_display* BandeHost::Display() { return sDisplay; }
int BandeHost::WakeWriteFd() { return sWakeWrite; }

uint64_t BandeHost::Open(const char* aTitle, const char* aAppId,
                         uint32_t aWidth, uint32_t aHeight) {
  return s_open(aTitle, aAppId, aWidth, aHeight);
}
void BandeHost::Close(uint64_t aWindow) { s_close(aWindow); }
uint64_t BandeHost::OpenPopup(uint64_t aParent, double aX, double aY,
                              double aWidth, double aHeight, bool aGrab) {
  return s_open_popup(aParent, aX, aY, aWidth, aHeight, aGrab);
}
void BandeHost::MovePopup(uint64_t aWindow, double aX, double aY,
                          double aWidth, double aHeight) {
  s_move_popup(aWindow, aX, aY, aWidth, aHeight);
}
wl_surface* BandeHost::Surface(uint64_t aWindow) {
  return static_cast<wl_surface*>(s_surface(aWindow));
}
void BandeHost::SetTitle(uint64_t aWindow, const char* aTitle) {
  s_set_title(aWindow, aTitle);
}
void BandeHost::SetSize(uint64_t aWindow, uint32_t aWidth, uint32_t aHeight) {
  s_set_size(aWindow, aWidth, aHeight);
}
void BandeHost::Size(uint64_t aWindow, uint32_t* aWidth, uint32_t* aHeight) {
  s_size(aWindow, aWidth, aHeight);
}
void BandeHost::SetLimits(uint64_t aWindow, uint32_t aMinWidth,
                          uint32_t aMinHeight, uint32_t aMaxWidth,
                          uint32_t aMaxHeight) {
  s_set_limits(aWindow, aMinWidth, aMinHeight, aMaxWidth, aMaxHeight);
}
double BandeHost::Scale(uint64_t aWindow) { return s_scale(aWindow); }
void BandeHost::StartMove(uint64_t aWindow) { s_start_move(aWindow); }
void BandeHost::StartResize(uint64_t aWindow, uint32_t aEdge) {
  s_start_resize(aWindow, aEdge);
}
void BandeHost::Minimize(uint64_t aWindow) { s_minimize(aWindow); }
void BandeHost::ToggleZoom(uint64_t aWindow) { s_toggle_zoom(aWindow); }
void BandeHost::SetFullScreen(uint64_t aWindow, bool aFullScreen) {
  s_set_full_screen(aWindow, aFullScreen);
}
void BandeHost::SetCursor(uint64_t aWindow, uint32_t aKind) {
  s_set_cursor(aWindow, aKind);
}
void BandeHost::Flush() { s_flush(); }
void BandeHost::Copy(bool aPrimary, bool aConcealed, size_t aCount,
                     const char* const* aMimes, const uint8_t* const* aData,
                     const size_t* aLengths) {
  s_copy(aPrimary, aConcealed, aCount, aMimes, aData, aLengths);
}
size_t BandeHost::ClipboardTypes(char* aOut, size_t aCapacity) {
  return s_clipboard_types(aOut, aCapacity);
}
bool BandeHost::Paste(const char* aMime) { return s_paste(aMime); }
bool BandeHost::StartDrag(uint64_t aWindow, uint32_t aActions, size_t aCount,
                          const char* const* aMimes,
                          const uint8_t* const* aData,
                          const size_t* aLengths) {
  return s_start_drag(aWindow, aActions, aCount, aMimes, aData, aLengths);
}
void BandeHost::SetTextInput(uint64_t aWindow, bool aEnabled, double aX,
                             double aY, double aWidth, double aHeight) {
  s_set_text_input(aWindow, aEnabled, aX, aY, aWidth, aHeight);
}
void BandeHost::ChooseFiles(uint64_t aWindow, uint32_t aRequest,
                            uint32_t aMode, const char* aTitle,
                            const char* aFilters, uint32_t aFilterIndex,
                            const char* aName, const char* aFolder) {
  s_choose_files(aWindow, aRequest, aMode, aTitle, aFilters, aFilterIndex,
                 aName, aFolder);
}
void BandeHost::PrintPdf(const char* aTitle, const char* aPath) {
  s_print_pdf(aTitle, aPath);
}
void BandeHost::ScreenCast(uint64_t aRequest, uint32_t aSources,
                           uint32_t aCursorMode, uint32_t aPersistMode,
                           const char* aRestoreToken) {
  s_screencast(aRequest, aSources, aCursorMode, aPersistMode, aRestoreToken);
}
void BandeHost::Camera(uint64_t aRequest) { s_camera(aRequest); }
void BandeHost::PortalStop(uint64_t aRequest) { s_portal_stop(aRequest); }
bool BandeHost::RemoteSend(const char* aName, const char* aPath,
                           const char* aInterface, const uint8_t* aCommand,
                           size_t aLength) {
  return Load() && s_remote_send(aName, aPath, aInterface, aCommand, aLength);
}
static void (*sRemoteHandler)(const uint8_t*, size_t) = nullptr;
bool BandeHost::RemoteServe(const char* aName, const char* aPath,
                            const char* aInterface,
                            void (*aHandler)(const uint8_t*, size_t)) {
  if (!IsActive() || !s_remote_serve(aName, aPath, aInterface)) {
    return false;
  }
  sRemoteHandler = aHandler;
  return true;
}
void BandeHost::RemoteStop() { sRemoteHandler = nullptr; }
void BandeHost::HandleRemote(const uint8_t* aCommand, size_t aLength) {
  if (sRemoteHandler) {
    sRemoteHandler(aCommand, aLength);
  }
}
uint64_t BandeHost::Inhibit(uint32_t aFlags, const char* aReason) {
  return IsActive() ? s_inhibit(aFlags, aReason) : 0;
}
void BandeHost::Uninhibit(uint64_t aInhibitor) {
  if (IsActive()) {
    s_uninhibit(aInhibitor);
  }
}
bool BandeHost::MprisOpen(const char* aName, const char* aIdentity,
                          const char* aDesktopEntry) {
  return IsActive() && s_mpris_open(aName, aIdentity, aDesktopEntry);
}
bool BandeHost::Notify(const char* aId, const char* aTitle, const char* aBody,
                       const uint8_t* aIcon, size_t aIconLength,
                       const char* aButtons, bool aSilent, bool aPersistent) {
  return IsActive() && s_notify(aId, aTitle, aBody, aIcon, aIconLength,
                                aButtons, aSilent, aPersistent);
}
void BandeHost::Unnotify(const char* aId) {
  if (IsActive()) {
    s_unnotify(aId);
  }
}
void BandeHost::MprisClose() { s_mpris_close(); }
void BandeHost::MprisStatus(uint32_t aStatus) { s_mpris_status(aStatus); }
void BandeHost::MprisTrack(const char* aTitle, const char* aArtist,
                           const char* aAlbum, const uint8_t* aArt,
                           size_t aArtLength) {
  s_mpris_track(aTitle, aArtist, aAlbum, aArt, aArtLength);
}
void BandeHost::MprisKeys(uint32_t aKeys) { s_mpris_keys(aKeys); }
void BandeHost::MprisPosition(double aDuration, double aPosition,
                              double aRate) {
  s_mpris_position(aDuration, aPosition, aRate);
}
void BandeHost::Activate(uint64_t aWindow, const char* aToken) {
  s_activate(aWindow, aToken);
}
bool BandeHost::GetAppearance(BandeAppearance* aOut) {
  return s_appearance(aOut);
}
void BandeHost::SetBlur(uint64_t aWindow, double aX, double aY, double aWidth,
                        double aHeight) {
  s_set_blur(aWindow, aX, aY, aWidth, aHeight);
}
void BandeHost::AcceptDrag(uint64_t aWindow, uint32_t aAction) {
  s_accept_drag(aWindow, aAction);
}
void BandeHost::WatchFd(int aFd) { s_watch_fd(aFd); }
void BandeHost::UnwatchFd(int aFd) { s_unwatch_fd(aFd); }
bool BandeHost::Step(bool aBlock) { return s_step(aBlock); }
bool BandeHost::NextEvent(BandeEvent* aEvent) { return s_next_event(aEvent); }

}  // namespace mozilla::widget
