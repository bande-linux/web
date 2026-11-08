/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "modules/portal/xdg_desktop_portal_utils.h"

#include <string>

namespace webrtc {
namespace xdg_portal {

std::string RequestResponseToString(RequestResponse request) {
  switch (request) {
    case RequestResponse::kUnknown:
      return "kUnknown";
    case RequestResponse::kSuccess:
      return "kSuccess";
    case RequestResponse::kUserCancelled:
      return "kUserCancelled";
    case RequestResponse::kError:
      return "kError";
    default:
      return "Uknown";
  }
}

RequestResponse RequestResponseFromPortalResponse(uint32_t portal_response) {
  switch (portal_response) {
    case 0:
      return RequestResponse::kSuccess;
    case 1:
      return RequestResponse::kUserCancelled;
    case 2:
      return RequestResponse::kError;
    default:
      return RequestResponse::kUnknown;
  }
}

}  // namespace xdg_portal
}  // namespace webrtc
