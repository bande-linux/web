/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsAppShell.h"

#include <fcntl.h>
#include <poll.h>
#include <unistd.h>
#include <wayland-client.h>

#include "BandeHost.h"
#include "gfxPlatform.h"
#include "nsWindow.h"

using namespace mozilla::widget;

nsAppShell::~nsAppShell() {
  if (mLocalPipe[0] >= 0) {
    close(mLocalPipe[0]);
    close(mLocalPipe[1]);
  }
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
      NativeEventCallback();
    } else {
      nsWindow::HandleBandeEvent(event);
    }
  }
  return handled;
}
