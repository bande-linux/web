/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandeFdWatch.h"

#include "BandeHost.h"
#include "nsTArray.h"

namespace mozilla::widget {

struct Watch {
  uint32_t mId;
  int mFd;
  BandeFdCallback mCallback;
  void* mData;
};

static nsTArray<Watch>& Watches() {
  static nsTArray<Watch> sWatches;
  return sWatches;
}

uint32_t BandeFdWatchAdd(int aFd, BandeFdCallback aCallback, void* aData) {
  if (!BandeHost::IsActive()) {
    return 0;
  }
  static uint32_t sNext = 1;
  Watches().AppendElement(Watch{sNext, aFd, aCallback, aData});
  BandeHost::WatchFd(aFd);
  return sNext++;
}

void BandeFdWatchRemove(uint32_t aWatch) {
  auto& watches = Watches();
  for (size_t i = 0; i < watches.Length(); i++) {
    if (watches[i].mId == aWatch) {
      BandeHost::UnwatchFd(watches[i].mFd);
      watches.RemoveElementAt(i);
      return;
    }
  }
}

void BandeFdWatchFire(int aFd) {
  auto& watches = Watches();
  for (size_t i = 0; i < watches.Length(); i++) {
    if (watches[i].mFd == aFd) {
      Watch watch = watches[i];
      if (!watch.mCallback(aFd, watch.mData)) {
        BandeFdWatchRemove(watch.mId);
      }
      return;
    }
  }
}

}  // namespace mozilla::widget
