/*
 *  Copyright 2022 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#ifndef MODULES_PORTAL_BANDE_PORTAL_H_
#define MODULES_PORTAL_BANDE_PORTAL_H_

#include <stdint.h>

extern "C" {
typedef void (*BandePortalRef)(void* aData);
typedef void (*BandeScreenCastAnswer)(void* aData, uint32_t aResponse,
                                      uint32_t aNode, int aFd,
                                      const char* aRestoreToken);
typedef void (*BandeScreenCastClosed)(void* aData);
typedef void (*BandeCameraAnswer)(void* aData, uint32_t aResponse, int aFd);

uint64_t bande_portal_screencast(uint32_t aSources, uint32_t aCursorMode,
                                 uint32_t aPersistMode,
                                 const char* aRestoreToken,
                                 BandeScreenCastAnswer aAnswer,
                                 BandeScreenCastClosed aClosed,
                                 BandePortalRef aRetain,
                                 BandePortalRef aRelease, void* aData);
uint64_t bande_portal_camera(BandeCameraAnswer aAnswer, BandePortalRef aRetain,
                             BandePortalRef aRelease, void* aData);
void bande_portal_stop(uint64_t aRequest);
}

#endif  // MODULES_PORTAL_BANDE_PORTAL_H_
