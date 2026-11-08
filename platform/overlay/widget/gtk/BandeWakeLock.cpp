/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandeWakeLock.h"

#include "BandeHost.h"
#include "mozilla/StaticPtr.h"
#include "nsIPowerManagerService.h"
#include "nsServiceManagerUtils.h"

using namespace mozilla;
using mozilla::widget::BandeHost;

static constexpr uint32_t kInhibitSuspend = 4;
static constexpr uint32_t kInhibitIdle = 8;

static StaticRefPtr<BandeWakeLock> sWakeLock;

NS_IMPL_ISUPPORTS(BandeWakeLock, nsIDOMMozWakeLockListener)

BandeWakeLock::~BandeWakeLock() {
  if (mInhibitor) {
    BandeHost::Uninhibit(mInhibitor);
  }
}

nsresult BandeWakeLock::Callback(const nsAString& aTopic,
                                 const nsAString& aState) {
  const bool foregroundTopic =
      aTopic.EqualsLiteral("screen") || aTopic.EqualsLiteral("video-playing") ||
      aTopic.EqualsLiteral("autoscroll");
  const bool backgroundTopic = aTopic.EqualsLiteral("audio-playing") ||
                               aTopic.EqualsLiteral("download-in-progress");
  if (!foregroundTopic && !backgroundTopic) {
    return NS_OK;
  }
  const nsString topic(aTopic);
  mForeground.Remove(topic);
  mBackground.Remove(topic);
  if (aState.EqualsLiteral("locked-foreground")) {
    (foregroundTopic ? mForeground : mBackground).Insert(topic);
  } else if (aState.EqualsLiteral("locked-background")) {
    mBackground.Insert(topic);
  }
  Update();
  return NS_OK;
}

void BandeWakeLock::Update() {
  uint32_t flags = 0;
  if (!mForeground.IsEmpty()) {
    flags |= kInhibitIdle | kInhibitSuspend;
  }
  if (!mBackground.IsEmpty()) {
    flags |= kInhibitSuspend;
  }
  if (flags == mFlags) {
    return;
  }
  mFlags = flags;
  if (mInhibitor) {
    BandeHost::Uninhibit(mInhibitor);
    mInhibitor = 0;
  }
  if (flags) {
    mInhibitor = BandeHost::Inhibit(flags, "Playing media");
  }
}

void BandeWakeLock::Start() {
  nsCOMPtr<nsIPowerManagerService> powerManager =
      do_GetService(POWERMANAGERSERVICE_CONTRACTID);
  if (!powerManager || sWakeLock) {
    return;
  }
  sWakeLock = new BandeWakeLock();
  powerManager->AddWakeLockListener(sWakeLock);
}

void BandeWakeLock::Stop() {
  if (!sWakeLock) {
    return;
  }
  nsCOMPtr<nsIPowerManagerService> powerManager =
      do_GetService(POWERMANAGERSERVICE_CONTRACTID);
  if (powerManager) {
    powerManager->RemoveWakeLockListener(sWakeLock);
  }
  sWakeLock = nullptr;
}
