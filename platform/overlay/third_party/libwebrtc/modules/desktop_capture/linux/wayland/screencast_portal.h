/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#ifndef MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREENCAST_PORTAL_H_
#define MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREENCAST_PORTAL_H_

#include <cstdint>
#include <string>

#include "api/scoped_refptr.h"
#include "modules/desktop_capture/desktop_capture_types.h"
#include "modules/desktop_capture/linux/wayland/screen_capture_portal_interface.h"
#include "modules/portal/pipewire_utils.h"
#include "modules/portal/portal_guard.h"
#include "modules/portal/portal_request_response.h"
#include "modules/portal/xdg_session_details.h"
#include "rtc_base/system/rtc_export.h"

namespace webrtc {

class RTC_EXPORT ScreenCastPortal
    : public xdg_portal::ScreenCapturePortalInterface {
 public:
  enum class CursorMode : uint32_t {
    kHidden = 0b01,
    kEmbedded = 0b10,
    kMetadata = 0b100
  };
  enum class PersistMode : uint32_t {
    kDoNotPersist = 0b00,
    kTransient = 0b01,
    kPersistent = 0b10
  };

  class PortalNotifier {
   public:
    virtual void OnScreenCastRequestResult(xdg_portal::RequestResponse result,
                                           uint32_t stream_node_id,
                                           int fd) = 0;
    virtual void OnScreenCastSessionClosed() = 0;

   protected:
    PortalNotifier() = default;
    virtual ~PortalNotifier() = default;
  };

  ScreenCastPortal(CaptureType type,
                   PortalNotifier* notifier,
                   bool prefer_cursor_embedded);
  ~ScreenCastPortal() override;

  void Start() override;
  void Stop() override;
  xdg_portal::SessionDetails GetSessionDetails() override;
  void OnPortalDone(xdg_portal::RequestResponse result) override;

  void SetSessionDetails(const xdg_portal::SessionDetails& session_details);
  uint32_t pipewire_stream_node_id();
  void SetPersistMode(ScreenCastPortal::PersistMode mode);
  void SetRestoreToken(const std::string& token);
  std::string RestoreToken() const;

 private:
  enum class CaptureSourceType : uint32_t {
    kScreen = 0b01,
    kWindow = 0b10,
    kAnyScreenContent = kScreen | kWindow
  };
  static CaptureSourceType ToCaptureSourceType(CaptureType type);

  static void OnAnswer(void* data,
                       uint32_t response,
                       uint32_t node,
                       int fd,
                       const char* restore_token);
  static void OnClosed(void* data);
  static void Retain(void* data);
  static void Release(void* data);

  PortalNotifier* notifier_;
  uint32_t pw_stream_node_id_ = 0;
  int pw_fd_ = kInvalidPipeWireFd;
  std::string restore_token_;
  CaptureSourceType capture_source_type_ =
      ScreenCastPortal::CaptureSourceType::kScreen;
  CursorMode cursor_mode_ = CursorMode::kMetadata;
  PersistMode persist_mode_ = ScreenCastPortal::PersistMode::kDoNotPersist;
  uint64_t request_ = 0;
  scoped_refptr<PortalGuard> guard_;
};

}  // namespace webrtc

#endif  // MODULES_DESKTOP_CAPTURE_LINUX_WAYLAND_SCREENCAST_PORTAL_H_
