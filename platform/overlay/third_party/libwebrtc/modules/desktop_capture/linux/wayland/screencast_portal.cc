/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "modules/desktop_capture/linux/wayland/screencast_portal.h"

#include <unistd.h>

#include <cstdint>
#include <string>

#include "modules/desktop_capture/desktop_capture_types.h"
#include "modules/portal/bande_portal.h"
#include "modules/portal/pipewire_utils.h"
#include "modules/portal/portal_guard.h"
#include "modules/portal/portal_request_response.h"
#include "modules/portal/xdg_desktop_portal_utils.h"
#include "modules/portal/xdg_session_details.h"
#include "rtc_base/logging.h"

namespace webrtc {

using xdg_portal::RequestResponse;
using xdg_portal::RequestResponseFromPortalResponse;

ScreenCastPortal::CaptureSourceType ScreenCastPortal::ToCaptureSourceType(
    CaptureType type) {
  switch (type) {
    case CaptureType::kScreen:
      return ScreenCastPortal::CaptureSourceType::kScreen;
    case CaptureType::kWindow:
      return ScreenCastPortal::CaptureSourceType::kWindow;
    case CaptureType::kAnyScreenContent:
      return ScreenCastPortal::CaptureSourceType::kAnyScreenContent;
  }
  return ScreenCastPortal::CaptureSourceType::kScreen;
}

ScreenCastPortal::ScreenCastPortal(CaptureType type,
                                   PortalNotifier* notifier,
                                   bool prefer_cursor_embedded)
    : notifier_(notifier),
      capture_source_type_(ToCaptureSourceType(type)),
      cursor_mode_(prefer_cursor_embedded ? CursorMode::kEmbedded
                                          : CursorMode::kMetadata) {}

ScreenCastPortal::~ScreenCastPortal() {
  Stop();
}

void ScreenCastPortal::Retain(void* data) {
  static_cast<PortalGuard*>(data)->AddRef();
}

void ScreenCastPortal::Release(void* data) {
  static_cast<PortalGuard*>(data)->Release();
}

void ScreenCastPortal::Start() {
  Stop();
  guard_ = scoped_refptr<PortalGuard>(new PortalGuard());
  guard_->portal = this;
  guard_->AddRef();
  request_ = bande_portal_screencast(
      static_cast<uint32_t>(capture_source_type_),
      static_cast<uint32_t>(cursor_mode_),
      static_cast<uint32_t>(persist_mode_), restore_token_.c_str(), OnAnswer,
      OnClosed, Retain, Release, guard_.get());
}

void ScreenCastPortal::Stop() {
  if (guard_) {
    MutexLock lock(&guard_->mutex);
    guard_->portal = nullptr;
  }
  guard_ = nullptr;
  if (request_) {
    bande_portal_stop(request_);
    request_ = 0;
  }
  if (pw_fd_ != kInvalidPipeWireFd) {
    close(pw_fd_);
    pw_fd_ = kInvalidPipeWireFd;
  }
}

void ScreenCastPortal::OnAnswer(void* data,
                                uint32_t response,
                                uint32_t node,
                                int fd,
                                const char* restore_token) {
  auto* guard = static_cast<PortalGuard*>(data);
  MutexLock lock(&guard->mutex);
  auto* that = static_cast<ScreenCastPortal*>(guard->portal);
  if (!that) {
    if (fd != kInvalidPipeWireFd) {
      close(fd);
    }
    return;
  }
  RequestResponse result = RequestResponseFromPortalResponse(response);
  if (result == RequestResponse::kSuccess &&
      fd == kInvalidPipeWireFd) {
    result = RequestResponse::kError;
  }
  if (result == RequestResponse::kSuccess) {
    that->pw_stream_node_id_ = node;
    that->pw_fd_ = fd;
    if (restore_token && *restore_token) {
      that->restore_token_ = restore_token;
    }
  } else if (fd != kInvalidPipeWireFd) {
    close(fd);
  }
  RTC_LOG(LS_INFO) << "Screen cast portal answered "
                   << xdg_portal::RequestResponseToString(result);
  that->OnPortalDone(result);
}

void ScreenCastPortal::OnClosed(void* data) {
  auto* guard = static_cast<PortalGuard*>(data);
  MutexLock lock(&guard->mutex);
  if (auto* that = static_cast<ScreenCastPortal*>(guard->portal)) {
    that->notifier_->OnScreenCastSessionClosed();
  }
}

xdg_portal::SessionDetails ScreenCastPortal::GetSessionDetails() {
  return {};
}

void ScreenCastPortal::OnPortalDone(RequestResponse result) {
  notifier_->OnScreenCastRequestResult(result, pw_stream_node_id_, pw_fd_);
}

void ScreenCastPortal::SetSessionDetails(
    const xdg_portal::SessionDetails& session_details) {
  if (session_details.pipewire_stream_node_id) {
    pw_stream_node_id_ = session_details.pipewire_stream_node_id;
  }
}

uint32_t ScreenCastPortal::pipewire_stream_node_id() {
  return pw_stream_node_id_;
}

void ScreenCastPortal::SetPersistMode(ScreenCastPortal::PersistMode mode) {
  persist_mode_ = mode;
}

void ScreenCastPortal::SetRestoreToken(const std::string& token) {
  restore_token_ = token;
}

std::string ScreenCastPortal::RestoreToken() const {
  return restore_token_;
}

}  // namespace webrtc
