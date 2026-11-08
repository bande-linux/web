/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandePortal.h"

#include <unistd.h>

#include "mozilla/Atomics.h"
#include "mozilla/StaticMutex.h"
#include "nsTHashMap.h"
#include "nsThreadUtils.h"

extern "C" {
typedef void (*BandePortalRef)(void* aData);
typedef void (*BandeScreenCastAnswer)(void* aData, uint32_t aResponse,
                                      uint32_t aNode, int aFd,
                                      const char* aRestoreToken);
typedef void (*BandeScreenCastClosed)(void* aData);
typedef void (*BandeCameraAnswer)(void* aData, uint32_t aResponse, int aFd);
}

namespace mozilla::widget {

static constexpr uint32_t kPortalError = 2;

struct PortalEntry {
  BandeScreenCastAnswer mScreenCast = nullptr;
  BandeScreenCastClosed mClosed = nullptr;
  BandeCameraAnswer mCamera = nullptr;
  BandePortalRef mRetain = nullptr;
  BandePortalRef mRelease = nullptr;
  void* mData = nullptr;
};

static StaticMutex sPortalLock MOZ_UNANNOTATED;
static nsTHashMap<uint64_t, PortalEntry>& PortalEntries() {
  static nsTHashMap<uint64_t, PortalEntry> sEntries;
  return sEntries;
}
static Atomic<uint64_t> sNextRequest(1);

static uint64_t PortalRegister(const PortalEntry& aEntry) {
  const uint64_t request = sNextRequest++;
  StaticMutexAutoLock lock(sPortalLock);
  PortalEntries().InsertOrUpdate(request, aEntry);
  return request;
}

static bool PortalTake(uint64_t aRequest, PortalEntry& aEntry) {
  StaticMutexAutoLock lock(sPortalLock);
  auto found = PortalEntries().Lookup(aRequest);
  if (!found) {
    return false;
  }
  aEntry = found.Data();
  aEntry.mRetain(aEntry.mData);
  return true;
}

static void PortalAnswer(uint64_t aRequest, uint32_t aResponse, uint32_t aNode,
                   int aFd, const char* aToken) {
  PortalEntry entry;
  if (!PortalTake(aRequest, entry)) {
    if (aFd >= 0) {
      close(aFd);
    }
    return;
  }
  if (entry.mScreenCast) {
    entry.mScreenCast(entry.mData, aResponse, aNode, aFd, aToken ? aToken : "");
  } else if (entry.mCamera) {
    entry.mCamera(entry.mData, aResponse, aFd);
  } else if (aFd >= 0) {
    close(aFd);
  }
  entry.mRelease(entry.mData);
}

static void PortalClosed(uint64_t aRequest) {
  PortalEntry entry;
  if (!PortalTake(aRequest, entry)) {
    return;
  }
  if (entry.mClosed) {
    entry.mClosed(entry.mData);
  }
  entry.mRelease(entry.mData);
}

void BandePortalHandle(const BandeEvent& aEvent) {
  switch (aEvent.kind) {
    case BandeEventKind::ScreenCast:
      PortalAnswer(aEvent.window, aEvent.b, aEvent.c, int32_t(aEvent.d), aEvent.text);
      break;
    case BandeEventKind::ScreenCastClosed:
      PortalClosed(aEvent.window);
      break;
    case BandeEventKind::Camera:
      PortalAnswer(aEvent.window, aEvent.b, 0, int32_t(aEvent.d), nullptr);
      break;
    default:
      break;
  }
}

}  // namespace mozilla::widget

using namespace mozilla;
using namespace mozilla::widget;

extern "C" uint64_t bande_portal_screencast(
    uint32_t aSources, uint32_t aCursorMode, uint32_t aPersistMode,
    const char* aRestoreToken, BandeScreenCastAnswer aAnswer,
    BandeScreenCastClosed aClosed, BandePortalRef aRetain,
    BandePortalRef aRelease, void* aData) {
  PortalEntry entry;
  entry.mScreenCast = aAnswer;
  entry.mClosed = aClosed;
  entry.mRetain = aRetain;
  entry.mRelease = aRelease;
  entry.mData = aData;
  const uint64_t request = PortalRegister(entry);
  nsCString token(aRestoreToken ? aRestoreToken : "");
  NS_DispatchToMainThread(NS_NewRunnableFunction(
      "BandeScreenCast", [request, aSources, aCursorMode, aPersistMode,
                          token]() {
        if (!BandeHost::IsActive()) {
          PortalAnswer(request, kPortalError, 0, -1, nullptr);
          return;
        }
        BandeHost::ScreenCast(request, aSources, aCursorMode, aPersistMode,
                              token.get());
      }));
  return request;
}

extern "C" uint64_t bande_portal_camera(BandeCameraAnswer aAnswer,
                                        BandePortalRef aRetain,
                                        BandePortalRef aRelease, void* aData) {
  PortalEntry entry;
  entry.mCamera = aAnswer;
  entry.mRetain = aRetain;
  entry.mRelease = aRelease;
  entry.mData = aData;
  const uint64_t request = PortalRegister(entry);
  NS_DispatchToMainThread(
      NS_NewRunnableFunction("BandeCamera", [request]() {
        if (!BandeHost::IsActive()) {
          PortalAnswer(request, kPortalError, 0, -1, nullptr);
          return;
        }
        BandeHost::Camera(request);
      }));
  return request;
}

extern "C" void bande_portal_stop(uint64_t aRequest) {
  PortalEntry entry;
  {
    StaticMutexAutoLock lock(sPortalLock);
    auto found = PortalEntries().Lookup(aRequest);
    if (!found) {
      return;
    }
    entry = found.Data();
    found.Remove();
  }
  entry.mRelease(entry.mData);
  NS_DispatchToMainThread(NS_NewRunnableFunction(
      "BandePortalStop", [aRequest]() {
        if (BandeHost::IsActive()) {
          BandeHost::PortalStop(aRequest);
        }
      }));
}
