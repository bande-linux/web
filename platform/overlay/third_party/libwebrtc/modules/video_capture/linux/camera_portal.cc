/*
 *  Copyright 2023 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "modules/video_capture/linux/camera_portal.h"

#include <unistd.h>

#include <cstdint>
#include <memory>

#include "api/scoped_refptr.h"
#include "modules/portal/bande_portal.h"
#include "modules/portal/pipewire_utils.h"
#include "modules/portal/portal_guard.h"
#include "modules/portal/portal_request_response.h"
#include "modules/portal/xdg_desktop_portal_utils.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"

namespace webrtc {

using xdg_portal::RequestResponse;
using xdg_portal::RequestResponseFromPortalResponse;

class CameraPortalPrivate {
 public:
  explicit CameraPortalPrivate(CameraPortal::PortalNotifier* notifier);
  ~CameraPortalPrivate();

  void Start();

 private:
  static void OnAnswer(void* data, uint32_t response, int fd);
  static void Retain(void* data);
  static void Release(void* data);

  CameraPortal::PortalNotifier* notifier_ = nullptr;
  uint64_t request_ = 0;
  scoped_refptr<PortalGuard> guard_;
};

CameraPortalPrivate::CameraPortalPrivate(CameraPortal::PortalNotifier* notifier)
    : notifier_(notifier) {}

CameraPortalPrivate::~CameraPortalPrivate() {
  if (guard_) {
    MutexLock lock(&guard_->mutex);
    guard_->portal = nullptr;
  }
  if (request_) {
    bande_portal_stop(request_);
  }
}

void CameraPortalPrivate::Retain(void* data) {
  static_cast<PortalGuard*>(data)->AddRef();
}

void CameraPortalPrivate::Release(void* data) {
  static_cast<PortalGuard*>(data)->Release();
}

void CameraPortalPrivate::Start() {
  if (guard_) {
    return;
  }
  guard_ = scoped_refptr<PortalGuard>(new PortalGuard());
  guard_->portal = this;
  guard_->AddRef();
  request_ = bande_portal_camera(OnAnswer, Retain, Release, guard_.get());
}

void CameraPortalPrivate::OnAnswer(void* data, uint32_t response, int fd) {
  auto* guard = static_cast<PortalGuard*>(data);
  MutexLock lock(&guard->mutex);
  auto* that = static_cast<CameraPortalPrivate*>(guard->portal);
  RequestResponse result = RequestResponseFromPortalResponse(response);
  if (result == RequestResponse::kSuccess && fd == kInvalidPipeWireFd) {
    result = RequestResponse::kError;
  }
  if (!that || !that->notifier_) {
    if (fd != kInvalidPipeWireFd) {
      close(fd);
    }
    return;
  }
  RTC_LOG(LS_INFO) << "Camera portal answered "
                   << xdg_portal::RequestResponseToString(result);
  CameraPortal::PortalNotifier* notifier = that->notifier_;
  that->notifier_ = nullptr;
  notifier->OnCameraRequestResult(
      result, result == RequestResponse::kSuccess ? fd : kInvalidPipeWireFd);
  if (result != RequestResponse::kSuccess && fd != kInvalidPipeWireFd) {
    close(fd);
  }
}

CameraPortal::CameraPortal(PortalNotifier* notifier)
    : private_(std::make_unique<CameraPortalPrivate>(notifier)) {}

CameraPortal::~CameraPortal() {}

void CameraPortal::Start() {
  private_->Start();
}

}  // namespace webrtc
