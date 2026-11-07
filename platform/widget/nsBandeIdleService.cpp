/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsBandeIdleService.h"

already_AddRefed<nsBandeIdleService> nsBandeIdleService::GetInstance() {
  RefPtr<nsBandeIdleService> service =
      nsUserIdleService::GetInstance().downcast<nsBandeIdleService>();
  if (!service && !mozilla::AppShutdown::IsInOrBeyond(
                      mozilla::ShutdownPhase::AppShutdownConfirmed)) {
    service = new nsBandeIdleService();
  }
  return service.forget();
}
