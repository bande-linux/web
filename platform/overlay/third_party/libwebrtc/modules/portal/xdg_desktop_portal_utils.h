/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#ifndef MODULES_PORTAL_XDG_DESKTOP_PORTAL_UTILS_H_
#define MODULES_PORTAL_XDG_DESKTOP_PORTAL_UTILS_H_

#include <cstdint>
#include <string>

#include "modules/portal/portal_request_response.h"
#include "rtc_base/system/rtc_export.h"

namespace webrtc {
namespace xdg_portal {

constexpr char kDesktopBusName[] = "org.freedesktop.portal.Desktop";
constexpr char kDesktopObjectPath[] = "/org/freedesktop/portal/desktop";
constexpr char kDesktopRequestObjectPath[] =
    "/org/freedesktop/portal/desktop/request";
constexpr char kSessionInterfaceName[] = "org.freedesktop.portal.Session";
constexpr char kRequestInterfaceName[] = "org.freedesktop.portal.Request";
constexpr char kScreenCastInterfaceName[] = "org.freedesktop.portal.ScreenCast";

RTC_EXPORT std::string RequestResponseToString(RequestResponse request);

RequestResponse RequestResponseFromPortalResponse(uint32_t portal_response);

}  // namespace xdg_portal
}  // namespace webrtc

#endif  // MODULES_PORTAL_XDG_DESKTOP_PORTAL_UTILS_H_
