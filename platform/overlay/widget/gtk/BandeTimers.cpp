/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "BandeTimers.h"

#include "mozilla/ClearOnShutdown.h"
#include "mozilla/StaticPtr.h"
#include "nsComponentManagerUtils.h"
#include "nsITimer.h"
#include "nsTHashMap.h"

namespace mozilla::widget {

static nsTHashMap<uint32_t, nsCOMPtr<nsITimer>>& Timers() {
  static nsTHashMap<uint32_t, nsCOMPtr<nsITimer>> sTimers;
  return sTimers;
}

uint32_t BandeTimeoutAdd(uint32_t aMilliseconds, BandeTimerCallback aCallback,
                         void* aData) {
  static uint32_t sNextTimer = 1;
  const uint32_t id = sNextTimer++;
  nsCOMPtr<nsITimer> timer;
  nsresult rv = NS_NewTimerWithCallback(
      getter_AddRefs(timer),
      [id, aCallback, aData](nsITimer* aTimer) {
        if (!aCallback(aData)) {
          aTimer->Cancel();
          Timers().Remove(id);
        }
      },
      aMilliseconds, nsITimer::TYPE_REPEATING_SLACK, "BandeTimeout"_ns);
  if (NS_FAILED(rv)) {
    return 0;
  }
  Timers().InsertOrUpdate(id, timer);
  return id;
}

void BandeTimeoutRemove(uint32_t aTimer) {
  if (auto timer = Timers().Extract(aTimer)) {
    (*timer)->Cancel();
  }
}

}  // namespace mozilla::widget
