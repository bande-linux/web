/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#ifndef MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREEN_CAPTURE_PORTAL_INTERFACE_H_
#define MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREEN_CAPTURE_PORTAL_INTERFACE_H_

#include "modules/portal/portal_request_response.h"
#include "modules/portal/xdg_session_details.h"
#include "rtc_base/system/rtc_export.h"

namespace webrtc {
namespace xdg_portal {

class RTC_EXPORT ScreenCapturePortalInterface {
 public:
  virtual ~ScreenCapturePortalInterface() {}
  virtual xdg_portal::SessionDetails GetSessionDetails() { return {}; }
  virtual void Start() {}
  virtual void Stop() {}
  virtual void OnPortalDone(xdg_portal::RequestResponse result) {}
};

}  // namespace xdg_portal
}  // namespace webrtc

#endif  // MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREEN_CAPTURE_PORTAL_INTERFACE_H_
