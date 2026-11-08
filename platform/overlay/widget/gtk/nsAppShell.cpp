/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsAppShell.h"

#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <unistd.h>
#include <wayland-client.h>

#include "BandeFdWatch.h"
#include "BandeHost.h"
#include "BandePortal.h"
#include "BandeWakeLock.h"
#include "nsBandeAlertsService.h"
#include "nsClipboard.h"
#include "nsFilePicker.h"
#include "nsLookAndFeel.h"
#include "gfxPlatform.h"
#include "mozilla/Atomics.h"
#include "mozilla/Logging.h"
#include "mozilla/Components.h"
#include "nsIAppStartup.h"
#include "nsWindow.h"

using namespace mozilla;
using namespace mozilla::widget;

LazyLogModule gWidgetLog("Widget");
LazyLogModule gWidgetDragLog("WidgetDrag");
LazyLogModule gWidgetWaylandLog("WidgetWayland");
LazyLogModule gWidgetPopupLog("WidgetPopup");
LazyLogModule gWidgetVsync("WidgetVSync");
LazyLogModule gDmabufLog("Dmabuf");
LazyLogModule gWidgetCompositorLog("WidgetCompositor");

static Atomic<bool> sTermRequested(false);
static int sSignalWakeFd = -1;

static void TermSignalHandler(int) {
  sTermRequested = true;
  unsigned char byte = 1;
  (void)!write(sSignalWakeFd, &byte, 1);
}

void nsAppShell::InstallTermSignalHandler() {
  struct sigaction action{};
  action.sa_handler = TermSignalHandler;
  sigaction(SIGTERM, &action, nullptr);
}

static void QuitIfTermRequested() {
  if (sTermRequested.exchange(false)) {
    nsCOMPtr<nsIAppStartup> startup = components::AppStartup::Service();
    if (startup) {
      bool userAllowedQuit = true;
      startup->Quit(nsIAppStartup::eForceQuit, 0, &userAllowedQuit);
    }
  }
}

nsAppShell::~nsAppShell() {
  if (mLocalPipe[0] >= 0) {
    close(mLocalPipe[0]);
    close(mLocalPipe[1]);
  }
}

NS_IMETHODIMP nsAppShell::Run() {
  const bool wakeLocks = XRE_IsParentProcess() && BandeHost::IsActive();
  if (wakeLocks) {
    BandeWakeLock::Start();
  }
  nsresult rv = nsBaseAppShell::Run();
  if (wakeLocks) {
    BandeWakeLock::Stop();
  }
  return rv;
}

nsresult nsAppShell::Init() {
  if (XRE_IsParentProcess() && !gfxPlatform::IsHeadless() &&
      BandeHost::Start()) {
    mWakeWrite = BandeHost::WakeWriteFd();
  } else {
    if (pipe2(mLocalPipe, O_NONBLOCK | O_CLOEXEC)) {
      return NS_ERROR_FAILURE;
    }
    mWakeWrite = mLocalPipe[1];
  }
  sSignalWakeFd = mWakeWrite;
  return nsBaseAppShell::Init();
}

void nsAppShell::ScheduleNativeEventCallback() {
  unsigned char byte = 1;
  (void)!write(mWakeWrite, &byte, 1);
}

bool nsAppShell::ProcessProcessLocal(bool aMayWait) {
  pollfd fd{mLocalPipe[0], POLLIN, 0};
  if (poll(&fd, 1, aMayWait ? -1 : 0) <= 0) {
    return false;
  }
  unsigned char bytes[64];
  while (read(mLocalPipe[0], bytes, sizeof(bytes)) > 0) {
  }
  QuitIfTermRequested();
  NativeEventCallback();
  return true;
}

bool nsAppShell::ProcessNextNativeEvent(bool aMayWait) {
  if (!BandeHost::IsActive()) {
    return ProcessProcessLocal(aMayWait);
  }
  wl_display* display = BandeHost::Display();
  bool handled = wl_display_dispatch_pending(display) > 0;
  wl_display_flush(display);
  BandeHost::Step(aMayWait && !handled);
  handled |= wl_display_dispatch_pending(display) > 0;
  BandeEvent event;
  while (BandeHost::NextEvent(&event)) {
    handled = true;
    if (event.kind == BandeEventKind::Wake) {
      QuitIfTermRequested();
      NativeEventCallback();
    } else if (event.kind == BandeEventKind::ClipboardData) {
      nsClipboard::HandleData(event.text, event.data, event.len);
    } else if (event.kind == BandeEventKind::FilesChosen) {
      nsFilePicker::HandleAnswer(
          event.a, event.b,
          nsDependentCSubstring(reinterpret_cast<const char*>(event.data),
                                event.len));
    } else if (event.kind == BandeEventKind::ClipboardChanged) {
      nsClipboard::HandleChanged();
    } else if (event.kind == BandeEventKind::ScreenCast ||
               event.kind == BandeEventKind::ScreenCastClosed ||
               event.kind == BandeEventKind::Camera) {
      BandePortalHandle(event);
    } else if (event.kind == BandeEventKind::Notification) {
      nsDependentCSubstring answer(reinterpret_cast<const char*>(event.data),
                                   event.len);
      const int32_t split = answer.FindChar('\n');
      nsDependentCString id(event.text ? event.text : "");
      if (split >= 0) {
        nsBandeAlertsService::HandleAction(id, Substring(answer, 0, split),
                                           Substring(answer, split + 1));
      } else {
        nsBandeAlertsService::HandleAction(id, answer, ""_ns);
      }
    } else if (event.kind == BandeEventKind::MediaKey) {
      HandleBandeMediaAction(event.a, event.x);
    } else if (event.kind == BandeEventKind::Remote) {
      BandeHost::HandleRemote(event.data, event.len);
    } else if (event.kind == BandeEventKind::Appearance) {
      nsLookAndFeel::HandleAppearanceChanged();
    } else if (event.kind == BandeEventKind::FdReady) {
      BandeFdWatchFire(int(event.a));
    } else {
      nsWindow::HandleBandeEvent(event);
    }
  }
  return handled;
}
